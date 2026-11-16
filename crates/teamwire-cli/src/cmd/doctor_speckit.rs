//! The spec-kit checks of `teamwire doctor`, printed only when doctor runs
//! inside a git repository whose root holds `.specify/`, on disk or in the
//! commit that `--base` names.
//!
//! Never: run `specify`, use the network, or change a file. The checks ask
//! git (`rev-parse`, `ls-tree`, `show`, `check-ignore`) and read files. A
//! worktree is cut from a commit, so every line reads that commit, never
//! git's index or the disk, unless its comment says why it reads the disk.
//! Every line is `ok` or `warn`, never `FAIL`, and names the ref and its
//! short commit id. Each warning names the file and one action that fixes it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use teamwire_core::{Command, RunOutput};

use crate::cmd::doctor::{Check, Inputs};
// A path, so the test crates that include this file by path find it too.
#[path = "doctor_speckit/committed.rs"]
mod committed;
#[path = "doctor_speckit/extensions.rs"]
mod extensions;
#[path = "doctor_speckit/integrations.rs"]
mod integrations;

use committed::committed_check;
use extensions::{events_check, git_extension_check, hooks_check};
use integrations::{command_folders, installed_integrations, integrations_check, version_check};

const GIT_TIMEOUT: Duration = Duration::from_secs(10);

const CONSTITUTION: &str = ".specify/memory/constitution.md";
const FEATURE_POINTER: &str = ".specify/feature.json";
const INTEGRATION_FILE: &str = ".specify/integration.json";

/// The folders `ls-tree` lists: spec-kit's own, and where the harnesses
/// teamwire runs keep spec-kit's commands and hooks.
const LISTED: [&str; 5] = [".specify", ".claude", ".agents", ".pi", ".codex"];

/// The spec-kit checks, or none outside a spec-kit repository.
pub(crate) fn speckit_checks(inputs: &Inputs<'_>) -> Vec<Check> {
    let Some(root) = git_root(&inputs.repo) else {
        return Vec::new();
    };
    let on_disk = root.join(".specify").is_dir();
    let base = match Base::read(inputs, &root) {
        Ok(base) => base,
        Err(why) if on_disk => return vec![Check::warn("spec-kit base", why)],
        Err(_) => return Vec::new(),
    };
    if !on_disk && !base.has_under(".specify") {
        return Vec::new();
    }
    vec![
        version_check(&base),
        committed_check(&base),
        constitution_check(&base),
        integrations_check(&base),
        git_extension_check(&base),
        hooks_check(&base),
        events_check(&base),
        feature_pointer_check(&base),
        workflows_check(&base),
    ]
}

/// The repository root is the nearest directory at or above `dir` that
/// holds `.git` (a directory, or a file in a worktree).
fn git_root(dir: &Path) -> Option<PathBuf> {
    dir.ancestors()
        .find(|d| d.join(".git").exists())
        .map(Path::to_path_buf)
}

/// Runs `git args` at the root and returns what it printed, or why it gave
/// no answer.
fn git(inputs: &Inputs<'_>, root: &Path, args: &[&str]) -> Result<RunOutput, String> {
    git_with(inputs, root, args, None)
}

