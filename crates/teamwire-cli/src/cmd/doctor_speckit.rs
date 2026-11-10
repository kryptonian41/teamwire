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
use crate::cmd::doctor_speckit_files::{has_speckit_event, installed_extensions};

const GIT_TIMEOUT: Duration = Duration::from_secs(10);

const CONSTITUTION: &str = ".specify/memory/constitution.md";
const FEATURE_POINTER: &str = ".specify/feature.json";

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
        committed_check(&base),
        constitution_check(&base),
        integrations_check(&base),
        git_extension_check(&base),
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
    let cmd = Command::new("git")
        .args(args.iter().copied())
        .cwd(root.to_path_buf());
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
}

/// A worktree is cut from a commit, so a `.specify/` that is only on disk or
/// only staged leaves a worktree member with no spec-kit files.
fn committed_check(base: &Base<'_>) -> Check {
    let name = "spec-kit committed";
    let at = base.at();
    if base.has(CONSTITUTION) {
        Check::pass(name, format!("{CONSTITUTION} is committed in {at}"))
    } else {
        Check::warn(
            name,
            format!(
                "{CONSTITUTION} is not in {at}, so a worktree member has no spec-kit files; \
                 commit .specify/ and the agents' spec-kit command folders on {}",
                base.name
            ),
        )
    }
}

/// spec-kit rewrites its feature pointer on every feature switch, so each
/// member's copy differs and must stay out of commits. Ignore rules apply to
/// the disk, so this line asks git about the working tree.
fn feature_pointer_check(base: &Base<'_>) -> Check {
    let name = "spec-kit feature pointer";
    let hint = git_status_hint(&base.root);
    match base.git(&["check-ignore", "-q", FEATURE_POINTER]) {
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
                "git check-ignore could not tell whether it ignores {FEATURE_POINTER} ({}); {hint}",
                git_said(&out)
            ),
        ),
        Err(e) => Check::warn(
            name,
            format!("could not ask git whether it ignores {FEATURE_POINTER}: {e}; {hint}"),
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

/// True when the commit holds an entry of `dir` whose name has the start and
/// end: a file, or a folder of files.
fn has_entry(base: &Base<'_>, dir: &str, start: &str, end: &str) -> bool {
    base.under(dir).any(|rest| {
        let entry = rest.split('/').next().unwrap_or(rest);
        entry.starts_with(start) && entry.ends_with(end)
    })
}

fn integrations_check(base: &Base<'_>) -> Check {
    let name = "spec-kit integrations";
    let file = ".specify/integration.json";
    let at = base.at();
    let installed: Vec<String> = match base.show(file).and_then(|text| {
        let text = text.ok_or_else(|| format!("{file} is not in {at}"))?;
        serde_json::from_str::<serde_json::Value>(&text)
            .map_err(|e| format!("could not read {file} in {at}: {e}"))
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
                format!("{e}; run `specify integration status` to repair it"),
            );
        }
    };
    let missing: Vec<String> = installed
        .iter()
        .filter_map(|key| {
            let (dir, start, end) = command_folder(key)?;
            (!has_entry(base, dir, start, end)).then(|| {
                format!(
                    "{key} has no {dir}/{start}*{end} in {at}; run `specify integration upgrade \
                     {key}`, then commit the folder on {}",
                    base.name
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
        Check::pass(name, format!("installed in {at}: {list}"))
    } else {
        Check::warn(
            name,
            format!("{file} in {at} lists {list}, but {}", missing.join("; ")),
        )
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

/// spec-kit's `git` extension makes branches per feature; teamwire gives each
/// member its own branch and worktree, so the two would fight.
fn git_extension_check(base: &Base<'_>) -> Check {
    let name = "spec-kit git extension";
    let file = ".specify/extensions.yml";
    let at = base.at();
    let text = match base.show(file) {
        Ok(text) => text.unwrap_or_default(),
        Err(e) => return Check::warn(name, e),
    };
    if installed_extensions(&text).iter().any(|id| id == "git") {
        Check::warn(
            name,
            format!(
                "{file} in {at} lists the git extension under installed:, but teamwire owns \
                 branches and worktrees; run `specify extension remove git`, then commit on {}",
                base.name
            ),
        )
    } else {
        Check::pass(name, format!("the git extension is not installed in {at}"))
    }
}

/// A spec-kit runtime event runs a spec-kit script from the agent's own hook
/// system, in every member session, with no member asking for it.
fn events_check(base: &Base<'_>) -> Check {
    let name = "spec-kit events";
    let at = base.at();
    let mut found = Vec::new();
    let mut ids: Vec<&str> = base
        .under(".specify/extensions")
        .filter_map(|rest| rest.strip_suffix("/extension.yml"))
        .filter(|id| !id.contains('/'))
        .collect();
    ids.sort_unstable();
    for id in ids {
        let file = format!(".specify/extensions/{id}/extension.yml");
        let text = base.show(&file).ok().flatten().unwrap_or_default();
        // shortcut: a top-level key is a line that starts with `events:`, not a YAML parse.
        if text.lines().any(|l| l.starts_with("events:")) {
            found.push(format!("{file} declares events:"));
        }
    }
    if base.has(".specify/integration-events.yml") {
        found.push(".specify/integration-events.yml exists".to_owned());
    }
    // The two files Codex launch refuses (`teamwire-harness-codex` `OWN_CONFIG_FILES`).
    for file in [".codex/config.toml", ".codex/hooks.json"] {
        if base.has(file) {
            found.push(format!(
                "{file} exists, so teamwire refuses to start a Codex member here"
            ));
        }
    }
    let settings = ".claude/settings.json";
    if base
        .show(settings)
        .ok()
        .flatten()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .is_some_and(|json| has_speckit_event(&json))
    {
        found.push(format!(
            "{settings} has a hook tagged \"__speckit_event__\": true"
        ));
    }
    if found.is_empty() {
        return Check::pass(name, format!("no spec-kit runtime event is set up in {at}"));
    }
    Check::warn(
        name,
        format!(
            "in {at}: {}; each runs a spec-kit script from the agent's own hooks in every member \
             session. Remove it, or reinstall the integration with \
             `--integration-options=\"--events false\"`",
            found.join("; ")
        ),
    )
}
