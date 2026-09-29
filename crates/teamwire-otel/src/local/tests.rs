#![expect(
    clippy::disallowed_methods,
    reason = "the test writes a file that is not a database into an archive directory"
)]

use std::time::Duration;

use serde_json::{Value, json};
use teamwire_core::{EventType, HarnessId, HostId, LeadRoute, Role, TeamName, Timestamp, Ulid};
use teamwire_fakes::TestHome;
use teamwire_store::{NewTeam, Store};

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

/// Writes one event row and returns its ULID.
fn ev(
    store: &mut Store,
    secs: i64,
    kind: EventType,
    who: Option<&str>,
    subject: Option<&Ulid>,
    data: &Value,
) -> Ulid {
    let who = who.map(role);
    store
        .write(|tx| tx.record_event(at(secs), kind, who.as_ref(), subject, data))
        .unwrap()
}

fn ulid(secs: i64) -> Ulid {
    Ulid::from_parts(at(secs), 7)
}

fn query(team: Option<&str>) -> MetricsQuery {
    MetricsQuery {
        team: team.map(|t| TeamName::parse(t).unwrap()),
        since: None,
    }
}

fn value(view: &MetricsView, name: &str, labels: &[(&str, &str)]) -> Option<f64> {
    view.metrics
        .iter()
        .find(|m| {
            m.name == name
                && labels
                    .iter()
                    .all(|(k, v)| m.labels.iter().any(|(mk, mv)| mk == k && mv == v))
                && m.labels.len() == labels.len()
        })
        .map(|m| m.value)
}

fn view(home: &TestHome, q: &MetricsQuery, now_secs: i64) -> MetricsView {
    metrics_at(home.path(), at(now_secs), q)
}

#[test]
fn messages_and_deliveries_are_counted_with_their_labels() {
    let home = TestHome::new().unwrap();
    let mut s = open(&home, "alpha");
    let m = ulid(10);
    ev(
        &mut s,
        10,
        EventType::MessageSent,
        Some("api"),
        Some(&m),
        &json!({"type": "question", "sender_kind": "member", "recipients": ["lead"], "chars": 4}),
    );
    ev(
        &mut s,
        11,
        EventType::MessageRefused,
        Some("api"),
        None,
        &json!({"type": "status", "reason": "too_long"}),
    );
    ev(
        &mut s,
        12,
        EventType::DeliveryHandedOff,
        None,
        Some(&m),
        &json!({"route": "host", "attempts": 2, "code": null, "seconds": 1.5}),
    );
    ev(
        &mut s,
        20,
        EventType::DeliveryAcked,
        None,
        Some(&m),
        &json!({"seconds": 10.0}),
    );
    let v = view(&home, &query(Some("alpha")), 100);
    assert_eq!(
        value(
            &v,
            "teamwire.messages.sent",
            &[
                ("team", "alpha"),
                ("type", "question"),
                ("sender_kind", "member")
            ]
        ),
        Some(1.0)
    );
    assert_eq!(
        value(
            &v,
            "teamwire.messages.refused",
            &[
                ("team", "alpha"),
                ("type", "status"),
                ("reason", "too_long")
            ]
        ),
        Some(1.0)
    );
    assert_eq!(
        value(
            &v,
            "teamwire.deliveries",
            &[
                ("team", "alpha"),
                ("route", "host"),
                ("outcome", "handed_off")
            ]
        ),
        Some(1.0)
    );
    assert_eq!(
        value(
            &v,
            "teamwire.delivery.duration",
            &[
                ("team", "alpha"),
                ("route", "host"),
                ("outcome", "handed_off")
            ]
        ),
        Some(1.5)
    );
    assert_eq!(
        value(
            &v,
            "teamwire.delivery.attempts",
            &[("team", "alpha"), ("route", "host")]
        ),
        Some(2.0)
    );
    // The acked message's type comes from its `message.sent` event.
    assert_eq!(
        value(
            &v,
            "teamwire.messages.acked",
            &[("team", "alpha"), ("type", "question")]
        ),
        Some(1.0)
    );
    assert_eq!(
        value(
            &v,
            "teamwire.ack.duration",
            &[("team", "alpha"), ("type", "question")]
        ),
        Some(10.0)
    );
    // A question handed to the lead is one wake-up.
    assert_eq!(
        value(
            &v,
            "teamwire.lead.wakeups",
            &[("team", "alpha"), ("source", "question")]
        ),
        Some(1.0)
    );
    assert!(v.errors.is_empty(), "{:?}", v.errors);
}

