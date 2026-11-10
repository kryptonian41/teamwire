//! The spec-kit checks of `teamwire doctor`, printed only when doctor runs
//! inside a git repository whose root holds `.specify/`.
//!
//! Never: run `specify`, use the network, or change a file. The checks read
//! files and ask git (`cat-file`, `show`, `check-ignore`). A worktree is cut
//! from a commit, so the constitution checks look at `HEAD`, not at git's
//! index or the disk.
//! Every line is `ok` or `warn`, never `FAIL`: a doctor failure blocks
//! `teamwire afk start`, and none of these stops a launch on its own. Each
//! warning names the file and what to do.

use std::path::{Path, PathBuf};
use std::time::Duration;

use teamwire_core::{Command, RunOutput};

use crate::cmd::doctor::{Check, Inputs};
use crate::cmd::doctor_speckit_files::{has_speckit_event, installed_extensions};

const GIT_TIMEOUT: Duration = Duration::from_secs(10);

const CONSTITUTION: &str = ".specify/memory/constitution.md";
const FEATURE_POINTER: &str = ".specify/feature.json";

/// The spec-kit checks, or none outside a spec-kit repository.
pub(crate) fn speckit_checks(inputs: &Inputs<'_>) -> Vec<Check> {
    let Some(root) = speckit_root(&inputs.repo) else {
        return Vec::new();
    };
    vec![
        committed_check(inputs, &root),
        constitution_check(inputs, &root),
        integrations_check(&root),
        git_extension_check(&root),
        events_check(&root),
        feature_pointer_check(inputs, &root),
        workflows_check(&root),
    ]
}

/// The repository root is the nearest directory at or above `dir` that
/// holds `.git` (a directory, or a file in a worktree). It counts only when
/// it also holds `.specify/`.
fn speckit_root(dir: &Path) -> Option<PathBuf> {
    let root = dir.ancestors().find(|d| d.join(".git").exists())?;
    root.join(".specify").is_dir().then(|| root.to_path_buf())
}

/// Runs `git args` at the root and returns what it printed, or why it gave
/// no answer.
fn git(inputs: &Inputs<'_>, root: &Path, args: &[&str]) -> Result<RunOutput, String> {
    let cmd = Command::new("git")
        .args(args.iter().copied())
        .cwd(root.to_path_buf());
    match inputs.platform.run(&cmd, GIT_TIMEOUT) {
        Ok(out) if out.timed_out => Err(format!("no answer within {} s", GIT_TIMEOUT.as_secs())),
        Ok(out) => Ok(out),
        Err(e) => Err(e.to_string()),
    }
}

/// The first line git wrote to stderr, which says why it refused.
fn git_said(out: &RunOutput) -> String {
    let text = String::from_utf8_lossy(&out.stderr);
    let line = text.lines().next().unwrap_or("").trim();
    if line.is_empty() {
        format!("exit code {}", out.exit_code)
    } else {
        line.to_owned()
    }
}

/// The action for a git that gave no usable answer.
fn git_status_hint(root: &Path) -> String {
    format!("run `git status` in {} to see why", root.display())
}

/// A worktree is cut from a commit, so a `.specify/` that is only on disk or
/// only staged leaves a worktree member with no spec-kit files.
fn committed_check(inputs: &Inputs<'_>, root: &Path) -> Check {
    let name = "spec-kit committed";
    let object = format!("HEAD:{CONSTITUTION}");
    match git(inputs, root, &["cat-file", "-e", &object]) {
        Ok(out) if out.exit_code == 0 => {
            Check::pass(name, format!("{CONSTITUTION} is committed in HEAD"))
        }
        // `cat-file -e` exits 128 for a path not in HEAD and for a
        // repository with no commit yet alike.
        Ok(out) => Check::warn(
            name,
            format!(
                "{CONSTITUTION} is not committed in HEAD ({}), so a worktree member has no \
                 spec-kit files; commit .specify/ and the agents' spec-kit command folders",
                git_said(&out)
            ),
        ),
        Err(e) => Check::warn(
            name,
            format!(
                "could not ask git whether {CONSTITUTION} is committed: {e}; {}",
                git_status_hint(root)
            ),
        ),
    }
}

