//! `status` reads and changes nothing (WAT-11), and says what the records say.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "integration tests unwrap and index; a failure fails the test"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "the test stands in for the hook that wrote a result file, outside the platform"
)]

#[path = "support/fixture.rs"]
pub mod fixture;

use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;

use fixture::{Bed, role};
use teamwire_core::TicketRef;
use teamwire_core::{
    Actor, Clock as _, CompletionStatus, DeliveryState, MessageType, PlaceState, Recipient,
    ReportState,
};
use teamwire_store::{Ctx, Store};
use teamwire_watch::{StatusView, WatchState};

/// A hash of every file under `dir`, by path.
fn hashes(dir: &Path) -> BTreeMap<String, u64> {
    let mut out = BTreeMap::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "db-shm")
                || path.to_string_lossy().ends_with("-shm")
            {
                // SQLite's shared-memory index changes whenever any reader
                // attaches to a live WAL database. It holds no stored state.
            } else {
                let mut h = DefaultHasher::new();
                std::fs::read(&path).unwrap().hash(&mut h);
                out.insert(path.display().to_string(), h.finish());
            }
        }
    }
    out
}

/// Status through a read-only store, as the command opens it.
fn status_of(bed: &Bed, role_name: Option<&str>) -> StatusView {
    let team = "alpha".parse().unwrap();
    let mut store = Store::open_read_only(bed.home.path(), &team).unwrap();
    let ctx = Ctx {
        home: bed.home.path(),
        store: &mut store,
        registry: &bed.registry,
        platform: &bed.platform,
        clock: &bed.clock,
        config: &bed.config,
    };
    let wanted = role_name.map(role);
    teamwire_watch::status_with(&ctx, wanted.as_ref())
}

fn busy_team() -> Bed {
    let mut bed = Bed::new();
    bed.add("api");
    bed.add("web");
    bed.working("api");
    bed.working("web");
    bed.poll();
    bed.delivered("api", CompletionStatus::Done);
    bed.idle("api");
    bed.question("web", DeliveryState::Undelivered);
    let id = bed.complete("web", CompletionStatus::Failed);
    bed.finish(&id, ReportState::Undelivered, None);
    bed
}

#[test]
fn status_changes_no_file() {
    let bed = busy_team();
    let team_dir = bed.home.path().join("teams").join("alpha");
    std::fs::create_dir_all(team_dir.join("outbox")).unwrap();
    std::fs::write(team_dir.join("outbox").join("api.md"), "result").unwrap();

    // With the writer still open, its WAL files are on disk.
    let before = hashes(&team_dir);
    let live = status_of(&bed, None);
    assert_eq!(hashes(&team_dir), before);
    assert!(live.errors.is_empty(), "{:?}", live.errors);

    // With the writer closed, nothing is left over either.
    let Bed {
        home,
        store,
        registry,
        platform,
        clock,
        config,
        harness,
        host,
        ..
    } = bed;
    drop(store);
    let closed_before = hashes(&team_dir);
    let team = "alpha".parse().unwrap();
    let mut ro = Store::open_read_only(home.path(), &team).unwrap();
    let ctx = Ctx {
        home: home.path(),
        store: &mut ro,
        registry: &registry,
        platform: &platform,
        clock: &clock,
        config: &config,
    };
    let view = teamwire_watch::status(&ctx);
    drop(ro);
    assert_eq!(hashes(&team_dir), closed_before);
    assert!(view.errors.is_empty(), "{:?}", view.errors);
    let _ = (harness, host);
}

#[test]
fn status_never_calls_a_writing_host_method() {
    let bed = busy_team();
    let view = status_of(&bed, None);
    assert_eq!(view.members.len(), 2);
    assert_eq!(bed.host.log().count(teamwire_fakes::CallKind::Write), 0);
    assert!(bed.harness.pushed().is_empty());
}

#[test]
fn status_shows_the_team_its_members_and_what_is_stuck() {
    let bed = busy_team();
    let view = status_of(&bed, None);
    assert_eq!(
        view.team.as_ref().map(ToString::to_string).as_deref(),
        Some("alpha")
    );
    assert_eq!(view.title.as_deref(), Some("Alpha"));
    let words: Vec<(&str, &str)> = view
        .members
        .iter()
        .map(|m| (m.role.as_str(), m.state.as_str()))
        .collect();
    assert_eq!(words, [("api", "done(done)"), ("web", "undelivered")]);
    assert_eq!(view.stuck_deliveries.len(), 1);
    assert_eq!(view.stuck_deliveries[0].state, DeliveryState::Undelivered);
    assert_eq!(view.undelivered_results.len(), 1);
    assert_eq!(view.undelivered_results[0].role.as_str(), "web");
    assert_eq!(view.messages.len(), 1);
    assert_eq!(view.messages[0].excerpt, "which branch?");
}