#[test]
fn a_histogram_shows_its_mean_and_its_count() {
    let home = TestHome::new().unwrap();
    let mut s = open(&home, "alpha");
    for (i, secs) in [(1, 2.0), (2, 4.0)] {
        let m = ulid(i);
        ev(
            &mut s,
            i,
            EventType::DeliveryHandedOff,
            None,
            Some(&m),
            &json!({"route": "host", "attempts": 1, "seconds": secs}),
        );
    }
    let v = view(&home, &query(Some("alpha")), 100);
    let labels = [
        ("team", "alpha"),
        ("route", "host"),
        ("outcome", "handed_off"),
    ];
    assert_eq!(value(&v, "teamwire.delivery.duration", &labels), Some(3.0));
    assert_eq!(
        value(&v, "teamwire.delivery.duration.count", &labels),
        Some(2.0)
    );
}

#[test]
fn an_undone_reprompt_is_not_a_reprompt_and_only_a_completed_close_says_so() {
    let home = TestHome::new().unwrap();
    let mut s = open(&home, "alpha");
    ev(
        &mut s,
        1,
        EventType::MemberCreating,
        Some("api"),
        None,
        &json!({"kind": "implementer"}),
    );
    ev(
        &mut s,
        2,
        EventType::MemberReprompted,
        Some("api"),
        None,
        &json!({"generation": 2}),
    );
    ev(
        &mut s,
        3,
        EventType::MemberReprompted,
        Some("api"),
        None,
        &json!({"generation": 1, "undone": true}),
    );
    ev(
        &mut s,
        4,
        EventType::TeamClosed,
        None,
        None,
        &json!({"outcome": "failed", "failed_step": "archive"}),
    );
    ev(
        &mut s,
        5,
        EventType::TeamClosed,
        None,
        None,
        &json!({"outcome": "completed"}),
    );
    let v = view(&home, &query(Some("alpha")), 100);
    assert_eq!(
        value(
            &v,
            "teamwire.member.reprompts",
            &[("team", "alpha"), ("kind", "implementer")]
        ),
        Some(1.0)
    );
    assert_eq!(
        value(
            &v,
            "teamwire.close.runs",
            &[("outcome", "failed"), ("failed_step", "archive")]
        ),
        Some(1.0)
    );
    assert_eq!(
        value(
            &v,
            "teamwire.close.runs",
            &[("outcome", "completed"), ("failed_step", "none")]
        ),
        Some(1.0)
    );
}

#[test]
fn hook_time_comes_from_hook_ms_and_launch_steps_from_steps() {
    let home = TestHome::new().unwrap();
    let mut s = open(&home, "alpha");
    ev(
        &mut s,
        1,
        EventType::MemberStarted,
        Some("api"),
        None,
        &json!({"launch": "L1", "requested": 1, "steps": {"worktree": 250, "spawn": 1500},
                "session_id": "s-1", "hook": "claude-stop", "hook_ms": 40}),
    );
    ev(
        &mut s,
        2,
        EventType::MemberFailed,
        Some("web"),
        None,
        &json!({"reason": "trust_refused", "steps": {"trust": 100}}),
    );
    let v = view(&home, &query(Some("alpha")), 100);
    assert_eq!(
        value(&v, "teamwire.hook.duration", &[("hook", "claude-stop")]),
        Some(0.04)
    );
    assert_eq!(
        value(
            &v,
            "teamwire.launch.duration",
            &[("team", "alpha"), ("step", "worktree")]
        ),
        Some(0.25)
    );
    assert_eq!(
        value(
            &v,
            "teamwire.launch.duration",
            &[("team", "alpha"), ("step", "spawn")]
        ),
        Some(1.5)
    );
    assert_eq!(
        value(
            &v,
            "teamwire.launch.members",
            &[
                ("team", "alpha"),
                ("outcome", "failed"),
                ("reason", "trust_refused")
            ]
        ),
        Some(1.0)
    );
    assert_eq!(
        value(
            &v,
            "teamwire.launch.members",
            &[
                ("team", "alpha"),
                ("outcome", "started"),
                ("reason", "none")
            ]
        ),
        Some(1.0)
    );
}

