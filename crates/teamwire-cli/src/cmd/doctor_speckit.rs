//! The spec-kit checks of `teamwire doctor`, printed only when doctor runs
//! inside a git repository whose root holds `.specify/`.
//!
//! Never: run `specify`, use the network, or change a file. The checks read
//! files and ask git (`ls-files`, `check-ignore`).
//! Every line is `ok` or `warn`, never `FAIL`: a doctor failure blocks
//! `teamwire afk start`, and none of these stops a launch on its own. Each
//! warning names the file and what to do.

use std::path::{Path, PathBuf};
use std::time::Duration;

use teamwire_core::Command;

use crate::cmd::doctor::{Check, Inputs};

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
        constitution_check(&root),
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

/// Runs `git args` at the root and returns its exit code, or why it gave none.
fn git(inputs: &Inputs<'_>, root: &Path, args: &[&str]) -> Result<i32, String> {
    let cmd = Command::new("git")
        .args(args.iter().copied())
        .cwd(root.to_path_buf());
    match inputs.platform.run(&cmd, GIT_TIMEOUT) {
        Ok(out) if out.timed_out => Err(format!("no answer within {} s", GIT_TIMEOUT.as_secs())),
        Ok(out) => Ok(out.exit_code),
        Err(e) => Err(e.to_string()),
    }
}

/// A worktree holds only what is committed, so an untracked `.specify/`
/// leaves a worktree member with no spec-kit files.
fn committed_check(inputs: &Inputs<'_>, root: &Path) -> Check {
    let name = "spec-kit committed";
    match git(
        inputs,
        root,
        &["ls-files", "--error-unmatch", "--", CONSTITUTION],
    ) {
        Ok(0) => Check::pass(name, format!("{CONSTITUTION} is tracked by git")),
        Ok(1) => Check::warn(
            name,
            format!(
                "{CONSTITUTION} is not tracked by git, so a worktree member has no spec-kit files; \
                 commit .specify/ and the agents' spec-kit command folders"
            ),
        ),
        Ok(code) => Check::warn(name, format!("git ls-files exited with code {code}")),
        Err(e) => Check::warn(name, format!("could not run git: {e}")),
    }
}

/// spec-kit rewrites its feature pointer on every feature switch, so each
/// member's copy differs and must stay out of commits.
fn feature_pointer_check(inputs: &Inputs<'_>, root: &Path) -> Check {
    let name = "spec-kit feature pointer";
    match git(inputs, root, &["check-ignore", "-q", FEATURE_POINTER]) {
        Ok(0) => Check::pass(name, format!("git ignores {FEATURE_POINTER}")),
        Ok(1) => Check::warn(
            name,
            format!(
                "git does not ignore {FEATURE_POINTER}, so members would commit their feature \
                 pointer; add `feature.json` to .specify/.gitignore"
            ),
        ),
        Ok(code) => Check::warn(name, format!("git check-ignore exited with code {code}")),
        Err(e) => Check::warn(name, format!("could not run git: {e}")),
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

fn constitution_check(root: &Path) -> Check {
    let name = "spec-kit constitution";
    match std::fs::read_to_string(root.join(CONSTITUTION)) {
        Ok(text) => match first_placeholder(&text) {
            Some(slot) => Check::warn(
                name,
                format!(
                    "{CONSTITUTION} still holds the template placeholder [{slot}]; \
                     fill it in with /speckit-constitution and commit it"
                ),
            ),
            None => Check::pass(name, "no template placeholder left"),
        },
        Err(e) => Check::warn(
            name,
            format!("could not read {CONSTITUTION}: {e}; write it with /speckit-constitution"),
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
        Err(e) => return Check::warn(name, format!("could not read {file}: {e}")),
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

/// One YAML scalar as spec-kit writes it: maybe quoted, maybe followed by a
/// comment.
fn yaml_scalar(text: &str) -> String {
    let text = text.split(" #").next().unwrap_or(text);
    text.trim().trim_matches(['"', '\'']).to_owned()
}

/// The extension ids under the top-level `installed:` key, written as an
/// inline list (`installed: [bug, git]`) or as `- git` items below it.
fn installed_extensions(text: &str) -> Vec<String> {
    // shortcut: reads `installed:` line by line instead of parsing YAML, so a flow
    // list that spans lines, an anchor or a quoted key is missed; switch to a
    // YAML parser if the workspace gains one or a check needs nested keys.
    let mut lines = text.lines();
    let Some(rest) = lines.find_map(|l| l.strip_prefix("installed:")) else {
        return Vec::new();
    };
    if let Some(inline) = rest.trim().strip_prefix('[') {
        return inline
            .trim_end()
            .trim_end_matches(']')
            .split(',')
            .map(yaml_scalar)
            .filter(|id| !id.is_empty())
            .collect();
    }
    // The block list ends at the next top-level key.
    lines
        .take_while(|l| l.is_empty() || l.starts_with([' ', '-', '#']))
        .filter_map(|l| l.trim_start().strip_prefix('-'))
        .map(yaml_scalar)
        .collect()
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

/// True when a JSON value holds an object with `"__speckit_event__": true`,
/// the tag spec-kit puts on each hook it writes into `.claude/settings.json`.
fn has_speckit_event(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            map.get("__speckit_event__") == Some(&serde_json::Value::Bool(true))
                || map.values().any(has_speckit_event)
        }
        serde_json::Value::Array(items) => items.iter().any(has_speckit_event),
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => false,
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
    if root.join(".codex/config.toml").exists() {
        found.push(
            ".codex/config.toml exists, so teamwire refuses to start a Codex member here"
                .to_owned(),
        );
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
