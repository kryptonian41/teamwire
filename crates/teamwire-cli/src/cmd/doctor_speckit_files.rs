//! The file readers of the spec-kit doctor checks: the `installed:` list and
//! the hooks of `.specify/extensions.yml`, the `events:` of a manifest, and
//! the spec-kit tag in `.claude/settings.json`.
//!
//! Never: write a file or run a program. Pure functions over text and JSON.
//! A YAML form the reader does not read is an error, never an empty answer,
//! so a check can tell "nothing there" from "could not read".

// A path, so the test crates that include this file by path find it too.
#[path = "doctor_speckit/yaml.rs"]
mod yaml;

pub(crate) use yaml::{Yaml, parse_yaml};

/// The extension ids under the top-level `installed:` key: each item is an
/// id, or a mapping with an `id`, as spec-kit reads them.
pub(crate) fn installed_extensions(text: &str) -> Result<Vec<String>, String> {
    let doc = parse_yaml(text)?;
    let Some(Yaml::List(items)) = doc.get("installed") else {
        return Ok(Vec::new());
    };
    Ok(items
        .iter()
        .filter_map(|item| item.text().or_else(|| item.get("id")?.text()))
        .map(|id| id.trim().to_owned())
        .filter(|id| !id.is_empty())
        .collect())
}

/// A hook a member must run: its extension, event and command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequiredHook {
    pub(crate) extension: String,
    pub(crate) event: String,
    pub(crate) command: String,
}

/// The hooks under `hooks:` that every spec-kit command file tells the agent
/// it MUST run: `optional: false`, not `enabled: false`, and no `condition`
/// (the command files skip a hook with one). The `git` extension's hooks are
/// left out, since its own line covers them.
pub(crate) fn required_hooks(text: &str) -> Result<Vec<RequiredHook>, String> {
    let doc = parse_yaml(text)?;
    let Some(Yaml::Map(events)) = doc.get("hooks") else {
        return Ok(Vec::new());
    };
    let mut found = Vec::new();
    for (event, hooks) in events {
        let Yaml::List(hooks) = hooks else { continue };
        for hook in hooks {
            let field = |key: &str| hook.get(key);
            let required = field("optional").is_some_and(Yaml::is_false)
                && !field("enabled").is_some_and(Yaml::is_false)
                && field("condition").is_none_or(Yaml::is_null);
            let extension = field("extension").and_then(Yaml::text).unwrap_or("?");
            if required && extension != "git" {
                found.push(RequiredHook {
                    extension: extension.to_owned(),
                    event: event.clone(),
                    command: field("command")
                        .and_then(Yaml::text)
                        .unwrap_or("?")
                        .to_owned(),
                });
            }
        }
    }
    Ok(found)
}

/// True when a JSON value holds an object with `"__speckit_event__": true`,
/// the tag spec-kit puts on each hook it writes into `.claude/settings.json`.
pub(crate) fn has_speckit_event(value: &serde_json::Value) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(text: &str) -> Vec<String> {
        installed_extensions(text).unwrap()
    }

    #[test]
    fn the_forms_spec_kit_writes_and_reads_list_git() {
        let git = vec!["git".to_owned()];
        // What `specify extension add git` writes.
        assert_eq!(
            ids("installed:\n- git\nsettings:\n  auto_execute_hooks: true\n"),
            git
        );
        // The five hand-written forms PyYAML reads as listing git.
        assert_eq!(ids("installed: [git] # note\n"), git);
        assert_eq!(ids("\"installed\":\n- git\n"), git);
        assert_eq!(ids("\u{feff}installed:\n- git\n"), git);
        assert_eq!(ids("installed: [bug,\n  git]\n"), vec!["bug", "git"]);
        assert_eq!(ids("installed:\r\n- git\r\nhooks: {}\r\n"), git);
        // Items spec-kit accepts as mappings with an id.
        assert_eq!(ids("installed:\n- id: git\n  version: 1.0.4\n"), git);
        assert_eq!(ids("installed: [{id: git}]\n"), git);
        assert_eq!(ids("installed:\n  - 'git'  # branches\n"), git);
    }

    #[test]
    fn git_elsewhere_and_empty_files_list_nothing() {
        assert_eq!(ids(""), Vec::<String>::new());
        assert_eq!(ids("installed: []\n"), Vec::<String>::new());
        assert_eq!(ids("settings:\n  installed: [git]\n"), Vec::<String>::new());
        assert_eq!(ids("installed:\n- bug\nhooks:\n- git\n"), vec!["bug"]);
    }

    #[test]
    fn a_form_the_reader_does_not_read_is_an_error() {
        for text in [
            "installed: &a [git]\n",
            "installed: *a\n",
            "installed: [git\n",
            "installed: \"git\n",
            "installed:\n\t- git\n",
            "installed: [git]\n---\ninstalled: []\n",
            "installed:\n- git\n   bad: indent\n",
        ] {
            assert!(installed_extensions(text).is_err(), "{text:?}");
        }
    }

    #[test]
    fn hooks_read_as_spec_kit_writes_them() {
        let doc = parse_yaml(
            "hooks:\n  before_tasks:\n  - extension: gate\n    command: speckit.gate.check\n    \
             enabled: true\n    optional: false\n    prompt: 'Run it? # yes'\n    condition: null\n",
        )
        .unwrap();
        let Some(Yaml::List(hooks)) = doc.get("hooks").and_then(|h| h.get("before_tasks")) else {
            unreachable!("{doc:?}");
        };
        let Some(hook) = hooks.first() else {
            unreachable!("{doc:?}");
        };
        assert_eq!(hook.get("extension").and_then(Yaml::text), Some("gate"));
        assert!(hook.get("optional").unwrap().is_false());
        assert!(!hook.get("enabled").unwrap().is_false());
        assert!(hook.get("condition").unwrap().is_null());
        assert_eq!(
            hook.get("prompt").and_then(Yaml::text),
            Some("Run it? # yes")
        );
        assert!(!Yaml::Scalar("false".to_owned(), true).is_false());
    }

    fn required(text: &str) -> Vec<(String, String, String)> {
        required_hooks(text)
            .unwrap()
            .into_iter()
            .map(|h| (h.extension, h.event, h.command))
            .collect()
    }

    #[test]
    fn only_enabled_unconditional_required_hooks_of_other_extensions_count() {
        let hook = |ext: &str, extra: &str| {
            format!(
                "  - extension: {ext}\n    command: speckit.{ext}.check\n    optional: false\n{extra}"
            )
        };
        let text = format!(
            "hooks:\n  before_tasks:\n{}{}{}{}{}  after_tasks:\n  - {{extension: flow, command: speckit.flow.go, optional: no}}\n",
            hook("gate", "    enabled: true\n    condition: null\n"),
            hook("off", "    enabled: false\n"),
            hook("cond", "    condition: config.x is set\n"),
            hook("git", ""),
            "  - extension: soft\n    command: speckit.soft.x\n    optional: true\n",
        );
        let want = |e: &str, ev: &str, c: &str| (e.to_owned(), ev.to_owned(), c.to_owned());
        assert_eq!(
            required(&text),
            vec![
                want("gate", "before_tasks", "speckit.gate.check"),
                want("flow", "after_tasks", "speckit.flow.go"),
            ]
        );
        assert_eq!(required("hooks: {}\n"), Vec::new());
        assert_eq!(required(""), Vec::new());
        assert!(required_hooks("hooks:\n  before_tasks: *gate\n").is_err());
    }
}