#[test]
fn a_member_run_is_timed_from_its_start_or_latest_reprompt() {
    let home = TestHome::new().unwrap();
    let mut s = open(&home, "alpha");
    ev(
        &mut s,
        1,
        EventType::MemberCreating,
        Some("api"),
        None,
        &json!({"kind": "implementer"}),
    );
    ev(
        &mut s,
        5,
        EventType::MemberStarted,
        Some("api"),
        None,
        &json!({}),
    );
    ev(
        &mut s,
        65,
        EventType::MemberCompleted,
        Some("api"),
        Some(&ulid(65)),
        &json!({"status": "done", "generation": 1, "n": 1}),
    );
    ev(
        &mut s,
        100,
        EventType::MemberReprompted,
        Some("api"),
        None,
        &json!({"generation": 2}),
    );
    ev(
        &mut s,
        130,
        EventType::MemberCompleted,
        Some("api"),
        Some(&ulid(130)),
        &json!({"status": "blocked", "generation": 2, "n": 2}),
    );
    let v = view(&home, &query(Some("alpha")), 200);
    let done = [
        ("team", "alpha"),
        ("kind", "implementer"),
        ("status", "done"),
    ];
    let blocked = [
        ("team", "alpha"),
        ("kind", "implementer"),
        ("status", "blocked"),
    ];
    assert_eq!(value(&v, "teamwire.member.run.duration", &done), Some(60.0));
    assert_eq!(
        value(&v, "teamwire.member.run.duration", &blocked),
        Some(30.0)
    );
    assert_eq!(value(&v, "teamwire.completions", &done), Some(1.0));
}

#[test]
fn a_refusal_reason_is_read_from_reason_or_code() {
    let home = TestHome::new().unwrap();
    let mut s = open(&home, "alpha");
    ev(
        &mut s,
        1,
        EventType::MessageRefused,
        Some("api"),
        None,
        &json!({"type": "note", "code": "too_long"}),
    );
    ev(
        &mut s,
        2,
        EventType::MessageRefused,
        Some("api"),
        None,
        &json!({"type": "note", "reason": "empty"}),
    );
    let v = view(&home, &query(Some("alpha")), 100);
    for reason in ["too_long", "empty"] {
        assert_eq!(
            value(
                &v,
                "teamwire.messages.refused",
                &[("team", "alpha"), ("type", "note"), ("reason", reason)]
            ),
            Some(1.0)
        );
    }
}

#[test]
fn holds_count_each_gap_and_unread() {
    let home = TestHome::new().unwrap();
    let mut s = open(&home, "alpha");
    ev(
        &mut s,
        1,
        EventType::MemberHeld,
        Some("api"),
        None,
        &json!({"gaps": {"report_missing": false, "commits_without_line": ["abc1234"],
                 "decisions_missing": true}, "unread": 2}),
    );
    let v = view(&home, &query(Some("alpha")), 100);
    assert_eq!(
        value(
            &v,
            "teamwire.holds",
            &[("team", "alpha"), ("reason", "report_missing")]
        ),
        None
    );
    for reason in ["progress_lines", "decisions", "unread"] {
        assert_eq!(
            value(
                &v,
                "teamwire.holds",
                &[("team", "alpha"), ("reason", reason)]
            ),
            Some(1.0),
            "{reason}"
        );
    }
}

