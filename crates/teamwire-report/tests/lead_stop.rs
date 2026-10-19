//! Which teams the lead's Stop hook names (ADR 0018 point 5): each condition
//! that is false on its own leaves the team out.
#![expect(
    clippy::unwrap_used,
    reason = "a failing unwrap in an integration test fails the test"
)]

use std::time::Duration;

use teamwire_core::{
    Clock, CompletionStatus, FinalSource, HarnessId, HostId, LeadRoute, MemberKind, MemberState,
    Mode, Pid, Role, SessionId, SessionKind, TeamName,
};
use teamwire_deliver::WATCH_LEASE_STALE;
use teamwire_fakes::{FakeClock, FakePlatform, TestHome};
use teamwire_report::{afk_hold_notes, teams_without_watch, watch_missing_note};
use teamwire_store::{NewCompletion, NewMember, NewTeam, Store};

struct Rig {
    home: TestHome,
    platform: FakePlatform,
    clock: FakeClock,
}

fn sid(text: &str) -> SessionId {
    SessionId::parse(text).unwrap()
}

impl Rig {
    fn new() -> Self {
        Self {
            home: TestHome::new().unwrap(),
            platform: FakePlatform::new(),
            clock: FakeClock::new(),
        }
    }

    /// An open team led from `lead-1`, with member `api` started.
    fn team(&self, name: &str) -> Store {
        let mut store = Store::open(self.home.path(), &TeamName::parse(name).unwrap()).unwrap();
        let now = self.clock.now();
        store
            .write(|tx| {
                tx.create_team(
                    now,
                    &NewTeam {
                        name: TeamName::parse(name).unwrap(),
                        title: name.to_owned(),
                        lead_harness: HarnessId::from_static("claude-code"),
                        lead_host: HostId::from_static("herdr"),
                        lead_route: LeadRoute::Sink,
                        lead_session: Some((sid("lead-1"), SessionKind::Interactive)),
                    },
                )?;
                tx.insert_member(
                    now,
                    &NewMember {
                        role: Role::parse("api").unwrap(),
                        title: "api".to_owned(),
                        kind: MemberKind::Implementer,
                        mode: Mode::Pane,
                        harness: HarnessId::from_static("claude-code"),
                        host: HostId::from_static("herdr"),
                        cwd: "/work".into(),
                        repo: None,
                        branch: None,
                        base: None,
                        base_sha: None,
                        check: None,
                        owns: vec![],
                        deadline_s: 1800,
                    },
                )?;
                tx.set_member_state(
                    now,
                    &Role::parse("api").unwrap(),
                    MemberState::Started,
                    &serde_json::json!({}),
                )
            })
            .unwrap();
        store
    }

    fn names(&self, session: &str) -> Vec<String> {
        teams_without_watch(self.home.path(), &sid(session), &self.platform, &self.clock)
            .iter()
            .map(|t| t.as_str().to_owned())
            .collect()
    }

    fn lease(&self, store: &mut Store, pid: u32) {
        let now = self.clock.now();
        store
            .write(|tx| {
                tx.take_watch_lease(now, Pid::new(pid), "host", |_| true)
                    .map(|_| ())
            })
            .unwrap();
    }
}

fn complete(store: &mut Store, rig: &Rig) {
    let now = rig.clock.now();
    store
        .write(|tx| {
            tx.insert_completion(
                now,
                &NewCompletion {
                    role: Role::parse("api").unwrap(),
                    generation: 1,
                    status: CompletionStatus::Done,
                    summary: "done".to_owned(),
                    final_message: None,
                    final_source: FinalSource::None,
                },
            )
            .map(|_| ())
        })
        .unwrap();
}

#[test]
fn a_working_team_with_no_watch_is_named() {
    let rig = Rig::new();
    let _store = rig.team("alpha");
    assert_eq!(rig.names("lead-1"), ["alpha"]);
}

#[test]
fn another_session_is_left_alone() {
    let rig = Rig::new();
    let _store = rig.team("alpha");
    assert!(rig.names("someone-else").is_empty());
}

