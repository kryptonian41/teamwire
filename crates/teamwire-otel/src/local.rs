//! `teamwire metrics`: the metrics of one team, or of every team and archive,
//! computed from the event rows with no collector (OBS-3).

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use teamwire_core::{Event, EventType, Timestamp};
use teamwire_store::{Store, StoreError, database_path};

use crate::defs::{self, Kind};
use crate::history::History;
use crate::observe::{Observation, gauges, observe};
use crate::{MetricValue, MetricsQuery, MetricsView};

/// What one database contributes.
struct Analysis {
    team: String,
    observations: Vec<Observation>,
    derived: Derived,
}

/// The counts behind the three derived lines. They are summed per team name
/// before the ratios are taken, so a live team and its archive combine.
#[derive(Default, Clone, Copy)]
struct Derived {
    wakeups: f64,
    members: f64,
    handed_off: f64,
    acked: f64,
    results: f64,
    first_try: f64,
}

/// Metrics for `q`, reading databases under `home`. `now` is the wall time
/// `q.since` counts back from.
pub fn metrics_at(home: &Path, now: Timestamp, q: &MetricsQuery) -> MetricsView {
    let mut errors = Vec::new();
    let mut analyses = Vec::new();
    for source in sources(home, q, &mut errors) {
        match Store::open_read_only_at(&source.path) {
            Ok(store) => match analyze(&store, &source, now, q) {
                Ok(a) => analyses.push(a),
                Err(e) => errors.push(format!("{}: {e}", source.path.display())),
            },
            Err(e) => errors.push(format!("{}: {e}", source.path.display())),
        }
    }
    finish(analyses, errors)
}

/// Metrics read through the caller's own store handle when `q.team` is set
/// (that handle is the team's database), else across everything under
/// `ctx.home`.
pub(crate) fn metrics_with(
    home: &Path,
    store: &Store,
    now: Timestamp,
    q: &MetricsQuery,
) -> MetricsView {
    let Some(team) = &q.team else {
        return metrics_at(home, now, q);
    };
    let source = Source {
        path: database_path(home, team),
        team: Some(team.as_str().to_owned()),
        live: true,
    };
    match analyze(store, &source, now, q) {
        Ok(a) => finish(vec![a], Vec::new()),
        Err(e) => finish(Vec::new(), vec![format!("team {team}: {e}")]),
    }
}

struct Source {
    path: PathBuf,
    /// The name to use when the database has no team row.
    team: Option<String>,
    /// A running team, not an archive: only it has an export mark that means
    /// anything.
    live: bool,
}

fn sources(home: &Path, q: &MetricsQuery, errors: &mut Vec<String>) -> Vec<Source> {
    if let Some(team) = &q.team {
        return vec![Source {
            path: database_path(home, team),
            team: Some(team.as_str().to_owned()),
            live: true,
        }];
    }
    let mut list = Vec::new();
    for (dir, live) in [("teams", true), ("archive", false)] {
        let root = home.join(dir);
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = Vec::new();
        for entry in entries {
            match entry {
                Ok(entry) => paths.push(entry.path().join("team.db")),
                Err(e) => errors.push(format!("{}: {e}", root.display())),
            }
        }
        paths.sort();
        list.extend(
            paths
                .into_iter()
                .filter(|p| p.is_file())
                .map(|path| Source {
                    path,
                    team: None,
                    live,
                }),
        );
    }
    list
}

fn analyze(
    store: &Store,
    source: &Source,
    now: Timestamp,
    q: &MetricsQuery,
) -> Result<Analysis, StoreError> {
    let (name, listing, lag) = store.read(|tx| {
        let name = tx.team()?.map(|t| t.name.as_str().to_owned());
        let listing = tx.events_after(0, None, None)?;
        let lag = if source.live {
            Some(
                tx.last_event_seq()?
                    .saturating_sub(tx.export_mark()?.last_event_seq),
            )
        } else {
            None
        };
        Ok((name, listing, lag))
    })?;
    let team = name
        .or_else(|| source.team.clone())
        .unwrap_or_else(|| "unknown".to_owned());
    if !listing.bad.is_empty() {
        tracing::warn!(
            team,
            rows = listing.bad.len(),
            "metrics skipped unreadable events"
        );
    }
    let cutoff = q
        .since
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .and_then(|ms| Timestamp::from_unix_millis(now.unix_millis().saturating_sub(ms)));
    let history = History::new(listing.items);
    let mut observations = Vec::new();
    let mut derived = Derived::default();
    let mut result_subjects = HashSet::new();
    let mut first_try = HashSet::new();
    let mut roles = HashSet::new();
    for e in history.events() {
        if e.kind == EventType::MemberCreating {
            if let Some(role) = &e.role {
                roles.insert(role.as_str());
            }
        }
        if cutoff.is_some_and(|c| e.time < c) {
            continue;
        }
        observations.extend(observe(&history, e, &team));
        first_try_of(&history, e, &mut result_subjects, &mut first_try);
    }
    #[expect(clippy::cast_precision_loss, reason = "counts are far below 2^52")]
    {
        derived.members = roles.len() as f64;
        derived.results = result_subjects.len() as f64;
        derived.first_try = first_try.len() as f64;
    }
    for o in &observations {
        match (o.def.name, o.labels.iter().find(|(k, _)| k == "outcome")) {
            (n, _) if n == defs::LEAD_WAKEUPS.name => derived.wakeups += o.value,
            (n, Some((_, v))) if n == defs::DELIVERIES.name && v == "handed_off" => {
                derived.handed_off += o.value;
            }
            (n, _) if n == defs::MESSAGES_ACKED.name => derived.acked += o.value,
            _ => {}
        }
    }
    if let Some(last) = history.events().last() {
        observations.extend(gauges(&history, last.seq, last.time, &team));
    } else {
        observations.extend(gauges(&history, 0, now, &team));
    }
    if let Some(lag) = lag {
        #[expect(clippy::cast_precision_loss, reason = "a backlog is far below 2^52")]
        observations.push(Observation {
            def: &defs::EXPORT_LAG,
            labels: Vec::new(),
            value: lag as f64,
            time: now,
        });
    }
    Ok(Analysis {
        team,
        observations,
        derived,
    })
}

