use std::path::PathBuf;

use serde_json::{Value, json};
use teamwire_core::{
    EventType, HarnessId, HostId, LeadRoute, MemberKind, Mode, Role, TeamName, Timestamp, Ulid,
};
use teamwire_fakes::TestHome;
use teamwire_store::{NewMember, NewTeam, Store};

use super::*;

fn at(secs: i64) -> Timestamp {
    Timestamp::from_unix_millis(1_790_000_000_000 + secs * 1000).unwrap()
}

fn role(name: &str) -> Role {
    Role::parse(name).unwrap()
}

fn open(home: &TestHome, team: &str) -> Store {
    let name = TeamName::parse(team).unwrap();
    let mut store = Store::open(home.path(), &name).unwrap();
    store
        .write(|tx| {
            tx.create_team(
                at(0),
                &NewTeam {
                    name: name.clone(),
                    title: team.to_owned(),
                    lead_harness: HarnessId::from_static("claude-code"),
                    lead_host: HostId::from_static("herdr"),
                    lead_route: LeadRoute::Sink,
                    lead_session: None,
                },
            )
        })
        .unwrap();
    store
}

fn ev(store: &mut Store, secs: i64, kind: EventType, who: &str, data: &Value) {
    let who = role(who);
    let subject = matches!(
        kind,
        EventType::MemberCompleted | EventType::ResultDelivering
    )
    .then(|| Ulid::from_parts(at(secs), 7));
    store
        .write(|tx| tx.record_event(at(secs), kind, Some(&who), subject.as_ref(), data))
        .unwrap();
}

/// Inserts `who` as an implementer on `harness`, with `class` as the
/// `task_class` launch records (null for none), and starts it at second 1
/// with `started` as the `member.started` data.
fn member(store: &mut Store, who: &str, harness: &'static str, class: &Value, started: &Value) {
    let new = NewMember {
        role: role(who),
        title: who.to_owned(),
        kind: MemberKind::Implementer,
        mode: Mode::Worktree,
        harness: HarnessId::from_static(harness),
        host: HostId::from_static("herdr"),
        cwd: PathBuf::from("/tmp"),
        repo: None,
        branch: None,
        base: None,
        base_sha: None,
        check: None,
        owns: Vec::new(),
        deadline_s: 3600,
    };
    store
        .write(|tx| tx.insert_member_with(at(1), &new, &json!({ "task_class": class })))
        .unwrap();
    ev(store, 1, EventType::MemberStarted, who, started);
}

fn tokens(store: &mut Store, secs: i64, who: &str, input: u64) {
    let usage = json!({"session": "s1", "usage": {"opus": {"input": input, "output": 0, "cache_read": 0, "cache_creation": 0}}});
    ev(store, secs, EventType::ResultDelivering, who, &usage);
}

fn verdict(store: &mut Store, secs: i64, who: &str, verdict: &str) {
    ev(
        store,
        secs,
        EventType::VerifyRecorded,
        who,
        &json!({"verdict": verdict, "flags": [], "exit": 0, "seconds": 1}),
    );
}

fn completed(store: &mut Store, secs: i64, who: &str) {
    ev(
        store,
        secs,
        EventType::MemberCompleted,
        who,
        &json!({"status": "done", "generation": 1, "n": 1}),
    );
}

fn reprompt(store: &mut Store, secs: i64, who: &str, undone: bool) {
    let data = if undone {
        json!({"generation": 2, "undone": true})
    } else {
        json!({"generation": 2})
    };
    ev(store, secs, EventType::MemberReprompted, who, &data);
}

fn row<'a>(report: &'a OutcomeReport, class: &str, model: &str) -> &'a OutcomeRow {
    let found = report
        .rows
        .iter()
        .find(|r| r.task_class == class && r.model == model);
    assert!(found.is_some(), "no row for {class} {model}: {report:#?}");
    found.unwrap()
}