#[test]
fn derived_lines_and_unread_gauge() {
    let home = TestHome::new().unwrap();
    let mut s = open(&home, "alpha");
    ev(
        &mut s,
        1,
        EventType::MemberCreating,
        Some("api"),
        None,
        &json!({"kind": "implementer"}),
    );
    ev(
        &mut s,
        2,
        EventType::MemberCreating,
        Some("web"),
        None,
        &json!({"kind": "implementer"}),
    );
    ev(
        &mut s,
        3,
        EventType::MemberStarted,
        Some("api"),
        None,
        &json!({}),
    );
    ev(
        &mut s,
        4,
        EventType::MemberStarted,
        Some("web"),
        None,
        &json!({}),
    );
    // Two messages to the lead; one is read.
    let (m1, m2) = (ulid(10), ulid(11));
    for m in [&m1, &m2] {
        ev(
            &mut s,
            10,
            EventType::MessageSent,
            Some("api"),
            Some(m),
            &json!({"type": "status", "sender_kind": "member"}),
        );
        ev(
            &mut s,
            11,
            EventType::DeliveryHandedOff,
            None,
            Some(m),
            &json!({"route": "host", "attempts": 1, "seconds": 1}),
        );
    }
    ev(
        &mut s,
        12,
        EventType::DeliveryAcked,
        None,
        Some(&m1),
        &json!({"seconds": 2}),
    );
    // Two watch lines and two results over the lead's pane are four
    // wake-ups, which is two per member.
    ev(
        &mut s,
        13,
        EventType::WatchLine,
        None,
        None,
        &json!({"events": ["SILENT"]}),
    );
    ev(
        &mut s,
        14,
        EventType::WatchLine,
        None,
        None,
        &json!({"events": ["DIED", "LATE"]}),
    );
    // One result delivered on the first try, one only on the second.
    let (c1, c2) = (ulid(20), ulid(21));
    ev(
        &mut s,
        20,
        EventType::ResultDelivering,
        Some("api"),
        Some(&c1),
        &json!({"route": "host"}),
    );
    ev(
        &mut s,
        21,
        EventType::ResultDelivered,
        Some("api"),
        Some(&c1),
        &json!({"route": "host", "seconds": 3}),
    );
    ev(
        &mut s,
        22,
        EventType::ResultDelivering,
        Some("web"),
        Some(&c2),
        &json!({"route": "host"}),
    );
    ev(
        &mut s,
        23,
        EventType::ResultUndelivered,
        Some("web"),
        Some(&c2),
        &json!({"route": "host", "code": "host_busy", "seconds": 3}),
    );
    ev(
        &mut s,
        24,
        EventType::ResultDelivering,
        Some("web"),
        Some(&c2),
        &json!({"route": "host"}),
    );
    ev(
        &mut s,
        25,
        EventType::ResultDelivered,
        Some("web"),
        Some(&c2),
        &json!({"route": "host", "seconds": 9}),
    );
    let v = view(&home, &query(Some("alpha")), 100);
    let team = [("team", "alpha")];
    assert_eq!(
        value(&v, "teamwire.lead.wakeups_per_member", &team),
        Some(2.0)
    );
    assert_eq!(value(&v, "teamwire.messages.read_share", &team), Some(0.5));
    assert_eq!(
        value(&v, "teamwire.results.first_try_share", &team),
        Some(0.5)
    );
    assert_eq!(
        value(
            &v,
            "teamwire.messages.unread",
            &[("team", "alpha"), ("recipient_kind", "lead")]
        ),
        Some(1.0)
    );
    assert_eq!(value(&v, "teamwire.watch.lines", &team), Some(2.0));
    assert_eq!(
        value(
            &v,
            "teamwire.watch.events",
            &[("team", "alpha"), ("event", "LATE")]
        ),
        Some(1.0)
    );
    assert_eq!(
        value(
            &v,
            "teamwire.members.live",
            &[("team", "alpha"), ("state", "started")]
        ),
        Some(2.0)
    );
    // Two delivered results: one over the lead's pane each, so two wake-ups
    // from results, plus the two watch lines.
    assert_eq!(
        value(
            &v,
            "teamwire.lead.wakeups",
            &[("team", "alpha"), ("source", "result")]
        ),
        Some(2.0)
    );
}

#[test]
fn since_leaves_out_older_events() {
    let home = TestHome::new().unwrap();
    let mut s = open(&home, "alpha");
    ev(&mut s, 10, EventType::WatchLine, None, None, &json!({}));
    ev(&mut s, 3000, EventType::WatchLine, None, None, &json!({}));
    let mut q = query(Some("alpha"));
    q.since = Some(Duration::from_secs(600));
    let v = view(&home, &q, 3100);
    assert_eq!(
        value(&v, "teamwire.watch.lines", &[("team", "alpha")]),
        Some(1.0)
    );
    let all = view(&home, &query(Some("alpha")), 3100);
    assert_eq!(
        value(&all, "teamwire.watch.lines", &[("team", "alpha")]),
        Some(2.0)
    );
}

#[test]
fn every_team_and_archive_is_read_when_no_team_is_named() {
    let home = TestHome::new().unwrap();
    let mut a = open(&home, "alpha");
    ev(&mut a, 5, EventType::WatchLine, None, None, &json!({}));
    let mut b = open(&home, "beta");
    ev(&mut b, 5, EventType::WatchLine, None, None, &json!({}));
    ev(&mut b, 6, EventType::WatchLine, None, None, &json!({}));
    let archived = home.path().join("archive").join("gamma-2026-09-30");
    std::fs::create_dir_all(&archived).unwrap();
    let mut g = open(&TestHome::new().unwrap(), "gamma");
    ev(&mut g, 5, EventType::WatchLine, None, None, &json!({}));
    g.backup_to(&archived.join("team.db")).unwrap();
    std::fs::write(archived.join("SHA256SUMS"), b"").unwrap();
    let v = view(&home, &query(None), 100);
    assert_eq!(
        value(&v, "teamwire.watch.lines", &[("team", "alpha")]),
        Some(1.0)
    );
    assert_eq!(
        value(&v, "teamwire.watch.lines", &[("team", "beta")]),
        Some(2.0)
    );
    assert_eq!(
        value(&v, "teamwire.watch.lines", &[("team", "gamma")]),
        Some(1.0)
    );
    assert!(v.errors.is_empty(), "{:?}", v.errors);
}

