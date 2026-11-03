//! The public evidence file, `examples/models/evidence.json`: it parses as
//! `Evidence`, every source is a dated `https` page, no source is dated after
//! the snapshot, and each source ranks its entries for a class 1, 2, 3 and so
//! on. `teamwire models` shows these ranks to leads, so a gap or a stray date
//! would mislead them.

use std::collections::{BTreeMap, BTreeSet};

use teamwire_core::{Evidence, EvidenceEntry, TaskClass};
use time::{Date, Month};

const EVIDENCE: &str = include_str!("../../../examples/models/evidence.json");

fn evidence() -> serde_json::Result<Evidence> {
    serde_json::from_str(EVIDENCE)
}

/// A strict `YYYY-MM-DD` reading: ten characters, zero-padded, and a day
/// that exists in its month.
fn date(text: &str) -> Option<Date> {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes.get(4) != Some(&b'-') || bytes.get(7) != Some(&b'-') {
        return None;
    }
    let digits = |range: std::ops::Range<usize>| -> Option<u32> {
        let part = text.get(range)?;
        if !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        part.parse().ok()
    };
    let year = i32::try_from(digits(0..4)?).ok()?;
    let month = Month::try_from(u8::try_from(digits(5..7)?).ok()?).ok()?;
    let day = u8::try_from(digits(8..10)?).ok()?;
    Date::from_calendar_date(year, month, day).ok()
}

/// The `(source, task_class)` groups whose ranks are not exactly 1..=n.
/// Ties share a rank, so a group ranked 1, 2, 2, 3 passes.
fn rank_gaps(entries: &[EvidenceEntry]) -> Vec<(String, TaskClass, Vec<u32>)> {
    let mut groups: BTreeMap<(String, &'static str), (TaskClass, BTreeSet<u32>)> = BTreeMap::new();
    for e in entries {
        groups
            .entry((e.source.clone(), e.task_class.as_str()))
            .or_insert_with(|| (e.task_class, BTreeSet::new()))
            .1
            .insert(e.rank);
    }
    groups
        .into_iter()
        .filter(|(_, (_, ranks))| {
            !u32::try_from(ranks.len()).is_ok_and(|n| ranks.iter().copied().eq(1..=n))
        })
        .map(|((source, _), (class, ranks))| (source, class, ranks.into_iter().collect()))
        .collect()
}

fn entry(source: &str, class: TaskClass, rank: u32) -> EvidenceEntry {
    EvidenceEntry {
        task_class: class,
        model: "m".into(),
        effort: None,
        rank,
        claim: "c".into(),
        source: source.into(),
        url: "https://example.com".into(),
        published: "2026-01-01".into(),
        kind: teamwire_core::EvidenceKind::Benchmark,
    }
}

#[test]
fn the_file_parses_and_has_entries() {
    let e = evidence().unwrap();
    assert!(!e.entries.is_empty());
}

#[test]
fn every_url_is_https() {
    for e in evidence().unwrap().entries {
        assert!(
            e.url.starts_with("https://"),
            "{} has url {}",
            e.source,
            e.url
        );
    }
}

#[test]
fn as_of_and_every_published_are_dates() {
    let e = evidence().unwrap();
    assert!(date(&e.as_of).is_some(), "as_of {:?}", e.as_of);
    for entry in &e.entries {
        assert!(
            date(&entry.published).is_some(),
            "{} has published {:?}",
            entry.source,
            entry.published
        );
    }
}

#[test]
fn nothing_is_published_after_as_of() {
    let e = evidence().unwrap();
    let as_of = date(&e.as_of).unwrap();
    for entry in &e.entries {
        let published = date(&entry.published).unwrap();
        assert!(
            published <= as_of,
            "{} is dated {} after as_of {}",
            entry.source,
            entry.published,
            e.as_of
        );
    }
}

#[test]
fn ranks_in_each_source_and_class_run_from_one_without_gaps() {
    assert_eq!(rank_gaps(&evidence().unwrap().entries), vec![]);
}

#[test]
fn every_entry_names_a_model_a_claim_and_a_source() {
    for e in evidence().unwrap().entries {
        assert!(!e.model.trim().is_empty(), "{} has no model", e.source);
        assert!(!e.claim.trim().is_empty(), "{} has no claim", e.source);
        assert!(
            !e.source.trim().is_empty(),
            "an entry for {} has no source",
            e.model
        );
        assert!(
            e.effort.as_deref().is_none_or(|x| !x.trim().is_empty()),
            "{} has an empty effort",
            e.source
        );
    }
}

#[test]
fn a_covered_class_has_at_least_two_entries() {
    let entries = evidence().unwrap().entries;
    for class in TaskClass::ALL {
        let n = entries.iter().filter(|e| e.task_class == *class).count();
        assert!(n == 0 || n >= 2, "{} has only {n} entry", class.as_str());
    }
}

#[test]
fn date_accepts_only_real_zero_padded_days() {
    assert_eq!(
        date("2026-11-03"),
        Date::from_calendar_date(2026, Month::November, 3).ok()
    );
    assert!(date("2024-02-29").is_some());
    for bad in [
        "2026-1-03",
        "2026-11-3",
        "2026/11/03",
        "2026-13-01",
        "2026-02-30",
        "2025-02-29",
        "26-11-03",
        "2026-11-03T00:00",
        "+202-11-03",
        "",
    ] {
        assert!(date(bad).is_none(), "{bad} was accepted");
    }
}

#[test]
fn rank_gaps_finds_a_missing_rank_or_a_start_above_one() {
    use TaskClass::{Judgement, RootCause};
    let fine = [
        entry("a", RootCause, 1),
        entry("a", RootCause, 2),
        entry("a", RootCause, 2),
        entry("a", RootCause, 3),
        entry("a", Judgement, 1),
        entry("b", RootCause, 1),
    ];
    assert_eq!(rank_gaps(&fine), vec![]);

    let gap = [entry("a", RootCause, 1), entry("a", RootCause, 3)];
    assert_eq!(rank_gaps(&gap), vec![("a".into(), RootCause, vec![1, 3])]);

    let late_start = [entry("a", Judgement, 2), entry("a", Judgement, 3)];
    assert_eq!(
        rank_gaps(&late_start),
        vec![("a".into(), Judgement, vec![2, 3])]
    );

    let zero = [entry("a", Judgement, 0), entry("a", Judgement, 1)];
    assert_eq!(rank_gaps(&zero), vec![("a".into(), Judgement, vec![0, 1])]);
}