#[test]
fn a_classified_and_an_unclassified_member_get_their_own_rows() {
    let home = TestHome::new().unwrap();
    let mut s = open(&home, "alpha");
    member(
        &mut s,
        "api",
        "claude-code",
        &json!("root_cause"),
        &json!({"model": "opus", "effort": "high"}),
    );
    tokens(&mut s, 100, "api", 500);
    verdict(&mut s, 200, "api", "FLAGGED");
    verdict(&mut s, 300, "api", "PASS");
    completed(&mut s, 1801, "api");
    // `web` is from a build before task classes, and never verified.
    member(&mut s, "web", "pi", &Value::Null, &json!({}));
    // `db` failed its launch, so it did no work and is left out.
    s.write(|tx| {
        tx.insert_member(
            at(2),
            &NewMember {
                role: role("db"),
                title: "db".to_owned(),
                kind: MemberKind::Reviewer,
                mode: Mode::Pane,
                harness: HarnessId::from_static("codex"),
                host: HostId::from_static("herdr"),
                cwd: PathBuf::from("/tmp"),
                repo: None,
                branch: None,
                base: None,
                base_sha: None,
                check: None,
                owns: Vec::new(),
                deadline_s: 60,
            },
        )
    })
    .unwrap();

    let report = outcome_report(home.path());
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.teams, 1);
    assert_eq!(report.rows.len(), 2, "{report:#?}");
    assert_eq!(
        *row(&report, "root_cause", "opus"),
        OutcomeRow {
            harness: "claude-code".to_owned(),
            model: "opus".to_owned(),
            effort: "high".to_owned(),
            task_class: "root_cause".to_owned(),
            kind: "implementer".to_owned(),
            members: 1,
            verify: VerifyCounts {
                pass: 1,
                ..VerifyCounts::default()
            },
            reprompts: 0,
            tokens_median: Some(500.0),
            minutes_median: Some(30.0),
            harder_than_class: 0,
        }
    );
    let web = row(&report, "unclassified", "unset");
    assert_eq!(web.harness, "pi");
    assert_eq!(web.effort, "unset");
    assert_eq!(
        web.verify,
        VerifyCounts {
            none: 1,
            ..VerifyCounts::default()
        }
    );
    assert_eq!(web.tokens_median, None);
    assert_eq!(web.minutes_median, None);
}

#[test]
fn the_report_serialises_with_the_verdicts_as_keys() {
    let home = TestHome::new().unwrap();
    let mut s = open(&home, "alpha");
    member(
        &mut s,
        "api",
        "claude-code",
        &json!("judgement"),
        &json!({}),
    );
    verdict(&mut s, 10, "api", "FAIL");
    let report = serde_json::to_value(outcome_report(home.path())).unwrap();
    let first = report.get("rows").and_then(|r| r.get(0)).unwrap();
    assert_eq!(first["task_class"], "judgement");
    assert_eq!(
        first["verify"],
        json!({"PASS": 0, "FLAGGED": 0, "FAIL": 1, "none": 0})
    );
    assert_eq!(first["tokens_median"], Value::Null);
    assert_eq!(first["harder_than_class"], 1);
    assert_eq!(report.get("teams"), Some(&json!(1)));
}

#[test]
fn an_undone_reprompt_is_not_counted() {
    let home = TestHome::new().unwrap();
    let mut s = open(&home, "alpha");
    let started = json!({"model": "sonnet"});
    member(
        &mut s,
        "api",
        "claude-code",
        &json!("cross_cutting"),
        &started,
    );
    member(
        &mut s,
        "web",
        "claude-code",
        &json!("cross_cutting"),
        &started,
    );
    // `api`: two reprompts, and an undo that takes the second back.
    reprompt(&mut s, 10, "api", false);
    reprompt(&mut s, 20, "api", false);
    reprompt(&mut s, 30, "api", true);
    // `web`: two reprompts that stand.
    reprompt(&mut s, 40, "web", false);
    reprompt(&mut s, 50, "web", false);
    let report = outcome_report(home.path());
    let r = row(&report, "cross_cutting", "sonnet");
    assert_eq!(r.members, 2);
    assert_eq!(r.reprompts, 3);
    // Only `web` reached 2 reprompts.
    assert_eq!(r.harder_than_class, 1);
}

/// Guard (ADR 0043 decision 9): a member woken twice by messages started two
/// generations, but nobody reprompted it, so it is not harder than its class.
#[test]
fn a_woken_generation_is_not_counted_as_a_reprompt() {
    let home = TestHome::new().unwrap();
    let mut s = open(&home, "alpha");
    member(
        &mut s,
        "api",
        "claude-code",
        &json!("cross_cutting"),
        &json!({}),
    );
    ev(
        &mut s,
        10,
        EventType::MemberWoken,
        "api",
        &json!({"generation": 2, "message": "m001"}),
    );
    ev(
        &mut s,
        20,
        EventType::MemberWoken,
        "api",
        &json!({"generation": 3, "message": "m002"}),
    );
    let report = outcome_report(home.path());
    let r = row(&report, "cross_cutting", "unset");
    assert_eq!(r.reprompts, 0);
    assert_eq!(r.harder_than_class, 0);
}

