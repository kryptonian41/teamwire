//! Shared helpers for the unit tests of the typed methods.

use teamwire_core::{
    Event, EventType, HarnessId, HostId, LeadRoute, MemberKind, Mode, Recipient, Role, TeamName,
    Timestamp,
};
use teamwire_fakes::TestHome;

use crate::{NewMember, NewTeam, Store};

pub(crate) struct Fixture {
    pub(crate) home: TestHome,
    pub(crate) store: Store,
}

pub(crate) fn fixture() -> Fixture {
    let home = TestHome::new().unwrap();
    let store = Store::open(home.path(), &TeamName::parse("alpha").unwrap()).unwrap();
    Fixture { home, store }
}

/// A fixed time `secs` seconds after an arbitrary start.
pub(crate) fn at(secs: i64) -> Timestamp {
    Timestamp::from_unix_millis(1_790_000_000_000 + secs * 1000).unwrap()
}

pub(crate) fn role(name: &str) -> Role {
    Role::parse(name).unwrap()
}

pub(crate) fn member(name: &str) -> Recipient {
    Recipient::Member(role(name))
}

pub(crate) fn new_team() -> NewTeam {
    NewTeam {
        name: TeamName::parse("alpha").unwrap(),
        title: "Alpha".to_owned(),
        lead_harness: HarnessId::from_static("claude-code"),
        lead_host: HostId::from_static("herdr"),
        lead_route: LeadRoute::Sink,
        lead_session: None,
    }
}

pub(crate) fn new_member(name: &str) -> NewMember {
    NewMember {
        role: role(name),
        title: format!("the {name}"),
        kind: MemberKind::Implementer,
        mode: Mode::Pane,
        harness: HarnessId::from_static("claude-code"),
        host: HostId::from_static("herdr"),
        cwd: "/work".into(),
        repo: None,
        branch: None,
        base: None,
        base_sha: None,
        check: Some("cargo test".to_owned()),
        owns: vec!["src/".to_owned()],
        deadline_s: 1800,
    }
}

/// Every event, oldest first.
pub(crate) fn events(store: &Store) -> Vec<Event> {
    store
        .read(|tx| tx.events_after(0, None, None)?.all_or_corrupt())
        .unwrap()
}

pub(crate) fn event_types(store: &Store) -> Vec<&'static str> {
    events(store).iter().map(|e| e.kind.as_str()).collect()
}

/// The newest event; every method that changes a row must have written one.
pub(crate) fn last_event(store: &Store) -> Event {
    events(store).pop().unwrap()
}

pub(crate) fn count_events(store: &Store, kind: EventType) -> usize {
    events(store).iter().filter(|e| e.kind == kind).count()
}

/// A store with the team and the named members (all `creating`).
pub(crate) fn with_members(names: &[&str]) -> Fixture {
    let mut f = fixture();
    f.store
        .write(|tx| {
            tx.create_team(at(0), &new_team())?;
            for name in names {
                tx.insert_member(at(1), &new_member(name))?;
            }
            Ok(())
        })
        .unwrap();
    f
}