#[test]
fn an_unreadable_archive_is_reported_and_the_rest_still_shows() {
    let home = TestHome::new().unwrap();
    let mut a = open(&home, "alpha");
    ev(&mut a, 5, EventType::WatchLine, None, None, &json!({}));
    let bad = home.path().join("archive").join("junk-2026-09-30");
    std::fs::create_dir_all(&bad).unwrap();
    std::fs::write(bad.join("team.db"), b"not a database at all").unwrap();
    std::fs::write(bad.join("SHA256SUMS"), b"").unwrap();
    let v = view(&home, &query(None), 100);
    assert_eq!(
        value(&v, "teamwire.watch.lines", &[("team", "alpha")]),
        Some(1.0)
    );
    assert_eq!(v.errors.len(), 1, "{:?}", v.errors);
    assert!(
        v.errors.iter().all(|e| e.contains("junk-2026-09-30")),
        "{:?}",
        v.errors
    );
}

#[test]
fn the_export_lag_counts_events_after_the_mark() {
    let home = TestHome::new().unwrap();
    let mut s = open(&home, "alpha");
    ev(&mut s, 5, EventType::WatchLine, None, None, &json!({}));
    let v = view(&home, &query(Some("alpha")), 100);
    // team.created and the watch line are both unexported.
    assert_eq!(value(&v, "teamwire.export.lag", &[]), Some(2.0));
}

/// Copies `store` to `archive/<name>/team.db`, with a `SHA256SUMS` file
/// written `age_secs` seconds before now unless `sums` is false.
fn archive(home: &TestHome, store: &Store, name: &str, sums: bool, age_secs: u64) {
    let dir = home.path().join("archive").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    store.backup_to(&dir.join("team.db")).unwrap();
    if sums {
        let file = std::fs::File::create(dir.join("SHA256SUMS")).unwrap();
        let when = std::time::SystemTime::now() - Duration::from_secs(age_secs);
        file.set_modified(when).unwrap();
    }
}

#[test]
fn a_live_team_and_its_archive_count_once() {
    let home = TestHome::new().unwrap();
    let mut a = open(&home, "alpha");
    ev(&mut a, 5, EventType::WatchLine, None, None, &json!({}));
    archive(&home, &a, "alpha-2026-09-30", true, 10);
    let v = view(&home, &query(None), 100);
    assert_eq!(
        value(&v, "teamwire.watch.lines", &[("team", "alpha")]),
        Some(1.0)
    );
    assert_eq!(
        value(&v, "teamwire.export.lag", &[]),
        Some(2.0),
        "the live copy wins"
    );
}

#[test]
fn of_several_archives_of_one_team_the_newest_counts() {
    let home = TestHome::new().unwrap();
    let mut a = open(&home, "alpha");
    ev(&mut a, 5, EventType::WatchLine, None, None, &json!({}));
    archive(&home, &a, "alpha-2026-09-29", true, 7200);
    ev(&mut a, 6, EventType::WatchLine, None, None, &json!({}));
    archive(&home, &a, "alpha-2026-09-30", true, 60);
    // The live directory is gone, as after a completed close.
    std::fs::remove_dir_all(home.path().join("teams")).unwrap();
    let v = view(&home, &query(None), 100);
    assert_eq!(
        value(&v, "teamwire.watch.lines", &[("team", "alpha")]),
        Some(2.0)
    );
}

#[test]
fn an_archive_without_sha256sums_is_skipped() {
    let home = TestHome::new().unwrap();
    let mut a = open(&home, "alpha");
    ev(&mut a, 5, EventType::WatchLine, None, None, &json!({}));
    archive(&home, &a, "alpha-2026-09-29", true, 7200);
    ev(&mut a, 6, EventType::WatchLine, None, None, &json!({}));
    archive(&home, &a, "alpha-2026-09-30", false, 0);
    std::fs::remove_dir_all(home.path().join("teams")).unwrap();
    let v = view(&home, &query(None), 100);
    assert_eq!(
        value(&v, "teamwire.watch.lines", &[("team", "alpha")]),
        Some(1.0)
    );
    assert!(v.errors.is_empty(), "{:?}", v.errors);
}