/// spec-kit rewrites its feature pointer on every feature switch, so each
/// member's copy differs and must stay out of commits.
fn feature_pointer_check(inputs: &Inputs<'_>, root: &Path) -> Check {
    let name = "spec-kit feature pointer";
    match git(inputs, root, &["check-ignore", "-q", FEATURE_POINTER]) {
        Ok(out) if out.exit_code == 0 => {
            Check::pass(name, format!("git ignores {FEATURE_POINTER}"))
        }
        Ok(out) if out.exit_code == 1 => Check::warn(
            name,
            format!(
                "git does not ignore {FEATURE_POINTER}, so members would commit their feature \
                 pointer; add `feature.json` to .specify/.gitignore"
            ),
        ),
        Ok(out) => Check::warn(
            name,
            format!(
                "git check-ignore could not tell whether it ignores {FEATURE_POINTER} ({}); {}",
                git_said(&out),
                git_status_hint(root)
            ),
        ),
        Err(e) => Check::warn(
            name,
            format!(
                "could not ask git whether it ignores {FEATURE_POINTER}: {e}; {}",
                git_status_hint(root)
            ),
        ),
    }
}

/// spec-kit's constitution template marks each slot as `[PROJECT_NAME]`,
/// `[PRINCIPLE_1_NAME]` and so on: capitals, digits and at least one
/// underscore. Task markers such as `[P]`, `[US1]` and `[NEEDS CLARIFICATION]`
/// do not have that shape.
fn first_placeholder(text: &str) -> Option<&str> {
    text.split('[')
        .skip(1)
        .filter_map(|rest| rest.split_once(']').map(|(inside, _)| inside))
        .find(|inside| {
            inside.contains('_')
                && inside.starts_with(|c: char| c.is_ascii_uppercase())
                && inside
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        })
}

/// Members get the constitution in `HEAD`, so that copy is the one read for
/// placeholders. A copy on disk that differs is named, since it is what the
/// lead sees but not what members get.
fn constitution_check(inputs: &Inputs<'_>, root: &Path) -> Check {
    let name = "spec-kit constitution";
    let object = format!("HEAD:{CONSTITUTION}");
    let committed = match git(inputs, root, &["show", &object]) {
        Ok(out) if out.exit_code == 0 => String::from_utf8_lossy(&out.stdout).into_owned(),
        Ok(out) => {
            return Check::warn(
                name,
                format!(
                    "{CONSTITUTION} is not committed in HEAD ({}); write it with \
                     /speckit-constitution and commit it",
                    git_said(&out)
                ),
            );
        }
        Err(e) => {
            return Check::warn(
                name,
                format!(
                    "could not ask git for {CONSTITUTION} in HEAD: {e}; {}",
                    git_status_hint(root)
                ),
            );
        }
    };
    let on_disk = std::fs::read_to_string(root.join(CONSTITUTION)).ok();
    let differs = if on_disk.as_deref() == Some(committed.as_str()) {
        ""
    } else {
        "; the copy on disk differs from HEAD, and members get the committed one"
    };
    match first_placeholder(&committed) {
        Some(slot) => Check::warn(
            name,
            format!(
                "{CONSTITUTION} in HEAD still holds the template placeholder [{slot}]; \
                 fill it in with /speckit-constitution and commit it{differs}"
            ),
        ),
        None => Check::pass(
            name,
            format!("no template placeholder left in HEAD{differs}"),
        ),
    }
}

/// Where each integration teamwire can run keeps its spec-kit commands: a
/// folder, and the start and end of an entry's name in it. Another
/// integration is listed without a folder check.
fn command_folder(integration: &str) -> Option<(&'static str, &'static str, &'static str)> {
    match integration {
        "claude" => Some((".claude/skills", "speckit-", "")),
        "codex" => Some((".agents/skills", "speckit-", "")),
        "pi" => Some((".pi/prompts", "speckit.", ".md")),
        _ => None,
    }
}

fn has_entry(dir: &Path, start: &str, end: &str) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.starts_with(start) && n.ends_with(end))
        })
    })
}