/// Tracks which results ended and which of them were delivered on the first
/// try: one `result.delivering`, and no earlier failed end.
fn first_try_of(
    h: &History,
    e: &Event,
    subjects: &mut HashSet<String>,
    first_try: &mut HashSet<String>,
) {
    let (true, Some(subject)) = (
        matches!(
            e.kind,
            EventType::ResultDelivered
                | EventType::ResultUndelivered
                | EventType::ResultUnconfirmed
        ),
        &e.subject,
    ) else {
        return;
    };
    let subject = subject.to_string();
    subjects.insert(subject.clone());
    if e.kind != EventType::ResultDelivered {
        return;
    }
    let earlier: Vec<&Event> = h.about(&subject).take_while(|p| p.seq < e.seq).collect();
    let tries = earlier
        .iter()
        .filter(|p| p.kind == EventType::ResultDelivering)
        .count();
    let failed_before = earlier.iter().any(|p| {
        matches!(
            p.kind,
            EventType::ResultUndelivered | EventType::ResultUnconfirmed
        )
    });
    if tries <= 1 && !failed_before {
        first_try.insert(subject);
    }
}

#[derive(Default)]
struct Agg {
    sum: f64,
    count: f64,
}

fn finish(analyses: Vec<Analysis>, errors: Vec<String>) -> MetricsView {
    type Key = (&'static str, Vec<(String, String)>);
    let mut table: BTreeMap<Key, (&'static defs::Def, Agg)> = BTreeMap::new();
    let mut per_team: BTreeMap<String, Derived> = BTreeMap::new();
    for a in analyses {
        let total = per_team.entry(a.team.clone()).or_default();
        total.wakeups += a.derived.wakeups;
        total.members += a.derived.members;
        total.handed_off += a.derived.handed_off;
        total.acked += a.derived.acked;
        total.results += a.derived.results;
        total.first_try += a.derived.first_try;
        for o in a.observations {
            let entry = table
                .entry((o.def.name, o.labels))
                .or_insert_with(|| (o.def, Agg::default()));
            entry.1.sum += o.value;
            entry.1.count += 1.0;
        }
    }
    let mut metrics = Vec::new();
    for ((name, labels), (def, agg)) in table {
        let mut push = |name: String, unit: &str, value: f64| {
            metrics.push(MetricValue {
                name,
                unit: unit.to_owned(),
                labels: labels.clone(),
                value,
            });
        };
        match def.kind {
            Kind::Counter => push(name.to_owned(), def.unit, agg.sum),
            // Gauges from several databases with one label set (the same team
            // live and archived) add up: each counts its own members.
            Kind::Gauge => push(name.to_owned(), def.unit, agg.sum),
            Kind::Histogram => {
                push(name.to_owned(), def.unit, agg.sum / agg.count);
                push(format!("{name}.count"), "{observation}", agg.count);
            }
        }
    }
    for (team, d) in per_team {
        let labels = vec![("team".to_owned(), team)];
        let mut derived = |name: &str, unit: &str, num: f64, den: f64| {
            if den > 0.0 {
                metrics.push(MetricValue {
                    name: name.to_owned(),
                    unit: unit.to_owned(),
                    labels: labels.clone(),
                    value: num / den,
                });
            }
        };
        derived(defs::WAKEUPS_PER_MEMBER, "{wakeup}", d.wakeups, d.members);
        derived(defs::READ_SHARE, "1", d.acked, d.handed_off);
        derived(defs::FIRST_TRY_SHARE, "1", d.first_try, d.results);
    }
    MetricsView { metrics, errors }
}

#[cfg(test)]
mod tests;