#[test]
fn a_team_that_is_not_open_is_left_out() {
    let rig = Rig::new();
    let mut store = rig.team("alpha");
    store
        .write(|tx| {
            tx.connection()
                .execute("UPDATE team SET state = 'closed'", [])
                .map(|_| ())
                .map_err(Into::into)
        })
        .unwrap();
    assert!(rig.names("lead-1").is_empty());
}

#[test]
fn a_team_with_no_member_started_is_left_out() {
    let rig = Rig::new();
    let mut store = rig.team("alpha");
    store
        .write(|tx| {
            tx.connection()
                .execute("UPDATE members SET state = 'creating'", [])
                .map(|_| ())
                .map_err(Into::into)
        })
        .unwrap();
    assert!(rig.names("lead-1").is_empty());
}

#[test]
fn a_team_whose_members_all_completed_is_left_out() {
    let rig = Rig::new();
    let mut store = rig.team("alpha");
    complete(&mut store, &rig);
    assert!(rig.names("lead-1").is_empty());
}

#[test]
fn a_reprompted_member_needs_a_watch_again() {
    let rig = Rig::new();
    let mut store = rig.team("alpha");
    complete(&mut store, &rig);
    let now = rig.clock.now();
    store
        .write(|tx| {
            tx.bump_generation(now, &Role::parse("api").unwrap(), "again")
                .map(|_| ())
        })
        .unwrap();
    assert_eq!(rig.names("lead-1"), ["alpha"]);
}

#[test]
fn a_live_watch_lease_leaves_the_team_out() {
    let rig = Rig::new();
    let mut store = rig.team("alpha");
    rig.platform.set_alive(Pid::new(700), true);
    rig.lease(&mut store, 700);
    assert!(rig.names("lead-1").is_empty());
}

#[test]
fn a_lease_whose_process_is_gone_does_not_count() {
    let rig = Rig::new();
    let mut store = rig.team("alpha");
    rig.lease(&mut store, 700);
    assert_eq!(rig.names("lead-1"), ["alpha"]);
}

#[test]
fn a_lease_nobody_refreshed_does_not_count() {
    let rig = Rig::new();
    let mut store = rig.team("alpha");
    rig.platform.set_alive(Pid::new(700), true);
    rig.lease(&mut store, 700);
    rig.clock
        .advance(WATCH_LEASE_STALE + Duration::from_secs(1));
    assert_eq!(rig.names("lead-1"), ["alpha"]);
}

#[test]
fn only_the_teams_that_need_a_watch_are_named() {
    let rig = Rig::new();
    let _a = rig.team("alpha");
    let mut b = rig.team("beta");
    complete(&mut b, &rig);
    let _c = rig.team("gamma");
    assert_eq!(rig.names("lead-1"), ["alpha", "gamma"]);
}

#[test]
fn the_note_names_the_team_and_the_command() {
    let note = watch_missing_note(&TeamName::parse("alpha").unwrap(), true);
    assert_eq!(
        note,
        "[teamwire:alpha] no watch is running; start `teamwire watch alpha` with Monitor before you stop."
    );
}

#[test]
fn a_lead_that_cannot_read_watch_lines_is_told_the_background_watch() {
    let note = watch_missing_note(&TeamName::parse("alpha").unwrap(), false);
    assert_eq!(
        note,
        "[teamwire:alpha] no watch is running; start it in the background with `nohup teamwire watch alpha --deliver >/dev/null 2>&1 &` before you stop."
    );
}

impl Rig {
    fn afk_notes(&self, session: &str) -> Vec<String> {
        afk_hold_notes(self.home.path(), &sid(session), &self.platform, &self.clock)
    }

    /// Marks the team as an AFK run whose budget ends `hours` from now.
    fn make_afk(&self, store: &mut Store, hours: u64) {
        let ends = self
            .clock
            .now()
            .checked_add(Duration::from_secs(hours * 3600))
            .unwrap()
            .to_string();
        store
            .write(|tx| {
                tx.connection()
                    .execute("UPDATE team SET afk_budget_ends_at = ?1", [&ends])
                    .map(|_| ())
                    .map_err(Into::into)
            })
            .unwrap();
    }

