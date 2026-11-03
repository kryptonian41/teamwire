//! The outcome report: what the members of past teams did, grouped by
//! harness, model, effort, task class and kind. `teamwire models --for
//! <class>` shows it beside the public evidence and never blends the two
//! into one score.
//!
//! The report gives counts and medians, never percentages: most groups
//! hold a handful of members, and the caller decides how to show a small one.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use teamwire_core::{EventType, Member};
use teamwire_store::{Store, StoreError};

use crate::MetricsQuery;
use crate::defs;
use crate::history::{History, UNCLASSIFIED, flag, text};
use crate::local::{CopyOf, Source, one_copy_of_each_team, sources};
use crate::observe::observe;

/// The value of `harness`, `model` or `effort` when none was recorded.
const UNSET: &str = "unset";

/// Every group of members of every open team and archive.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OutcomeReport {
    /// One row per harness, model, effort, task class and kind, sorted by
    /// those five fields.
    pub rows: Vec<OutcomeRow>,
    /// How many teams were read, each counted once however many copies of
    /// it exist.
    pub teams: u64,
    /// Why part of the report could not be read.
    pub errors: Vec<String>,
}

/// The members of one group and how their work went.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutcomeRow {
    pub harness: String,
    pub model: String,
    pub effort: String,
    /// A `TaskClass` value, or `unclassified`.
    pub task_class: String,
    pub kind: String,
    pub members: u64,
    /// Each member's last verify verdict.
    pub verify: VerifyCounts,
    /// Reprompts across the group, leaving out the ones an undo took back.
    pub reprompts: u64,
    /// The median of each member's total tokens, counting only members
    /// with a token reading. `None` when no member has one.
    pub tokens_median: Option<f64>,
    /// The median minutes from a member's first `member.started` to its
    /// first `member.completed`, over the members that completed.
    pub minutes_median: Option<f64>,
    /// How many members look harder than their class (`harder_than_class`).
    pub harder_than_class: u64,
}

/// How many members of a group ended on each verify verdict.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyCounts {
    #[serde(rename = "PASS")]
    pub pass: u64,
    #[serde(rename = "FLAGGED")]
    pub flagged: u64,
    #[serde(rename = "FAIL")]
    pub fail: u64,
    /// Members never verified.
    pub none: u64,
}

/// One member, read from its team's events and member row.
#[derive(Debug, Clone)]
struct MemberOutcome {
    harness: String,
    model: String,
    effort: String,
    task_class: &'static str,
    kind: String,
    verdict: Option<String>,
    reprompts: u64,
    tokens: Option<f64>,
    minutes: Option<f64>,
}

/// The outcome report over every team under `home/teams/` and every
/// archive under `home/archive/`, the set `teamwire metrics` reads with no
/// team named.
pub fn outcome_report(home: &Path) -> OutcomeReport {
    let mut errors = Vec::new();
    let mut teams = Vec::new();
    for source in sources(home, &MetricsQuery::default(), &mut errors) {
        match Store::open_read_only_at(&source.path).and_then(|s| read_team(&s, &source)) {
            Ok(team) => teams.push(team),
            Err(e) => errors.push(format!("{}: {e}", source.path.display())),
        }
    }
    let teams = one_copy_of_each_team(teams, |(copy, _)| copy);
    let count = u64::try_from(teams.len()).unwrap_or(u64::MAX);
    let members: Vec<MemberOutcome> = teams.into_iter().flat_map(|(_, m)| m).collect();
    OutcomeReport {
        rows: rows(&members),
        teams: count,
        errors,
    }
}

fn read_team(store: &Store, source: &Source) -> Result<(CopyOf, Vec<MemberOutcome>), StoreError> {
    let (identity, members, listing) = store.read(|tx| {
        let identity = tx.team()?.map(|t| t.ulid.to_string());
        Ok((identity, tx.members()?, tx.events_after(0, None, None)?))
    })?;
    if !listing.bad.is_empty() || !members.bad.is_empty() {
        tracing::warn!(
            path = %source.path.display(),
            events = listing.bad.len(),
            members = members.bad.len(),
            "the outcome report skipped unreadable rows"
        );
    }
    let history = History::new(listing.items);
    let outcomes = members
        .items
        .iter()
        .filter_map(|m| member_outcome(&history, m))
        .collect();
    Ok((CopyOf::new(source, identity), outcomes))
}