#[test]
fn member_states_follow_the_records_and_the_look() {
    let mut bed = Bed::new();
    for name in ["idle", "busy", "stuck", "gone", "late", "fresh", "unrep"] {
        bed.add_with_deadline(name, if name == "late" { 100 } else { 1800 });
    }
    bed.age(std::time::Duration::from_secs(120));
    bed.working("busy");
    bed.working("late");
    bed.blocked("stuck");
    bed.vanish("gone");
    bed.complete("unrep", CompletionStatus::Done);
    bed.add("fresh2");
    let view = status_of(&bed, None);
    let state = |name: &str| {
        view.members
            .iter()
            .find(|m| m.role.as_str() == name)
            .map(|m| m.state.clone())
            .unwrap()
    };
    assert_eq!(state("idle"), "quiet");
    assert_eq!(state("busy"), "working");
    assert_eq!(state("stuck"), "blocked");
    assert_eq!(state("gone"), "died?");
    assert_eq!(state("late"), "late");
    assert_eq!(state("unrep"), "unreported");
    assert_eq!(state("fresh2"), "working", "startup is not quiet");
}

#[test]
fn an_idle_member_that_was_never_prompted_is_quiet_at_once() {
    let mut bed = Bed::new();
    bed.add_undispatched("calm");
    bed.add("prompted");
    bed.add_undispatched("reprompted");
    bed.age(std::time::Duration::from_secs(1));
    bed.reprompt("reprompted");
    let view = status_of(&bed, None);
    let state = |name: &str| {
        view.members
            .iter()
            .find(|m| m.role.as_str() == name)
            .map(|m| m.state.clone())
            .unwrap()
    };
    // Inside the grace period: only the member nobody prompted is quiet.
    assert_eq!(state("calm"), "quiet");
    assert_eq!(state("prompted"), "working");
    assert_eq!(state("reprompted"), "working");
}

#[test]
fn outside_prompt_shown() {
    let mut bed = Bed::new();
    bed.add("api");
    bed.delivered("api", CompletionStatus::Done);
    bed.idle("api");
    assert_eq!(status_of(&bed, None).members[0].state, "done(done)");
    // Someone typed into the finished member's session by hand.
    bed.show("api", PlaceState::Working);
    assert_eq!(
        status_of(&bed, None).members[0].state,
        "prompted outside teamwire"
    );
}

#[test]
fn role_adds_the_resume_command() {
    let bed = busy_team();
    let plain = status_of(&bed, None);
    assert!(plain.members.iter().all(|m| m.resume_command.is_none()));
    let with = status_of(&bed, Some("api"));
    let api = with
        .members
        .iter()
        .find(|m| m.role.as_str() == "api")
        .unwrap();
    let web = with
        .members
        .iter()
        .find(|m| m.role.as_str() == "web")
        .unwrap();
    assert!(web.resume_command.is_none());
    let command = api.resume_command.as_deref().unwrap();
    assert!(command.contains("--resume sess-api"), "{command}");
    assert!(command.contains("--settings"), "{command}");
    assert!(!command.contains("TEAMWIRE_TOKEN"), "{command}");
    let missing = status_of(&bed, Some("nobody"));
    assert!(missing.errors.iter().any(|e| e.contains("nobody")));
}

#[test]
fn status_says_whether_a_watch_runs() {
    let mut bed = Bed::new();
    bed.add("api");
    let none = status_of(&bed, None).watch;
    assert_eq!((none.state, none.pid), (WatchState::None, None));

    bed.start_watch(4242);
    bed.age(std::time::Duration::from_secs(30));
    let running = status_of(&bed, None).watch;
    assert_eq!(running.state, WatchState::Running);
    assert_eq!(running.pid.map(teamwire_core::Pid::get), Some(4242));
    assert_eq!(running.heartbeat_age_s, Some(30));

    // The heartbeat is old: a lease nobody renews is stale.
    bed.age(std::time::Duration::from_secs(900));
    assert_eq!(status_of(&bed, None).watch.state, WatchState::Stale);

    // The process is gone, whatever the heartbeat says.
    let mut bed = Bed::new();
    bed.start_watch(4243);
    bed.platform.set_alive(teamwire_core::Pid::new(4243), false);
    assert_eq!(status_of(&bed, None).watch.state, WatchState::Stale);
}

fn secs(n: u64) -> std::time::Duration {
    std::time::Duration::from_secs(n)
}

