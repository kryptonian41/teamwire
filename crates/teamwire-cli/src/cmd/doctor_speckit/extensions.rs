//! The extension lines of the spec-kit checks: the `git` extension, the
//! hooks a member must run, and the runtime events spec-kit wrote. Each
//! reads the commit, through `Base`.

use super::Base;
use crate::cmd::doctor::Check;
use crate::cmd::doctor_speckit_files::{has_speckit_event, installed_extensions, required_hooks};

const EXTENSIONS: &str = ".specify/extensions.yml";

/// The warning for a YAML file the reader cannot read: it never passes.
fn unreadable_yaml(file: &str, at: &str, why: &str) -> String {
    format!(
        "could not read {file} in {at} ({why}); check the file by hand, since doctor reads only \
         the YAML forms spec-kit writes"
    )
}

/// spec-kit's `git` extension makes branches per feature; teamwire gives each
/// member its own branch and worktree, so the two would fight.
pub(super) fn git_extension_check(base: &Base<'_>) -> Check {
    let name = "spec-kit git extension";
    let file = EXTENSIONS;
    let at = base.at();
    let text = match base.show(file) {
        Ok(text) => text.unwrap_or_default(),
        Err(e) => return Check::warn(name, e),
    };
    let ids = match installed_extensions(&text) {
        Ok(ids) => ids,
        Err(e) => return Check::warn(name, unreadable_yaml(file, &at, &e)),
    };
    if ids.iter().any(|id| id == "git") {
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

/// Every spec-kit command file tells the agent it MUST run a hook with
/// `optional: false` before it goes on. Such a hook may need a person, a
/// pull request or a branch switch that a worktree member cannot give.
pub(super) fn hooks_check(base: &Base<'_>) -> Check {
    let name = "spec-kit hooks";
    let at = base.at();
    let text = match base.show(EXTENSIONS) {
        Ok(text) => text.unwrap_or_default(),
        Err(e) => return Check::warn(name, e),
    };
    let hooks = match required_hooks(&text) {
        Ok(hooks) => hooks,
        Err(e) => return Check::warn(name, unreadable_yaml(EXTENSIONS, &at, &e)),
    };
    if hooks.is_empty() {
        return Check::pass(
            name,
            format!(
                "no enabled extension hook in {at} is required (optional: false), apart from the git extension's"
            ),
        );
    }
    let listed: Vec<String> = hooks
        .iter()
        .map(|h| format!("{} runs {} at {}", h.extension, h.command, h.event))
        .collect();
    let mut extensions: Vec<&str> = hooks.iter().map(|h| h.extension.as_str()).collect();
    extensions.dedup();
    Check::warn(
        name,
        format!(
            "{EXTENSIONS} in {at} has hooks a member must run (optional: false): {}; read what \
             each command runs, then run `specify extension remove {}` or set `optional: true` \
             on the hook in {EXTENSIONS}, then commit on {}",
            listed.join(", "),
            extensions.join("` or `specify extension remove "),
            base.name
        ),
    )
}

/// A spec-kit runtime event runs a spec-kit script from the agent's own hook
/// system, in every member session, with no member asking for it.
pub(super) fn events_check(base: &Base<'_>) -> Check {
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