#[test]
fn an_unclassified_member_is_never_harder_than_its_class() {
    let home = TestHome::new().unwrap();
    let mut s = open(&home, "alpha");
    member(&mut s, "api", "claude-code", &Value::Null, &json!({}));
    reprompt(&mut s, 10, "api", false);
    reprompt(&mut s, 20, "api", false);
    verdict(&mut s, 30, "api", "FAIL");
    let report = outcome_report(home.path());
    let r = row(&report, "unclassified", "unset");
    assert_eq!(r.reprompts, 2);
    assert_eq!(r.verify.fail, 1);
    assert_eq!(r.harder_than_class, 0);
}

/// Members of `root_cause` with these token totals, one of them on a
/// second model so the class spans two rows.
fn class_with_tokens(totals: &[u64]) -> OutcomeReport {
    let home = TestHome::new().unwrap();
    let mut s = open(&home, "alpha");
    for (i, total) in totals.iter().enumerate() {
        let who = format!("m{i}");
        let model = if i == 0 { "haiku" } else { "opus" };
        member(
            &mut s,
            &who,
            "claude-code",
            &json!("root_cause"),
            &json!({ "model": model }),
        );
        tokens(&mut s, 10, &who, *total);
    }
    outcome_report(home.path())
}

#[test]
fn heavy_tokens_count_only_once_the_class_has_five_readings() {
    // Four readings: 1000 is over 3 times the median, but the class is too
    // small to judge.
    let four = class_with_tokens(&[100, 100, 100, 1000]);
    let harder: u64 = four.rows.iter().map(|r| r.harder_than_class).sum();
    assert_eq!(harder, 0, "{four:#?}");

    // Five readings: the median is 100, so 1000 counts and 300 does not.
    let five = class_with_tokens(&[100, 100, 100, 300, 1000]);
    assert_eq!(row(&five, "root_cause", "haiku").harder_than_class, 0);
    let opus = row(&five, "root_cause", "opus");
    assert_eq!(opus.harder_than_class, 1);
    assert_eq!(opus.members, 4);
    // 100, 100, 300, 1000: the mean of the two middle values.
    assert_eq!(opus.tokens_median, Some(200.0));
}

#[test]
fn an_archive_is_read_beside_an_open_team_and_each_team_counts_once() {
    let home = TestHome::new().unwrap();
    let mut alpha = open(&home, "alpha");
    member(
        &mut alpha,
        "api",
        "claude-code",
        &json!("mechanical_edit"),
        &json!({}),
    );
    // `alpha` is also archived: the live copy counts and the archive does not.
    archive(&home, &alpha, "alpha-2026-09-30");

    let mut beta = open(&home, "beta");
    member(
        &mut beta,
        "web",
        "codex",
        &json!("mechanical_edit"),
        &json!({}),
    );
    archive(&home, &beta, "beta-2026-09-30");
    drop(beta);
    std::fs::remove_dir_all(home.path().join("teams").join("beta")).unwrap();

    let report = outcome_report(home.path());
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.teams, 2);
    let harnesses: Vec<(&str, u64)> = report
        .rows
        .iter()
        .map(|r| (r.harness.as_str(), r.members))
        .collect();
    assert_eq!(harnesses, vec![("claude-code", 1), ("codex", 1)]);
}

/// Copies `store` to `archive/<name>/team.db`, with a `SHA256SUMS` file
/// beside it. The report reads only that `SHA256SUMS` exists and when it was
/// written, so a second database copy stands in for the checksum list.
fn archive(home: &TestHome, store: &Store, name: &str) {
    let dir = home.path().join("archive").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    store.backup_to(&dir.join("team.db")).unwrap();
    store.backup_to(&dir.join("SHA256SUMS")).unwrap();
}

#[test]
fn minutes_run_to_the_first_completion_not_a_later_one() {
    let home = TestHome::new().unwrap();
    let mut s = open(&home, "alpha");
    member(
        &mut s,
        "api",
        "claude-code",
        &json!("root_cause"),
        &json!({}),
    );
    // Started at second 1, done at minute 10, then reprompted and done again
    // at minute 60.
    completed(&mut s, 601, "api");
    reprompt(&mut s, 700, "api", false);
    completed(&mut s, 3601, "api");
    let report = outcome_report(home.path());
    assert_eq!(
        row(&report, "root_cause", "unset").minutes_median,
        Some(10.0)
    );
}

#[test]
fn the_median_of_an_even_count_is_the_mean_of_the_middle_two() {
    assert_eq!(median(vec![]), None);
    assert_eq!(median(vec![3.0, 1.0, 2.0]), Some(2.0));
    assert_eq!(median(vec![4.0, 1.0, 2.0, 3.0]), Some(2.5));
}
