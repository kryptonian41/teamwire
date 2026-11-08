//! `teamwire preset`: the six team shapes and the three steps added to a team.
//!
//! The presets are the files of `examples/presets/`, compiled into the binary
//! so the command works wherever it is installed. Each file is a team spec
//! that `launch` accepts once its placeholders are filled in. Keys that start
//! with an underscore carry the preset's rules; `launch` ignores them.

use std::process::ExitCode;

use serde_json::Value;

use crate::cmd::shared::{fail, say};
use crate::wiring::Wiring;
use crate::{Global, PresetArgs, USAGE};

/// The presets in the order `preset` lists them: the ones to reach for first
/// come first. The `plan-review` step goes before the implementers, and the
/// `integrator` and `closer` steps after the last wave.
const PRESETS: [(&str, &str); 9] = [
    (
        "survey",
        include_str!("../../../../examples/presets/survey.json"),
    ),
    (
        "implement-then-review",
        include_str!("../../../../examples/presets/implement-then-review.json"),
    ),
    (
        "parallel-tickets",
        include_str!("../../../../examples/presets/parallel-tickets.json"),
    ),
    (
        "dependent-tickets",
        include_str!("../../../../examples/presets/dependent-tickets.json"),
    ),
    (
        "contract-split",
        include_str!("../../../../examples/presets/contract-split.json"),
    ),
    (
        "speckit",
        include_str!("../../../../examples/presets/speckit.json"),
    ),
    (
        "plan-review",
        include_str!("../../../../examples/presets/plan-review.json"),
    ),
    (
        "integrator",
        include_str!("../../../../examples/presets/integrator.json"),
    ),
    (
        "closer",
        include_str!("../../../../examples/presets/closer.json"),
    ),
];

/// The order keys are printed in. A spec reads best with the team and title
/// first and each member's identity before its task; `serde_json` would sort
/// them alphabetically.
const KEY_ORDER: [&str; 29] = [
    "_wave",
    "_blocked_by",
    "_launch_after",
    "_launch",
    "_note",
    "role",
    "team",
    "title",
    "setup",
    "harness",
    "host",
    "children",
    "kind",
    "mode",
    "task_class",
    "repo",
    "branch",
    "base",
    "cwd",
    "model",
    "effort",
    "plugins",
    "context_handoff_tokens",
    "deadline_s",
    "check",
    "owns",
    "task",
    "task_file",
    "dispatch",
];

fn key_rank(key: &str) -> (usize, &str) {
    let rank = KEY_ORDER
        .iter()
        .position(|k| *k == key)
        .unwrap_or(KEY_ORDER.len());
    (rank, key)
}

/// `value` as indented JSON with keys in `KEY_ORDER`.
fn pretty(value: &Value, depth: usize) -> String {
    let pad = "  ".repeat(depth + 1);
    let close = "  ".repeat(depth);
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|a, b| key_rank(a).cmp(&key_rank(b)));
            let items: Vec<String> = keys
                .iter()
                .map(|key| {
                    let item = map.get(*key).unwrap_or(&Value::Null);
                    format!(
                        "{pad}{}: {}",
                        Value::String((*key).clone()),
                        pretty(item, depth + 1)
                    )
                })
                .collect();
            format!("{{\n{}\n{close}}}", items.join(",\n"))
        }
        Value::Array(items) if items.iter().any(|v| v.is_object() || v.is_array()) => {
            let items: Vec<String> = items
                .iter()
                .map(|item| format!("{pad}{}", pretty(item, depth + 1)))
                .collect();
            format!("[\n{}\n{close}]", items.join(",\n"))
        }
        Value::Array(_) | Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
            value.to_string()
        }
    }
}

fn strings(value: &Value) -> Vec<&str> {
    match value {
        Value::String(text) => vec![text.as_str()],
        Value::Array(items) => items.iter().filter_map(Value::as_str).collect(),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::Object(_) => Vec::new(),
    }
}

/// The one-line description of a shape.
fn describe(text: &str) -> String {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|doc| doc.get("_shape").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_default()
}

fn list() -> String {
    let lines: Vec<String> = PRESETS
        .iter()
        .map(|(name, text)| format!("{name:<22} {}", describe(text)))
        .collect();
    lines.join("\n") + "\n"
}

/// The shape's rules as text, then its spec skeleton.
fn show(name: &str, text: &str) -> Result<String, serde_json::Error> {
    let doc: Value = serde_json::from_str(text)?;
    let mut out = vec![format!("{name}: {}", describe(text))];
    let mut skeleton = serde_json::Map::new();
    if let Value::Object(map) = &doc {
        for (key, value) in map {
            if key == "_shape" {
                continue;
            }
            let Some(title) = key.strip_prefix('_') else {
                skeleton.insert(key.clone(), value.clone());
                continue;
            };
            let heading = title.replace('_', " ");
            let lines = strings(value);
            if let [only] = lines.as_slice() {
                out.push(format!("\n{heading}: {only}"));
            } else {
                out.push(format!("\n{heading}:"));
                out.extend(
                    lines
                        .iter()
                        .enumerate()
                        .map(|(i, line)| format!("  {}. {line}", i + 1)),
                );
            }
        }
    }
    out.push(
        "\nSpec skeleton. Fill in the CHANGEME and /abs/path values. Leave out each member \
         with a `_launch` or `_launch_after` note and add it later with `--add`. Then run \
         `teamwire launch --spec FILE`:"
            .to_owned(),
    );
    out.push(pretty(&Value::Object(skeleton), 0));
    Ok(out.join("\n") + "\n")
}

pub(crate) fn run(args: &PresetArgs, _global: &Global, _wiring: &Wiring) -> ExitCode {
    let PresetArgs { name } = args;
    let Some(name) = name else {
        say(&list());
        return ExitCode::SUCCESS;
    };
    let Some((_, text)) = PRESETS.iter().find(|(n, _)| n == name) else {
        let names: Vec<&str> = PRESETS.iter().map(|(n, _)| *n).collect();
        return fail(
            "preset",
            &format!("no shape named {name}; the shapes are {}", names.join(", ")),
            USAGE,
        );
    };
    match show(name, text) {
        Ok(text) => {
            say(&text);
            ExitCode::SUCCESS
        }
        Err(e) => fail(
            "preset",
            &format!("the shape {name} is damaged: {e}"),
            USAGE,
        ),
    }
}
