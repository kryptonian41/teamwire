//! The crate layer rule of `docs/conventions.md` section 5, read from
//! `cargo metadata --no-deps`.
//!
//! Never: let a dev-dependency name a crate that normal dependencies could
//! not, except `teamwire-fakes`, which every crate may use in its tests.
//! Invariants: a crate under `crates/` missing from the table
//! below is itself a problem, so a new crate cannot skip the rule.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::Value;

const L0: &[&str] = &["teamwire-core"];
const L1: &[&str] = &["teamwire-store", "teamwire-platform"];
const L2: &[&str] = &[
    "teamwire-host-herdr",
    "teamwire-host-tmux",
    "teamwire-harness-claude",
    "teamwire-harness-codex",
    "teamwire-harness-pi",
];
const L3: &[&str] = &["teamwire-deliver", "teamwire-verify"];
const L4: &[&str] = &[
    "teamwire-msg",
    "teamwire-report",
    "teamwire-launch",
    "teamwire-watch",
    "teamwire-otel",
    "teamwire-close",
];
const L5: &[&str] = &["teamwire-mcp"];
const CLI: &str = "teamwire-cli";
const FAKES: &str = "teamwire-fakes";
/// The only packages that may depend on `anyhow` (AGENTS.md hard rules).
const ANYHOW_USERS: &[&str] = &[CLI, "xtask"];
const ANYTHING: &[&str] = &["teamwire-crash-tests", "teamwire-e2e"];

/// What a crate in the table may depend on.
enum Rule {
    Only(Vec<&'static str>),
    Anything,
}

/// The rule for `name`, or `None` when the crate is not in the table.
fn allowed(name: &str) -> Option<Rule> {
    let join = |parts: &[&[&'static str]]| -> Vec<&'static str> {
        parts.iter().flat_map(|p| p.iter().copied()).collect()
    };
    let rule = if L0.contains(&name) {
        Some(vec![])
    } else if L1.contains(&name) {
        Some(join(&[L0]))
    } else if L2.contains(&name) {
        Some(join(&[L0, &["teamwire-platform"]]))
    } else if L3.contains(&name) {
        Some(join(&[L0, L1]))
    } else if L4.contains(&name) {
        Some(join(&[L0, L1, L3]))
    } else if L5.contains(&name) {
        Some(join(&[L0, L1, L3, L4]))
    } else if name == CLI {
        Some(join(&[L0, L1, L2, L3, L4, L5]))
    } else if name == FAKES {
        Some(join(&[L0]))
    } else if ANYTHING.contains(&name) {
        return Some(Rule::Anything);
    } else {
        return None;
    };
    rule.map(Rule::Only)
}

/// `cargo metadata --format-version 1 --no-deps` for the workspace at `root`.
#[expect(
    clippy::disallowed_methods,
    reason = "xtask is a developer tool that runs cargo directly"
)]
pub(crate) fn cargo_metadata(root: &Path) -> Result<Value> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = std::process::Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--no-deps", "--locked"])
        .current_dir(root)
        .output()
        .context("running cargo metadata")?;
    if !output.status.success() {
        bail!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    serde_json::from_slice(&output.stdout).context("parsing cargo metadata output")
}

pub(crate) fn check_layers(metadata: &Value, problems: &mut Vec<String>) {
    let packages = metadata
        .get("packages")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    for package in packages {
        let Some(name) = package.get("name").and_then(Value::as_str) else {
            continue;
        };
        check_anyhow(name, package, problems);
        if !name.starts_with("teamwire-") {
            continue;
        }
        let Some(rule) = allowed(name) else {
            problems.push(format!(
                "{name}: not in the layer table of docs/conventions.md section 5; add it to xtask/src/layers.rs"
            ));
            continue;
        };
        let Rule::Only(rule) = rule else {
            continue;
        };
        let deps = package
            .get("dependencies")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice);
        for dep in deps {
            let Some(dep_name) = dep.get("name").and_then(Value::as_str) else {
                continue;
            };
            let is_dev = dep.get("kind").and_then(Value::as_str) == Some("dev");
            let fakes_in_tests = is_dev && dep_name == FAKES;
            if fakes_in_tests || !dep_name.starts_with("teamwire-") || rule.contains(&dep_name) {
                continue;
            }
            problems.push(format!(
                "{name}: depends on {dep_name}, which is not in a lower layer (docs/conventions.md section 5)"
            ));
        }
    }
}