/// `None` for a member that never started: its launch failed, so it did no
/// work a model can be judged by.
fn member_outcome(h: &History, m: &Member) -> Option<MemberOutcome> {
    let role = m.role.as_str();
    let mine: Vec<_> = h.of_role(role).collect();
    let first_start = mine.iter().find(|e| e.kind == EventType::MemberStarted)?;
    let last_start = mine
        .iter()
        .rfind(|e| e.kind == EventType::MemberStarted)
        .unwrap_or(first_start);
    let recorded = |key: &str| {
        text(&last_start.data, key)
            .filter(|v| !v.is_empty())
            .unwrap_or(UNSET)
            .to_owned()
    };
    let minutes = mine
        .iter()
        .find(|e| e.kind == EventType::MemberCompleted && e.seq > first_start.seq)
        .map(|done| done.time.saturating_since(first_start.time).as_secs_f64() / 60.0);
    let verdict = mine
        .iter()
        .rfind(|e| e.kind == EventType::VerifyRecorded)
        .and_then(|e| text(&e.data, "verdict"))
        .map(str::to_owned);
    let reprompts = mine
        .iter()
        .filter(|e| {
            e.kind == EventType::MemberReprompted && !flag(&e.data, "undone") && !h.is_undone(e)
        })
        .count();
    Some(MemberOutcome {
        harness: m.harness.as_str().to_owned(),
        model: recorded("model"),
        effort: recorded("effort"),
        task_class: h.task_class(role, u64::MAX),
        kind: m.kind.as_str().to_owned(),
        verdict,
        reprompts: u64::try_from(reprompts).unwrap_or(u64::MAX),
        tokens: tokens(h, role),
        minutes,
    })
}

/// The member's total tokens of every type, counted the way
/// `teamwire.member.tokens` counts them. `None` when no `result.delivering`
/// of the member carried a usage reading.
fn tokens(h: &History, role: &str) -> Option<f64> {
    let readings: Vec<_> = h
        .of_role(role)
        .filter(|e| e.kind == EventType::ResultDelivering)
        .filter(|e| e.data.get("usage").is_some_and(Value::is_object))
        .collect();
    if readings.is_empty() {
        return None;
    }
    let total = readings
        .into_iter()
        .flat_map(|e| observe(h, e, ""))
        .filter(|o| o.def.name == defs::MEMBER_TOKENS.name)
        .map(|o| o.value)
        .sum();
    Some(total)
}

/// Whether a member looks harder than the task class its lead gave it.
///
/// This rule is a first guess, not a measured one. Change it here and
/// nowhere else. A classified member counts when any of these hold:
/// - it had 2 or more reprompts that were not undone;
/// - its last verify verdict was `FAIL`;
/// - its total tokens were over 3 times the median of its class, when the
///   class has at least 5 members with a token reading.
///
/// `class_tokens` holds the token totals of every member of the same class
/// that has a reading, sorted. An unclassified member never counts: it has
/// no class to be harder than.
fn harder_than_class(m: &MemberOutcome, class_tokens: &[f64]) -> bool {
    const REPROMPTS: u64 = 2;
    const TOKEN_FACTOR: f64 = 3.0;
    const CLASS_FLOOR: usize = 5;
    if m.task_class == UNCLASSIFIED {
        return false;
    }
    let heavy = match (m.tokens, median_of_sorted(class_tokens)) {
        (Some(mine), Some(median)) if class_tokens.len() >= CLASS_FLOOR => {
            mine > TOKEN_FACTOR * median
        }
        _ => false,
    };
    m.reprompts >= REPROMPTS || m.verdict.as_deref() == Some("FAIL") || heavy
}

type GroupKey = (String, String, String, String, String);

fn rows(members: &[MemberOutcome]) -> Vec<OutcomeRow> {
    let mut by_class: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    for m in members {
        if let Some(t) = m.tokens {
            by_class.entry(m.task_class).or_default().push(t);
        }
    }
    for list in by_class.values_mut() {
        list.sort_by(f64::total_cmp);
    }
    let mut groups: BTreeMap<GroupKey, Vec<&MemberOutcome>> = BTreeMap::new();
    for m in members {
        let key = (
            m.harness.clone(),
            m.model.clone(),
            m.effort.clone(),
            m.task_class.to_owned(),
            m.kind.clone(),
        );
        groups.entry(key).or_default().push(m);
    }
    groups
        .into_iter()
        .map(|((harness, model, effort, task_class, kind), list)| {
            let class_tokens = by_class
                .get(task_class.as_str())
                .map_or(&[][..], Vec::as_slice);
            let mut verify = VerifyCounts::default();
            for m in &list {
                match m.verdict.as_deref() {
                    Some("PASS") => verify.pass += 1,
                    Some("FLAGGED") => verify.flagged += 1,
                    Some("FAIL") => verify.fail += 1,
                    _ => verify.none += 1,
                }
            }
            let count = |n: usize| u64::try_from(n).unwrap_or(u64::MAX);
            OutcomeRow {
                members: count(list.len()),
                verify,
                reprompts: list.iter().map(|m| m.reprompts).sum(),
                tokens_median: median(list.iter().filter_map(|m| m.tokens).collect()),
                minutes_median: median(list.iter().filter_map(|m| m.minutes).collect()),
                harder_than_class: count(
                    list.iter()
                        .filter(|m| harder_than_class(m, class_tokens))
                        .count(),
                ),
                harness,
                model,
                effort,
                task_class,
                kind,
            }
        })
        .collect()
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    values.sort_by(f64::total_cmp);
    median_of_sorted(&values)
}

/// The middle value, or the mean of the two middle values.
fn median_of_sorted(values: &[f64]) -> Option<f64> {
    let mid = values.len() / 2;
    match values.len() {
        0 => None,
        n if n % 2 == 1 => values.get(mid).copied(),
        _ => Some((values.get(mid - 1)? + values.get(mid)?) / 2.0),
    }
}

#[cfg(test)]
mod tests;
