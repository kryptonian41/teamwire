//! `teamwire pr-watch`: watch GitHub pull-request checks (EXT-1).
//!
//! The rules: silence while nothing changed, one line per poll at most, and
//! no line for a check that passes.
//! `gh pr checks` exits 8 while checks are pending and 1 when one failed, yet
//! still prints its JSON, so the exit code of `gh` is never a signal here. A
//! call that prints nothing or prints text that is not JSON is skipped and
//! tried again on the next poll.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::process::ExitCode;
use std::time::Duration;

use serde_json::Value;
use teamwire_core::{Command, Platform};

use crate::wiring::Wiring;
use required::Required;
use crate::{Global, PrWatchArgs, USAGE};

mod required;

const DEFAULT_INTERVAL_S: u64 = 45;
const GH_TIMEOUT: Duration = Duration::from_secs(60);

/// How many polls in a row must find no checks before a pull request with
/// none is done. Right after a push GitHub has no check runs yet, and `gh`
/// then says the same as for a pull request that never gets any.
const EMPTY_POLLS_FOR_DONE: u32 = 3;

/// What the watch remembers between polls, so it can stay quiet.
#[derive(Debug, Default)]
struct Memory {
    /// (PR, check) pairs whose first failure was already reported.
    announced: HashSet<(u64, String)>,
    /// The last mergeability verdict of each PR.
    mergeable: HashMap<u64, String>,
    /// Each finished PR with the number of checks that failed.
    done: HashMap<u64, usize>,
    /// How many polls in a row found no checks on each PR.
    empty_polls: HashMap<u64, u32>,
    /// The required checks of each PR's base branch, read once per PR.
    required: HashMap<u64, Required>,
}

fn label(repo: &str, pr: u64) -> String {
    format!("{repo}#{pr}")
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned()
}

impl Memory {
    /// The lines one pull request adds to this poll, given what `gh` printed
    /// (`None` when the call printed nothing usable).
    fn poll_one(
        &mut self,
        repo: &str,
        pr: u64,
        checks: Option<&Value>,
        view: Option<&Value>,
    ) -> Vec<String> {
        let label = label(repo, pr);
        let mut events = Vec::new();
        if let Some(view) = view {
            let state = text(view, "state");
            if state == "MERGED" || state == "CLOSED" {
                // A failure already announced still counts: the watch must not
                // exit 0 because the PR was merged before the DONE line.
                let failed = self.announced.iter().filter(|(p, _)| *p == pr).count();
                self.done.insert(pr, failed);
                events.push(format!("{label} {state}"));
                return events;
            }
            let verdict = format!(
                "{}/{}",
                text(view, "mergeable"),
                text(view, "mergeStateStatus")
            );
            if !verdict.contains("UNKNOWN") && self.mergeable.get(&pr) != Some(&verdict) {
                if self.mergeable.contains_key(&pr)
                    || ["CONFLICTING", "BLOCKED", "DRAFT"]
                        .iter()
                        .any(|first| verdict.contains(first))
                {
                    events.push(format!("{label} MERGEABLE: {verdict}"));
                }
                self.mergeable.insert(pr, verdict);
            }
        }
        let Some(checks) = checks.and_then(Value::as_array) else {
            return events;
        };
        // A PR with no checks has nothing pending, but only once the list
        // stayed empty long enough for CI to have started.
        if checks.is_empty() {
            let polls = self.empty_polls.entry(pr).or_default();
            *polls += 1;
            if *polls < EMPTY_POLLS_FOR_DONE {
                return events;
            }
        } else {
            self.empty_polls.remove(&pr);
        }
        let mut buckets: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for check in checks {
            let (name, bucket) = (text(check, "name"), text(check, "bucket"));
            if (bucket == "fail" || bucket == "cancel") && self.announced.insert((pr, name.clone()))
            {
                events.push(format!("{label} FAIL: {name}"));
            }
            buckets.entry(bucket).or_default().push(name);
        }
        if buckets.contains_key("pending") {
            return events;
        }
        let count = |bucket: &str| buckets.get(bucket).map_or(0, Vec::len);
        let failed: Vec<&str> = ["fail", "cancel"]
            .iter()
            .filter_map(|b| buckets.get(*b))
            .flatten()
            .map(String::as_str)
            .collect();
        let mut line = format!(
            "{label} DONE: {} passed, {} failed, {} skipped",
            count("pass"),
            failed.len(),
            count("skipping")
        );
        if !failed.is_empty() {
            line = format!("{line} [{}]", failed.join(", "));
        }
        // DIRTY was already said as a conflict; the other states are news.
        if let Some((_, state)) = self.mergeable.get(&pr).and_then(|v| v.split_once('/'))
            && !matches!(state, "CLEAN" | "DIRTY" | "")
        {
            line = format!("{line} (merge state: {state})");
        }
        self.done.insert(pr, failed.len());
        events.push(line);
        events
    }