fn git_with(
    inputs: &Inputs<'_>,
    root: &Path,
    args: &[&str],
    stdin: Option<Vec<u8>>,
) -> Result<RunOutput, String> {
    let mut cmd = Command::new("git")
        .args(args.iter().copied())
        .cwd(root.to_path_buf());
    if let Some(bytes) = stdin {
        cmd = cmd.stdin(bytes);
    }
    match inputs.git.run(&cmd, GIT_TIMEOUT) {
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

/// The commit `--base` names, and the spec-kit paths in it.
pub(crate) struct Base<'a> {
    inputs: &'a Inputs<'a>,
    pub(crate) root: PathBuf,
    /// The ref as the lead named it.
    pub(crate) name: String,
    sha: String,
    files: BTreeSet<String>,
    /// True when the ref is the checked-out branch, so a commit here lands on it.
    pub(crate) checked_out: bool,
}

impl<'a> Base<'a> {
    fn read(inputs: &'a Inputs<'a>, root: &Path) -> Result<Self, String> {
        let name = inputs.base.clone();
        let hint = git_status_hint(root);
        let spec = format!("{name}^{{commit}}");
        let out = git(
            inputs,
            root,
            &["rev-parse", "--verify", "--end-of-options", &spec],
        )
        .map_err(|e| format!("could not ask git for the commit `{name}` names: {e}; {hint}"))?;
        if out.exit_code != 0 {
            let said = git_said(&out);
            return Err(if said.contains("Needed a single revision") {
                format!(
                    "git cannot resolve `{name}` to a commit ({said}), so no spec-kit line can \
                     read what members start from; pass `--base` a branch or commit that \
                     exists, or make the first commit"
                )
            } else {
                format!("git could not resolve `{name}` ({said}); {hint}")
            });
        }
        let sha = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        let checked_out = name == "HEAD"
            || git(inputs, root, &["symbolic-ref", "-q", "--short", "HEAD"]).is_ok_and(|out| {
                out.exit_code == 0 && String::from_utf8_lossy(&out.stdout).trim() == name
            });
        let mut args = vec!["ls-tree", "-r", "-z", "--name-only", sha.as_str(), "--"];
        args.extend(LISTED);
        let out = git(inputs, root, &args)
            .map_err(|e| format!("could not ask git for the files in `{name}`: {e}; {hint}"))?;
        if out.exit_code != 0 {
            return Err(format!(
                "git could not list the files in `{name}` ({}); {hint}",
                git_said(&out)
            ));
        }
        let files = String::from_utf8_lossy(&out.stdout)
            .split('\0')
            .filter(|f| !f.is_empty())
            .map(str::to_owned)
            .collect();
        Ok(Self {
            inputs,
            root: root.to_path_buf(),
            name,
            sha,
            files,
            checked_out,
        })
    }

    /// `HEAD (b529e55)`: the ref and its short commit id.
    pub(crate) fn at(&self) -> String {
        format!("{} ({})", self.name, self.sha.get(..7).unwrap_or(&self.sha))
    }

    pub(crate) fn has(&self, path: &str) -> bool {
        self.files.contains(path)
    }

    /// The committed files under the folder `dir`.
    pub(crate) fn under<'s>(&'s self, dir: &'s str) -> impl Iterator<Item = &'s str> + 's {
        self.files
            .iter()
            .filter_map(move |f| f.strip_prefix(dir)?.strip_prefix('/'))
    }

    fn has_under(&self, dir: &str) -> bool {
        self.under(dir).next().is_some()
    }

    /// The committed text of `path`, or `None` when the commit lacks it.
    pub(crate) fn show(&self, path: &str) -> Result<Option<String>, String> {
        if !self.has(path) {
            return Ok(None);
        }
        let object = format!("{}:{path}", self.sha);
        let at = self.at();
        match git(self.inputs, &self.root, &["show", &object]) {
            Ok(out) if out.exit_code == 0 => {
                Ok(Some(String::from_utf8_lossy(&out.stdout).into_owned()))
            }
            Ok(out) => Err(format!(
                "git could not read {path} in {at} ({}); {}",
                git_said(&out),
                git_status_hint(&self.root)
            )),
            Err(e) => Err(format!(
                "could not ask git for {path} in {at}: {e}; {}",
                git_status_hint(&self.root)
            )),
        }
    }

    /// `git args` at the root, for the lines that compare the disk.
    pub(crate) fn git(&self, args: &[&str]) -> Result<RunOutput, String> {
        git(self.inputs, &self.root, args)
    }

    pub(crate) fn git_stdin(&self, args: &[&str], stdin: Vec<u8>) -> Result<RunOutput, String> {
        git_with(self.inputs, &self.root, args, Some(stdin))
    }

    pub(crate) fn sha(&self) -> &str {
        &self.sha
    }
}

