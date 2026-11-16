//! The extension lines of the spec-kit checks: the `git` extension, the
//! hooks a member must run, and the runtime events spec-kit wrote. Each
//! reads the commit, through `Base`.

use super::Base;
use crate::cmd::doctor::Check;
use crate::cmd::doctor_speckit_files::{
    declares_events, has_speckit_event, installed_extensions, required_hooks,
};

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

/// The integrations the committed `integration.json` lists, for the action.
fn integration_keys(base: &Base<'_>) -> Vec<String> {
    base.show(super::INTEGRATION_FILE)
        .ok()
        .flatten()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .map(|json| super::installed_integrations(&json))
        .unwrap_or_default()
}

/// A spec-kit runtime event runs a spec-kit script from the agent's own hook
/// system, in every member session, with no member asking for it. Only the
/// hooks spec-kit really wrote count: a manifest that declares `events:` sets
/// up nothing while the extension is disabled or the integration runs with
/// `--events false`.
pub(super) fn events_check(base: &Base<'_>) -> Check {
    let name = "spec-kit events";
    let at = base.at();
    let mut declared: Vec<&str> = base
        .under(".specify/extensions")
        .filter_map(|rest| rest.strip_suffix("/extension.yml"))
        .filter(|id| !id.contains('/'))
        .filter(|id| {
            let file = format!(".specify/extensions/{id}/extension.yml");
            base.show(&file)
                .ok()
                .flatten()
                .is_some_and(|text| declares_events(&text))
        })
        .collect();
    declared.sort_unstable();
    let mut hooks = Vec::new();
    let settings = ".claude/settings.json";
    if base
        .show(settings)
        .ok()
        .flatten()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .is_some_and(|json| has_speckit_event(&json))
    {
        hooks.push(format!(
            "{settings} has a hook tagged \"__speckit_event__\": true"
        ));
    }
    let config = ".codex/config.toml";
    if base
        .show(config)
        .ok()
        .flatten()
        .is_some_and(|text| text.contains("speckit_marker = true"))
    {
        hooks.push(format!("{config} has a hook marked speckit_marker = true"));
    }
    let dispatcher = ".specify/events.py";
    if base.has(dispatcher) {
        hooks.push(format!("{dispatcher} is the script those hooks run"));
    }
    // The two files Codex launch refuses (`teamwire-harness-codex` `OWN_CONFIG_FILES`),
    // committed or only on disk.
    let mut codex = Vec::new();
    let mut codex_committed = false;
    for file in [config, ".codex/hooks.json"] {
        if base.has(file) {
            codex_committed = true;
            codex.push(format!("{file} is in {at}"));
        } else if base.root.join(file).exists() {
            codex.push(format!("{file} is on disk but not in {at}"));
        }
    }
    if hooks.is_empty() && codex.is_empty() {
        let note = if declared.is_empty() {
            String::new()
        } else {
            format!(
                "; {} declares events:, but spec-kit wrote no hook for them (a disabled \
                 extension, or an integration with --events false)",
                declared.join(", ")
            )
        };
        return Check::pass(
            name,
            format!("no spec-kit runtime event is set up in {at}{note}"),
        );
    }
    let mut found = Vec::new();
    let mut actions = Vec::new();
    if !hooks.is_empty() {
        found.push(format!(
            "in {at}, {}; each runs a spec-kit script from the agent's own hooks in every \
             member session",
            hooks.join(", ")
        ));
        let mut keys = integration_keys(base);
        if keys.is_empty() {
            keys.push("<key>".to_owned());
        }
        let upgrade = keys
            .iter()
            .map(|k| {
                format!("`specify integration upgrade {k} --force --integration-options=\"--events false\"`")
            })
            .collect::<Vec<_>>()
            .join(" and ");
        actions.push(if declared.is_empty() {
            format!("run {upgrade}")
        } else {
            let ids = declared.join(" ");
            format!(
                "run `specify extension remove {ids}` (or `specify extension disable {ids}`), or {upgrade}"
            )
        });
    }
    if !codex.is_empty() {
        found.push(format!(
            "{}, and teamwire refuses to start a Codex member while it is",
            codex.join(", ")
        ));
        if hooks.is_empty() {
            actions.push("remove it from the repository".to_owned());
        }
    }
    // A file only on disk was never committed, so removing it needs no commit.
    let then = if hooks.is_empty() && !codex_committed {
        String::new()
    } else {
        format!(", then commit on {}", base.name)
    };
    Check::warn(
        name,
        format!("{}; {}{then}", found.join("; "), actions.join(", ")),
    )
}