#[test]
fn a_delivery_handed_off_and_not_read_is_listed_after_ten_minutes() {
    let mut bed = Bed::new();
    bed.add("api");
    bed.send(MessageType::Note, "api", None);
    bed.age(secs(599));
    assert!(status_of(&bed, None).unread_deliveries.is_empty());
    bed.age(secs(2));
    let unread = status_of(&bed, None).unread_deliveries;
    assert_eq!(unread.len(), 1, "{unread:?}");
    assert_eq!(unread[0].to.as_str(), "api");
    assert_eq!(unread[0].age_s, 601);
}

#[test]
fn a_question_is_listed_until_an_answer_points_at_it() {
    let mut bed = Bed::new();
    bed.add("api");
    let ask = bed.send(MessageType::Question, "api", None);
    bed.age(secs(30));
    let open = status_of(&bed, None).unanswered_questions;
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].from, Actor::Lead);
    assert_eq!(open[0].to, [Recipient::Member(role("api"))]);
    assert_eq!(open[0].age_s, 30);
    bed.send(MessageType::Answer, "api", Some(ask));
    assert!(status_of(&bed, None).unanswered_questions.is_empty());
}

#[test]
fn a_member_started_by_another_version_is_flagged() {
    let mut bed = Bed::new();
    bed.add("new");
    bed.add_started_by("old", "0.0.1");
    let view = status_of(&bed, None);
    let of = |name: &str| {
        view.members
            .iter()
            .find(|m| m.role.as_str() == name)
            .unwrap()
    };
    assert_eq!(
        of("new").started_by.as_deref(),
        Some(teamwire_store::WRITER_VERSION)
    );
    assert!(!of("new").older_build);
    assert_eq!(of("old").started_by.as_deref(), Some("0.0.1"));
    assert!(of("old").older_build);
}

#[test]
fn a_message_the_typed_line_carried_whole_is_not_listed_as_unread() {
    let mut bed = Bed::new();
    bed.add("api");
    bed.send_body(MessageType::Note, "api", None, "rebase on main");
    bed.age(secs(601));
    assert!(status_of(&bed, None).unread_deliveries.is_empty());
}

#[test]
fn a_message_the_member_finished_after_is_not_listed_as_unread() {
    let mut bed = Bed::new();
    bed.add("api");
    bed.send(MessageType::Note, "api", None);
    bed.age(secs(1));
    bed.complete("api", teamwire_core::CompletionStatus::Done);
    bed.age(secs(601));
    assert!(status_of(&bed, None).unread_deliveries.is_empty());

    // A later message is unread again: the report came before it.
    bed.send(MessageType::Note, "api", None);
    bed.age(secs(601));
    assert_eq!(status_of(&bed, None).unread_deliveries.len(), 1);
}

#[test]
fn only_the_newest_result_of_a_member_is_listed_as_not_received() {
    let mut bed = busy_team();
    bed.age(secs(1));
    let newer = bed.complete("web", CompletionStatus::Done);
    assert!(status_of(&bed, None).undelivered_results.is_empty());

    bed.finish(&newer, ReportState::Unconfirmed, None);
    let view = status_of(&bed, None);
    assert_eq!(view.undelivered_results.len(), 1);
    assert_eq!(view.undelivered_results[0].n, 2);
}

#[test]
fn a_result_the_lead_marked_received_is_not_listed() {
    let mut bed = busy_team();
    bed.idle("web");
    let web = status_of(&bed, None).undelivered_results[0].clone();
    let now = bed.clock.now();
    let c = bed
        .store
        .read(|tx| tx.latest_completion(&web.role))
        .unwrap()
        .unwrap();
    assert!(
        bed.store
            .write(|tx| tx.mark_result_received(now, &c.completion.ulid))
            .unwrap()
    );
    let view = status_of(&bed, None);
    assert!(view.undelivered_results.is_empty());
    let state = view.members.iter().find(|m| m.role == web.role).unwrap();
    assert_eq!(state.state, "done(failed)");
}

fn linear(id: &str) -> TicketRef {
    TicketRef {
        provider: "linear".to_owned(),
        id: id.to_owned(),
    }
}

#[test]
fn status_names_the_refs_the_team_holds() {
    let mut bed = Bed::new();
    assert!(status_of(&bed, None).refs.is_empty());
    let json = serde_json::to_value(status_of(&bed, None)).unwrap();
    assert_eq!(json["refs"], serde_json::json!([]));

    bed.store
        .write(|tx| tx.set_refs(&[linear("GKA-580"), linear("GKA-9")]))
        .unwrap();
    assert_eq!(
        status_of(&bed, None).refs,
        [linear("GKA-580"), linear("GKA-9")]
    );
}