fn integrations_check(root: &Path) -> Check {
    let name = "spec-kit integrations";
    let file = ".specify/integration.json";
    let installed: Vec<String> = match std::fs::read_to_string(root.join(file))
        .map_err(|e| e.to_string())
        .and_then(|text| {
            serde_json::from_str::<serde_json::Value>(&text).map_err(|e| e.to_string())
        }) {
        Ok(json) => json
            .get("installed_integrations")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        Err(e) => {
            return Check::warn(
                name,
                format!(
                    "could not read {file}: {e}; run `specify integration status` to repair it"
                ),
            );
        }
    };
    let missing: Vec<String> = installed
        .iter()
        .filter_map(|key| {
            let (dir, start, end) = command_folder(key)?;
            (!has_entry(&root.join(dir), start, end)).then(|| {
                format!(
                    "{key} has no {dir}/{start}*{end}; run `specify integration upgrade {key}` \
                     and commit the folder"
                )
            })
        })
        .collect();
    let list = if installed.is_empty() {
        "none".to_owned()
    } else {
        installed.join(", ")
    };
    if missing.is_empty() {
        Check::pass(name, format!("installed: {list}"))
    } else {
        Check::warn(
            name,
            format!("{file} lists {list}, but {}", missing.join("; ")),
        )
    }
}

/// Always `ok`: a spec-kit workflow is something a member could start by
/// hand, never something teamwire runs.
fn workflows_check(root: &Path) -> Check {
    let name = "spec-kit workflows";
    let file = ".specify/workflows/workflow-registry.json";
    let ids: Vec<String> = std::fs::read_to_string(root.join(file))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|json| {
            json.get("workflows")?
                .as_object()
                .map(|m| m.keys().cloned().collect())
        })
        .unwrap_or_default();
    let list = if ids.is_empty() {
        "none".to_owned()
    } else {
        ids.join(", ")
    };
    Check::pass(
        name,
        format!("installed: {list}; teamwire starts none of them"),
    )
}

/// spec-kit's `git` extension makes branches per feature; teamwire gives each
/// member its own branch and worktree, so the two would fight.
fn git_extension_check(root: &Path) -> Check {
    let name = "spec-kit git extension";
    let file = ".specify/extensions.yml";
    let text = std::fs::read_to_string(root.join(file)).unwrap_or_default();
    if installed_extensions(&text).iter().any(|id| id == "git") {
        Check::warn(
            name,
            format!(
                "{file} lists the git extension under installed:, but teamwire owns branches and \
                 worktrees; run `specify extension remove git`"
            ),
        )
    } else {
        Check::pass(name, "the git extension is not installed")
    }
}

/// A spec-kit runtime event runs a spec-kit script from the agent's own hook
/// system, in every member session, with no member asking for it.
fn events_check(root: &Path) -> Check {
    let name = "spec-kit events";
    let mut found = Vec::new();
    let extensions = root.join(".specify/extensions");
    let mut ids: Vec<String> = std::fs::read_dir(&extensions)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    ids.sort();
    for id in ids {
        let file = format!(".specify/extensions/{id}/extension.yml");
        let text = std::fs::read_to_string(root.join(&file)).unwrap_or_default();
        // shortcut: a top-level key is a line that starts with `events:`, not a YAML parse.
        if text.lines().any(|l| l.starts_with("events:")) {
            found.push(format!("{file} declares events:"));
        }
    }
    if root.join(".specify/integration-events.yml").exists() {
        found.push(".specify/integration-events.yml exists".to_owned());
    }
    // The two files Codex launch refuses (`teamwire-harness-codex` `OWN_CONFIG_FILES`).
    for file in [".codex/config.toml", ".codex/hooks.json"] {
        if root.join(file).exists() {
            found.push(format!(
                "{file} exists, so teamwire refuses to start a Codex member here"
            ));
        }
    }
    let settings = ".claude/settings.json";
    if std::fs::read_to_string(root.join(settings))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .is_some_and(|json| has_speckit_event(&json))
    {
        found.push(format!(
            "{settings} has a hook tagged \"__speckit_event__\": true"
        ));
    }
    if found.is_empty() {
        return Check::pass(name, "no spec-kit runtime event is set up");
    }
    Check::warn(
        name,
        format!(
            "{}; each runs a spec-kit script from the agent's own hooks in every member session. \
             Remove it, or reinstall the integration with `--integration-options=\"--events false\"`",
            found.join("; ")
        ),
    )
}
