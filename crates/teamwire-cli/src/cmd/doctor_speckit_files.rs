//! The file readers of the spec-kit doctor checks: the `installed:` list of
//! `.specify/extensions.yml` and the spec-kit tag in `.claude/settings.json`.
//!
//! Never: write a file or run a program. Pure functions over text and JSON.

/// One YAML scalar as spec-kit writes it: maybe quoted, maybe followed by a
/// comment.
fn yaml_scalar(text: &str) -> String {
    let text = text.split(" #").next().unwrap_or(text);
    text.trim().trim_matches(['"', '\'']).to_owned()
}

/// The extension ids under the top-level `installed:` key, written as an
/// inline list (`installed: [bug, git]`) or as `- git` items below it.
pub(crate) fn installed_extensions(text: &str) -> Vec<String> {
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
