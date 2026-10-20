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

use crate::cmd::send::{fail, say_line};
use crate::wiring::Wiring;
use crate::{Global, PrWatchArgs, USAGE};
use required::Required;

mod required;

const DEFAULT_INTERVAL_S: u64 = 45;
const GH_TIMEOUT: Duration = Duration::from_secs(60);

/// How many polls in a row must find no checks before a pull request with
/// none is done. Right after a push GitHub has no check runs yet, and `gh`
/// then says the same as for a pull request that never gets any.
const EMPTY_POLLS_FOR_DONE: u32 = 3;

/// How many polls in a row `gh` may give no answer before the watch says so.
const SILENT_POLLS_FOR_UNREADABLE: u32 = 3;

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
    /// How many polls in a row `gh` gave no answer for each PR.
    silent_polls: HashMap<u64, u32>,
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
        reread: &mut dyn FnMut() -> Option<Value>,
        repo: &str,
        pr: u64,
        checks: Option<&Value>,
        view: Option<&Value>,
    ) -> Vec<String> {
        let label = label(repo, pr);
        let mut events = Vec::new();
        // Said once per outage: only the poll that makes the third in a row
        // says it, and an answer from `gh` resets the count.
        if checks.is_none() && view.is_none() {
            let polls = self.silent_polls.entry(pr).or_default();
            *polls += 1;
            if *polls == SILENT_POLLS_FOR_UNREADABLE {
                events.push(format!(
                    "{label} UNREADABLE: gh gave no answer for {SILENT_POLLS_FOR_UNREADABLE} polls"
                ));
            }
        } else {
            self.silent_polls.remove(&pr);
        }
        // Where this poll said the verdict, and what the watch knew before,
        // so a re-read at DONE can take the saying back.
        let mut verdict_at = None;
        let mut verdict_before = None;
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
            let before = self.mergeable.get(&pr).cloned();
            verdict_at = self.see_verdict(&label, pr, view, &mut events);
            verdict_before = Some(before);
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
        let reported: Vec<String> = buckets.values().flatten().cloned().collect();
        let missing = self.required.get(&pr).and_then(|r| r.missing(&reported));
        let mut line = format!(
            "{label} DONE: {} passed, {} failed, {} skipped",
            count("pass"),
            failed.len(),
            count("skipping")
        );
        if !failed.is_empty() {
            line = format!("{line} [{}]", failed.join(", "));
        }
        // A required check that never started is not a pass: the pull request
        // cannot merge, yet `gh pr checks` does not list it.
        match &missing {
            Some(names) if !names.is_empty() => {
                line = format!("{line} (required, never reported: {})", names.join(", "));
            }
            Some(_) => {}
            None => line = format!("{line} (required checks unknown)"),
        }
        // GitHub recomputes the merge state after the last check finishes, so
        // a BLOCKED read in the same poll may already be out of date. Ask
        // once more, and let the fresh verdict replace the one said above.
        if self
            .mergeable
            .get(&pr)
            .is_some_and(|v| v.ends_with("/BLOCKED"))
            && let Some(fresh) = reread()
            && text(&fresh, "mergeable") != "UNKNOWN"
            && text(&fresh, "mergeStateStatus") != "UNKNOWN"
        {
            // Take back the verdict this poll said, as if it was never read.
            if let (Some(at), Some(before)) = (verdict_at, verdict_before) {
                events.remove(at);
                match before {
                    Some(v) => self.mergeable.insert(pr, v),
                    None => self.mergeable.remove(&pr),
                };
            }
            self.see_verdict(&label, pr, &fresh, &mut events);
        }
        // DIRTY was already said as a conflict; the other states are news.
        if let Some((_, state)) = self.mergeable.get(&pr).and_then(|v| v.split_once('/'))
            && !matches!(state, "CLEAN" | "DIRTY" | "")
        {
            line = format!("{line} (merge state: {state})");
        }
        // CLEAN means GitHub found every requirement met, so a "missing" check
        // beside it most likely means the set was read or matched wrongly:
        // the line names it, the exit code does not count it.
        let clean = self
            .mergeable
            .get(&pr)
            .is_some_and(|v| v.ends_with("/CLEAN"));
        let unreported = if clean {
            0
        } else {
            missing.map_or(0, |m| m.len())
        };
        self.done.insert(pr, failed.len() + unreported);
        events.push(line);
        events
    }

    /// Records the mergeability verdict of `view` and says it when it is news.
    /// Returns where in `events` the line went.
    fn see_verdict(
        &mut self,
        label: &str,
        pr: u64,
        view: &Value,
        events: &mut Vec<String>,
    ) -> Option<usize> {
        let verdict = format!(
            "{}/{}",
            text(view, "mergeable"),
            text(view, "mergeStateStatus")
        );
        if verdict.contains("UNKNOWN") || self.mergeable.get(&pr) == Some(&verdict) {
            return None;
        }
        let said = self.mergeable.contains_key(&pr)
            || ["CONFLICTING", "BLOCKED", "DRAFT"]
                .iter()
                .any(|first| verdict.contains(first));
        self.mergeable.insert(pr, verdict.clone());
        said.then(|| {
            events.push(format!("{label} MERGEABLE: {verdict}"));
            events.len() - 1
        })
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

/// What `gh pr view` says on stderr when the pull request or the repository
/// does not exist, after a `GraphQL: ` prefix.
const NOT_FOUND_TEXTS: [&str; 2] = [
    "Could not resolve to a PullRequest",
    "Could not resolve to a Repository",
];

/// `gh pr view`'s JSON, or else the one-line reason it gave for a pull
/// request or repository that does not exist. Any other failure is `Err(None)`.
fn gh_view(platform: &dyn Platform, args: &[&str]) -> Result<Value, Option<String>> {
    let out = platform
        .run(&Command::new("gh").args(args.iter().copied()), GH_TIMEOUT)
        .map_err(|_| None)?;
    if let Ok(json) = serde_json::from_slice(&out.stdout) {
        return Ok(json);
    }
    Err(String::from_utf8_lossy(&out.stderr)
        .lines()
        .find(|l| NOT_FOUND_TEXTS.iter().any(|t| l.contains(t)))
        .map(|l| l.trim().to_owned()))
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

fn view_args<'a>(number: &'a str, repo: &'a str) -> [&'a str; 7] {
    [
        "pr",
        "view",
        number,
        "-R",
        repo,
        "--json",
        "mergeable,mergeStateStatus,state,baseRefName",
    ]
}

/// How long to wait before reading the merge state again at DONE: half a poll,
/// but never long, because `Platform` and `Clock` have no sleep and the watch
/// sleeps with `std::thread::sleep` already.
fn recheck_wait(interval: Duration) -> Duration {
    (interval / 2).min(Duration::from_secs(5))
}

pub(crate) fn run(args: &PrWatchArgs, _global: &Global, wiring: &Wiring) -> ExitCode {
    let PrWatchArgs {
        repo,
        interval,
        prs,
    } = args;
    if *interval == Some(0) {
        return fail("pr-watch", "--interval must be at least 1 second", USAGE);
    }
    let interval = Duration::from_secs(interval.unwrap_or(DEFAULT_INTERVAL_S));
    let names: Vec<String> = prs.iter().map(|pr| label(repo, *pr)).collect();
    say_line(&format!(
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
            let view = match gh_view(wiring.platform.as_ref(), &view_args(&number, repo)) {
                Ok(view) => Some(view),
                Err(Some(reason)) => {
                    // Done and failed, so the watch ends once every pull request is done.
                    memory.done.insert(*pr, 1);
                    events.push(format!("{} NOT-FOUND: {reason}", label(repo, *pr)));
                    continue;
                }
                Err(None) => None,
            };
            if let Some(base) = view.as_ref().map(|v| text(v, "baseRefName"))
                && !base.is_empty()
                && !memory.required.contains_key(pr)
            {
                let set = required::read(wiring.platform.as_ref(), repo, &base);
                memory.required.insert(*pr, set);
            }
            let mut reread = || {
                std::thread::sleep(recheck_wait(interval));
                gh(wiring.platform.as_ref(), &view_args(&number, repo))
            };
            events.extend(memory.poll_one(&mut reread, repo, *pr, checks.as_ref(), view.as_ref()));
        }
        if !events.is_empty() {
            say_line(&events.join(" | "));
        }
        if memory.finished(prs) {
            return ExitCode::from(u8::from(memory.any_failed()));
        }
        std::thread::sleep(interval);
    }
}