    fn finished(&self, prs: &[u64]) -> bool {
        prs.iter().all(|pr| self.done.contains_key(pr))
    }

    fn any_failed(&self) -> bool {
        self.done.values().any(|failed| *failed > 0)
    }
}

/// `gh`'s JSON, or `None` when the call failed, timed out or printed
/// anything that is not JSON.
fn gh(platform: &dyn Platform, args: &[&str]) -> Option<Value> {
    let out = platform
        .run(&Command::new("gh").args(args.iter().copied()), GH_TIMEOUT)
        .ok()?;
    serde_json::from_slice(&out.stdout).ok()
}

/// What `gh pr checks` prints on stderr, with exit 1 and nothing on stdout,
/// for a pull request that has no checks at all.
const NO_CHECKS_TEXT: &str = "no checks reported";

/// The check list of one pull request. A pull request with no checks gets an
/// empty list, because `gh` prints no JSON for it and the watch would
/// otherwise ask again for ever.
fn gh_checks(platform: &dyn Platform, args: &[&str]) -> Option<Value> {
    let out = platform
        .run(&Command::new("gh").args(args.iter().copied()), GH_TIMEOUT)
        .ok()?;
    if let Ok(json) = serde_json::from_slice(&out.stdout) {
        return Some(json);
    }
    String::from_utf8_lossy(&out.stderr)
        .contains(NO_CHECKS_TEXT)
        .then(|| Value::Array(Vec::new()))
}

#[expect(clippy::print_stdout, reason = "the lines are the command's output")]
fn say(text: &str) {
    println!("{text}");
}

#[expect(
    clippy::print_stderr,
    reason = "the binary reports a refusal on stderr"
)]
fn fail(message: &str) -> ExitCode {
    eprintln!("teamwire pr-watch: {message}");
    ExitCode::from(USAGE)
}

pub(crate) fn run(args: &PrWatchArgs, _global: &Global, wiring: &Wiring) -> ExitCode {
    let PrWatchArgs {
        repo,
        interval,
        prs,
    } = args;
    if *interval == Some(0) {
        return fail("--interval must be at least 1 second");
    }
    let interval = Duration::from_secs(interval.unwrap_or(DEFAULT_INTERVAL_S));
    let names: Vec<String> = prs.iter().map(|pr| label(repo, *pr)).collect();
    say(&format!(
        "WATCHING {} (poll {}s)",
        names.join(", "),
        interval.as_secs()
    ));
    let mut memory = Memory::default();
    loop {
        let mut events = Vec::new();
        for pr in prs {
            if memory.done.contains_key(pr) {
                continue;
            }
            let number = pr.to_string();
            let checks = gh_checks(
                wiring.platform.as_ref(),
                &["pr", "checks", &number, "-R", repo, "--json", "name,bucket"],
            );
            let view = gh(
                wiring.platform.as_ref(),
                &[
                    "pr",
                    "view",
                    &number,
                    "-R",
                    repo,
                    "--json",
                    "mergeable,mergeStateStatus,state,baseRefName",
                ],
            );
            if let Some(base) = view.as_ref().map(|v| text(v, "baseRefName"))
                && !base.is_empty()
                && !memory.required.contains_key(pr)
            {
                let set = required::read(wiring.platform.as_ref(), repo, &base);
                memory.required.insert(*pr, set);
            }
            events.extend(memory.poll_one(repo, *pr, checks.as_ref(), view.as_ref()));
        }
        if !events.is_empty() {
            say(&events.join(" | "));
        }
        if memory.finished(prs) {
            return ExitCode::from(u8::from(memory.any_failed()));
        }
        std::thread::sleep(interval);
    }
}
