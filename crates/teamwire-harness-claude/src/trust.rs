//! The folder-trust step. Two questions, both read-only. Did the user
//! already trust this directory (`~/.claude.json`)? And, when a dialog is
//! on screen, is it the plain one that teamwire may answer?
//!
//! Never answers a dialog that also grants folder-declared commands
//! (INV-25), never answers for a home directory, and never writes
//! `~/.claude.json`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use teamwire_core::{
    HarnessError, HarnessErrorCode, Host, Key, Place, ScreenState, StartupOutcome, TrustPolicy,
};

use crate::screen::{ESCALATION_MARKERS, PLAIN_TRUST_MARKERS, classify, has_any, squash};

/// How many screen lines to read for the dialog.
const TAIL_LINES: u16 = 40;
const HOST_TIMEOUT: Duration = Duration::from_secs(5);

/// What the configuration says about one directory, before Claude starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustCheck {
    /// The user's home directory. Claude Code treats trust there as
    /// session-only and never saves it, so the dialog can never be settled
    /// ahead of time.
    Home,
    /// `hasTrustDialogAccepted` is saved for the directory (or for the
    /// directory that covers it): no dialog will appear.
    Persisted,
    /// No saved trust. A dialog will appear.
    Missing,
}

/// The directories with `hasTrustDialogAccepted: true`, from the text of
/// `.claude.json`. A file that is missing or not JSON trusts nothing.
fn trusted_dirs(claude_json: &Path) -> Vec<PathBuf> {
    let Some(doc) = std::fs::read(claude_json)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
    else {
        return Vec::new();
    };
    let Some(projects) = doc.get("projects").and_then(Value::as_object) else {
        return Vec::new();
    };
    projects
        .iter()
        .filter(|(_, v)| v.get("hasTrustDialogAccepted").and_then(Value::as_bool) == Some(true))
        .flat_map(|(k, _)| {
            let raw = PathBuf::from(k);
            let real = std::fs::canonicalize(&raw).ok();
            [Some(raw), real].into_iter().flatten()
        })
        .collect()
}

/// The root of the git repository holding `dir`: the nearest ancestor with a
/// `.git`. For a linked worktree the root is the main checkout, because the
/// worktree's own `.git` file points into it.
fn repo_root(dir: &Path) -> Option<PathBuf> {
    let root = dir.ancestors().find(|d| d.join(".git").exists())?;
    let dot_git = root.join(".git");
    if dot_git.is_file() {
        let text = std::fs::read_to_string(&dot_git).ok()?;
        let gitdir = PathBuf::from(text.strip_prefix("gitdir:")?.trim());
        // `<main>/.git/worktrees/<name>`
        let worktrees = gitdir.parent()?;
        if worktrees.file_name().is_some_and(|n| n == "worktrees") {
            let main = worktrees.parent()?.parent()?;
            return Some(std::fs::canonicalize(main).unwrap_or_else(|_| main.to_path_buf()));
        }
    }
    Some(root.to_path_buf())
}

/// Reads `claude_json` for `cwd`, as `team-launch.py` `check_trust` does.
///
/// A git repository is a trust boundary: a trusted parent directory does not
/// cover a repository nested in it, so inside a repository only the
/// repository root (or `cwd` itself) counts. A plain directory inherits
/// trust from any ancestor.
pub(crate) fn check(claude_json: &Path, cwd: &Path) -> TrustCheck {
    let cwd = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    if let Some(home) = claude_json.parent() {
        let home = std::fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf());
        if cwd == home {
            return TrustCheck::Home;
        }
    }
    let trusted = trusted_dirs(claude_json);
    let covered = match repo_root(&cwd) {
        Some(root) => trusted.contains(&cwd) || trusted.contains(&root),
        None => cwd.ancestors().any(|d| trusted.iter().any(|t| t == d)),
    };
    if covered {
        TrustCheck::Persisted
    } else {
        TrustCheck::Missing
    }
}

/// How long `answer_startup` waits: `steps` reads, `step` apart. A count of
/// sleeps bounds the wait because the clock is not readable here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupPoll {
    pub step: Duration,
    pub steps: u32,
}

impl Default for StartupPoll {
    /// About 30 s in half-second steps.
    fn default() -> Self {
        Self {
            step: Duration::from_millis(500),
            steps: 60,
        }
    }
}

fn read(host: &dyn Host, place: &Place) -> Result<String, HarnessError> {
    Ok(host.read_tail(place, TAIL_LINES, HOST_TIMEOUT)?)
}

/// Watches the member's screen until it shows the input prompt, the plain
/// trust dialog (answered when `policy` allows) or an escalated dialog
/// (never answered). A folder with saved trust shows no dialog, so it ends
/// as `NoDialog`.
pub(crate) fn answer_startup(
    host: &dyn Host,
    place: &Place,
    policy: TrustPolicy,
    poll: StartupPoll,
) -> Result<StartupOutcome, HarnessError> {
    for _ in 0..poll.steps {
        let raw = read(host, place)?;
        let tail = squash(&raw);
        // The stronger dialog reuses the plain dialog's title and its "Yes, I
        // trust this folder" button, so escalation is checked first and
        // refused whatever the policy says.
        if has_any(&tail, &ESCALATION_MARKERS) {
            return Ok(StartupOutcome::RefusedEscalated);
        }
        if has_any(&tail, &PLAIN_TRUST_MARKERS) {
            return match policy {
                TrustPolicy::Refuse => Err(HarnessError::new(
                    HarnessErrorCode::Refused,
                    "the spec does not accept the folder-trust dialog",
                )),
                TrustPolicy::Accept => accept(host, place, poll),
            };
        }
        if classify(&raw) == ScreenState::InputPrompt {
            return Ok(StartupOutcome::NoDialog);
        }
        std::thread::sleep(poll.step);
    }
    Err(timeout(
        "neither the input prompt nor a trust dialog appeared",
    ))
}

/// The screen below the last line of the trust dialog. The dialog's text can
/// stay in the scrollback above the prompt that follows it, and the
/// classifier reads any dialog text near the end as an open dialog, so the
/// confirmation looks only at what the dialog left below it.
fn after_dialog(screen: &str) -> String {
    let lines: Vec<&str> = screen.lines().collect();
    let last = lines.iter().rposition(|l| {
        let l = squash(l);
        has_any(&l, &PLAIN_TRUST_MARKERS) || has_any(&l, &ESCALATION_MARKERS)
    });
    let below = last.map_or(&lines[..], |i| lines.get(i + 1..).unwrap_or_default());
    below.join("\n")
}

fn timeout(what: &str) -> HarnessError {
    HarnessError::new(HarnessErrorCode::Timeout, what)
}

fn accept(
    host: &dyn Host,
    place: &Place,
    poll: StartupPoll,
) -> Result<StartupOutcome, HarnessError> {
    // "No, exit" is the preselected line, so Down moves to "Yes, I trust
    // this folder". Never `1`: it does not pick the first line.
    host.send_keys(place, &[Key::Down], HOST_TIMEOUT)?;
    host.send_keys(place, &[Key::Enter], HOST_TIMEOUT)?;
    for _ in 0..poll.steps {
        if classify(&after_dialog(&read(host, place)?)) == ScreenState::InputPrompt {
            return Ok(StartupOutcome::AnsweredPlainTrust);
        }
        std::thread::sleep(poll.step);
    }
    Err(timeout(
        "the input prompt did not appear after the trust answer",
    ))
}
