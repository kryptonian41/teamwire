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

/// What one endpoint said.
enum Answer {
    Json(Value),
    /// HTTP 404: the server has no such feature (no rulesets, no rules), so
    /// nothing is required through this endpoint.
    NotFound,
    /// Any other failure: the endpoint may well require checks.
    Failed,
}

/// A `gh api` answer, trusted only when `gh` exited 0. On an error `gh` still
/// prints the error body as JSON, and that body is not an answer.
/// `paginate` reads every page of a list: `--slurp` joins the pages into one
/// array of arrays.
fn api(platform: &dyn Platform, path: &str, paginate: bool) -> Answer {
    let mut args = vec!["api"];
    if paginate {
        args.extend(["--paginate", "--slurp"]);
    }
    args.push(path);
    let Ok(out) = platform.run(&Command::new("gh").args(args), GH_TIMEOUT) else {
        return Answer::Failed;
    };
    if out.exit_code != 0 {
        let said = String::from_utf8_lossy(&[out.stderr, out.stdout].concat()).into_owned();
        return if said.contains("HTTP 404") || said.contains("Not Found") {
            Answer::NotFound
        } else {
            Answer::Failed
        };
    }
    serde_json::from_slice(&out.stdout).map_or(Answer::Failed, Answer::Json)
}

/// `None` for a failure; JSON null for a 404, which answers "nothing".
fn json_or_none(answer: Answer) -> Option<Value> {
    match answer {
        Answer::Json(v) => Some(v),
        Answer::NotFound => Some(Value::Null),
        Answer::Failed => None,
    }
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
    let (Some(branch), Some(rules)) = (
        json_or_none(api(
            platform,
            &format!("repos/{repo}/branches/{base}"),
            false,
        )),
        json_or_none(api(
            platform,
            &format!("repos/{repo}/rules/branches/{base}"),
            true,
        )),
    ) else {
        // One endpoint failing for any reason but 404 may hide required
        // checks, so the whole set is unknown, not the other endpoint's half.
        return Required::Unknown;
    };
    let mut names = BTreeSet::new();
    let status_checks = branch
        .get("protection")
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
    // Slurped pages are an array of arrays; a plain list is read too.
    for rule in rules.as_array().into_iter().flatten().flat_map(|page| {
        page.as_array()
            .map_or_else(|| std::slice::from_ref(page), Vec::as_slice)
    }) {
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