    fn dir(&self) -> String {
        self.home.path().join("teams/alpha").display().to_string()
    }
}

#[test]
fn an_afk_team_with_nothing_to_wake_the_lead_gets_the_note() {
    let rig = Rig::new();
    let mut store = rig.team("alpha");
    rig.make_afk(&mut store, 8);
    complete(&mut store, &rig);
    let dir = rig.dir();
    assert_eq!(
        rig.afk_notes("lead-1"),
        [format!(
            "[teamwire:alpha] AFK run: nobody will answer you. If you were about to ask the user, decide it yourself, add it to {dir}/afk/decisions.md, and carry on. If the work is finished, add the reporter, then run `teamwire close alpha` once {dir}/afk/REPORT.md exists. If a command you started will wake you, stop again."
        )]
    );
}

#[test]
fn an_afk_team_with_a_working_member_and_no_watch_gets_the_note() {
    let rig = Rig::new();
    let mut store = rig.team("alpha");
    rig.make_afk(&mut store, 8);
    assert_eq!(rig.afk_notes("lead-1").len(), 1);
}

#[test]
fn a_working_member_and_a_live_watch_will_wake_the_lead() {
    let rig = Rig::new();
    let mut store = rig.team("alpha");
    rig.make_afk(&mut store, 8);
    rig.platform.set_alive(Pid::new(700), true);
    rig.lease(&mut store, 700);
    assert!(rig.afk_notes("lead-1").is_empty());
}

#[test]
fn a_live_watch_with_no_working_member_wakes_nobody() {
    let rig = Rig::new();
    let mut store = rig.team("alpha");
    rig.make_afk(&mut store, 8);
    complete(&mut store, &rig);
    rig.platform.set_alive(Pid::new(700), true);
    rig.lease(&mut store, 700);
    assert_eq!(rig.afk_notes("lead-1").len(), 1);
}

#[test]
fn an_ordinary_team_gets_no_afk_note() {
    let rig = Rig::new();
    let mut store = rig.team("alpha");
    complete(&mut store, &rig);
    assert!(rig.afk_notes("lead-1").is_empty());
}

#[test]
fn another_session_or_a_closed_team_gets_no_afk_note() {
    let rig = Rig::new();
    let mut store = rig.team("alpha");
    rig.make_afk(&mut store, 8);
    assert!(rig.afk_notes("someone-else").is_empty());
    store
        .write(|tx| {
            tx.connection()
                .execute("UPDATE team SET state = 'closed'", [])
                .map(|_| ())
                .map_err(Into::into)
        })
        .unwrap();
    assert!(rig.afk_notes("lead-1").is_empty());
}

#[test]
fn past_the_budget_the_note_says_so_and_asks_for_the_report() {
    let rig = Rig::new();
    let mut store = rig.team("alpha");
    rig.make_afk(&mut store, 1);
    let ends = rig
        .clock
        .now()
        .checked_add(Duration::from_secs(3600))
        .unwrap();
    rig.clock.advance(Duration::from_secs(3601));
    let notes = rig.afk_notes("lead-1");
    assert_eq!(notes.len(), 1);
    let note = notes.first().unwrap();
    assert!(
        note.starts_with(&format!(
            "[teamwire:alpha] budget spent at {ends}: finish what is running"
        )),
        "{}",
        note
    );
    assert!(note.contains(&format!("{}/afk/REPORT.md", rig.dir())));
}

#[test]
fn another_team_of_the_session_that_is_working_and_watched_wakes_the_lead() {
    let rig = Rig::new();
    let mut afk = rig.team("alpha");
    rig.make_afk(&mut afk, 8);
    complete(&mut afk, &rig);
    assert_eq!(
        rig.afk_notes("lead-1").len(),
        1,
        "nothing wakes the lead yet"
    );

    let mut beta = rig.team("beta");
    rig.platform.set_alive(Pid::new(700), true);
    rig.lease(&mut beta, 700);
    assert!(rig.afk_notes("lead-1").is_empty());
    // A team led by another session does not wake this lead.
    assert_eq!(rig.afk_notes("someone-else").len(), 0);
}