/// Libraries return `thiserror` errors; `anyhow` is for the binaries that
/// print them.
fn check_anyhow(name: &str, package: &Value, problems: &mut Vec<String>) {
    let deps = package
        .get("dependencies")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let uses = deps
        .iter()
        .any(|d| d.get("name").and_then(Value::as_str) == Some("anyhow"));
    if uses && !ANYHOW_USERS.contains(&name) {
        problems.push(format!(
            "{name}: depends on anyhow; only teamwire-cli and xtask may (AGENTS.md hard rules)"
        ));
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn metadata(name: &str, deps: &[(&str, Option<&str>)]) -> Value {
        let deps: Vec<Value> = deps
            .iter()
            .map(|(n, kind)| json!({"name": n, "kind": kind}))
            .collect();
        json!({"packages": [{"name": name, "dependencies": deps}]})
    }

    fn run(metadata: &Value) -> Vec<String> {
        let mut problems = Vec::new();
        check_layers(metadata, &mut problems);
        problems
    }

    #[test]
    fn lower_layer_edges_pass() {
        let m = metadata(
            "teamwire-msg",
            &[
                ("teamwire-core", None),
                ("teamwire-store", None),
                ("teamwire-deliver", None),
                ("serde", None),
            ],
        );
        assert_eq!(run(&m), Vec::<String>::new());
    }

    #[test]
    fn same_layer_edge_fails() {
        let m = metadata("teamwire-msg", &[("teamwire-report", None)]);
        let problems = run(&m);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            problems
                .first()
                .is_some_and(|p| p.contains("teamwire-msg: depends on teamwire-report"))
        );
    }

    #[test]
    fn service_crate_naming_an_adapter_fails() {
        let m = metadata(
            "teamwire-deliver",
            &[("teamwire-host-herdr", Some("build"))],
        );
        assert_eq!(run(&m).len(), 1);
    }

    #[test]
    fn dev_dependency_on_a_higher_layer_fails() {
        let m = metadata("teamwire-store", &[("teamwire-msg", Some("dev"))]);
        assert_eq!(run(&m).len(), 1);
        let ok = metadata("teamwire-msg", &[("teamwire-store", Some("dev"))]);
        assert_eq!(run(&ok), Vec::<String>::new());
    }

    #[test]
    fn dev_dependency_on_fakes_passes_but_normal_one_fails() {
        let dev = metadata("teamwire-store", &[("teamwire-fakes", Some("dev"))]);
        assert_eq!(run(&dev), Vec::<String>::new());
        let normal = metadata("teamwire-store", &[("teamwire-fakes", None)]);
        assert_eq!(run(&normal).len(), 1);
    }

    #[test]
    fn cli_may_use_adapters_and_test_crates_anything() {
        let cli = metadata(
            "teamwire-cli",
            &[("teamwire-host-herdr", None), ("teamwire-mcp", None)],
        );
        assert_eq!(run(&cli), Vec::<String>::new());
        let e2e = metadata("teamwire-e2e", &[("teamwire-cli", None)]);
        assert_eq!(run(&e2e), Vec::<String>::new());
    }

    #[test]
    fn anyhow_is_allowed_only_in_cli_and_xtask() {
        let cli = metadata("teamwire-cli", &[("anyhow", None)]);
        assert_eq!(run(&cli), Vec::<String>::new());
        let tool = metadata("xtask", &[("anyhow", None)]);
        assert_eq!(run(&tool), Vec::<String>::new());
        let lib = metadata("teamwire-store", &[("anyhow", None)]);
        assert_eq!(run(&lib).len(), 1);
        let dev = metadata("teamwire-e2e", &[("anyhow", Some("dev"))]);
        assert_eq!(run(&dev).len(), 1);
    }

    #[test]
    fn crate_missing_from_the_table_fails() {
        let m = metadata("teamwire-newcomer", &[]);
        assert_eq!(run(&m).len(), 1);
    }
}
