//! The checks a pull request's base branch requires, read from GitHub.
//!
//! The admin-only branch-protection endpoint is never used. The branch
//! endpoint and the rules endpoint are readable by anyone who can read the
//! repository, and together they cover both classic protection and rulesets.

use std::collections::BTreeSet;

use serde_json::Value;
use teamwire_core::{Command, Platform};

use super::GH_TIMEOUT;

/// The required check names of one base branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Required {
    /// Neither endpoint answered, so nothing can be said about missing checks.
    Unknown,
    /// The union of what the endpoints that did answer require. It may be
    /// empty: a branch with no protection answers with no contexts.
    Known(BTreeSet<String>),
}

/// An `gh api` answer, only when `gh` exited 0. On an error `gh` still prints
/// the error body as JSON, and that body is not an answer.
fn api(platform: &dyn Platform, path: &str) -> Option<Value> {
    let out = platform
        .run(&Command::new("gh").args(["api", path]), GH_TIMEOUT)
        .ok()?;
    if out.exit_code != 0 {
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}

fn strings<'a>(value: Option<&'a Value>, key: &str) -> impl Iterator<Item = &'a str> {
    value
        .and_then(|v| v.get(key))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

/// Reads the required set of `base`. The watch calls it once per pull
/// request and keeps the answer, `Unknown` included.
pub(super) fn read(platform: &dyn Platform, repo: &str, base: &str) -> Required {
    let branch = api(platform, &format!("repos/{repo}/branches/{base}"));
    let rules = api(platform, &format!("repos/{repo}/rules/branches/{base}"));
    if branch.is_none() && rules.is_none() {
        return Required::Unknown;
    }
    let mut names = BTreeSet::new();
    let status_checks = branch
        .as_ref()
        .and_then(|b| b.get("protection"))
        .and_then(|p| p.get("required_status_checks"));
    names.extend(strings(status_checks, "contexts").map(str::to_owned));
    for check in status_checks
        .and_then(|s| s.get("checks"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        names.extend(
            check
                .get("context")
                .and_then(Value::as_str)
                .map(str::to_owned),
        );
    }
    for rule in rules
        .as_ref()
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if rule.get("type").and_then(Value::as_str) != Some("required_status_checks") {
            continue;
        }
        for check in rule
            .get("parameters")
            .and_then(|p| p.get("required_status_checks"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            names.extend(
                check
                    .get("context")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            );
        }
    }
    Required::Known(names)
}

/// A check name reduced to what the two sides agree on. GitHub prints a
/// required check as the bare job name, while `gh pr checks` may print it
/// behind a workflow or app prefix such as `CI / build`, and case differs
/// between tools. Two names are the same check when the last
/// ` / `-separated part matches, ignoring case.
/// shortcut: two workflows with the same job name count as one check; split
/// the match on the full name if a repository needs to tell them apart.
fn same_check_key(name: &str) -> String {
    name.rsplit(" / ")
        .next()
        .unwrap_or(name)
        .trim()
        .to_lowercase()
}

impl Required {
    /// The required checks absent from `reported`, in name order; `None`
    /// when the required set is unknown.
    pub(super) fn missing(&self, reported: &[String]) -> Option<Vec<String>> {
        let Self::Known(names) = self else {
            return None;
        };
        let seen: BTreeSet<String> = reported.iter().map(|n| same_check_key(n)).collect();
        Some(
            names
                .iter()
                .filter(|n| !seen.contains(&same_check_key(n)))
                .cloned()
                .collect(),
        )
    }
}