/// spec-kit rewrites its feature pointer on every feature switch, so each
/// member's copy differs and must stay out of commits. Ignore rules apply to
/// the disk, so this line asks git about the working tree. `--no-index`
/// finds the rule even for a tracked file, which plain `check-ignore` skips.
/// Whether the pointer is tracked comes from the commit, so every form names it.
fn feature_pointer_check(base: &Base<'_>) -> Check {
    let name = "spec-kit feature pointer";
    let hint = git_status_hint(&base.root);
    let at = base.at();
    match base.git(&["check-ignore", "-q", "--no-index", FEATURE_POINTER]) {
        Ok(out) if out.exit_code == 0 && base.has(FEATURE_POINTER) => Check::warn(
            name,
            format!(
                "{FEATURE_POINTER} is tracked in {at}, and git ignores only untracked files, so \
                 members would commit their feature pointer; run `git rm --cached \
                 {FEATURE_POINTER}`, then commit on {}",
                base.name
            ),
        ),
        Ok(out) if out.exit_code == 0 => Check::pass(
            name,
            format!("git ignores {FEATURE_POINTER}, which is not tracked in {at}"),
        ),
        Ok(out) if out.exit_code == 1 => Check::warn(
            name,
            format!(
                "git does not ignore {FEATURE_POINTER}, so members who start from {at} would \
                 commit their feature pointer; add `feature.json` to .specify/.gitignore, then \
                 commit on {}",
                base.name
            ),
        ),
        Ok(out) => Check::warn(
            name,
            format!(
                "git check-ignore could not tell whether it ignores {FEATURE_POINTER} ({}), so \
                 doctor cannot tell whether members who start from {at} would commit it; {hint}",
                git_said(&out)
            ),
        ),
        Err(e) => Check::warn(
            name,
            format!(
                "could not ask git whether it ignores {FEATURE_POINTER}: {e}, so doctor cannot \
                 tell whether members who start from {at} would commit it; {hint}"
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

/// Members get the committed constitution, so that copy is the one read for
/// placeholders. A copy on disk that differs is named, since it is what the
/// lead sees but not what members get.
fn constitution_check(base: &Base<'_>) -> Check {
    let name = "spec-kit constitution";
    let at = base.at();
    let committed = match base.show(CONSTITUTION) {
        Ok(Some(text)) => text,
        Ok(None) => {
            return Check::warn(
                name,
                format!(
                    "{CONSTITUTION} is not in {at}; write it with /speckit-constitution and \
                     commit it on {}",
                    base.name
                ),
            );
        }
        Err(e) => return Check::warn(name, e),
    };
    let on_disk = std::fs::read_to_string(base.root.join(CONSTITUTION)).ok();
    let differs = if on_disk.as_deref() == Some(committed.as_str()) {
        String::new()
    } else {
        format!("; the copy on disk differs from {at}, and members get the committed one")
    };
    match first_placeholder(&committed) {
        Some(slot) => Check::warn(
            name,
            format!(
                "{CONSTITUTION} in {at} still holds the template placeholder [{slot}]; \
                 fill it in with /speckit-constitution and commit it on {}{differs}",
                base.name
            ),
        ),
        None => Check::pass(
            name,
            format!("no template placeholder left in {at}{differs}"),
        ),
    }
}

/// Always `ok`: a spec-kit workflow is something a member could start by
/// hand, never something teamwire runs.
fn workflows_check(base: &Base<'_>) -> Check {
    let name = "spec-kit workflows";
    let file = ".specify/workflows/workflow-registry.json";
    let ids: Vec<String> = base
        .show(file)
        .ok()
        .flatten()
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
        format!(
            "installed in {}: {list}; teamwire starts none of them",
            base.at()
        ),
    )
}
