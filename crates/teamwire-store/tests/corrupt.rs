//! Damaged rows: reported and skipped, never read as success; a damaged
//! counter never hands out an id twice.
#![expect(
    clippy::indexing_slicing,
    reason = "a test that indexes past the end panics and fails, which is the intended outcome"
)]
#![expect(
    clippy::redundant_closure_for_method_calls,
    reason = "a method path does not satisfy `Store::read`'s closure bound: it takes a reference to a type with its own lifetime"
)]
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

use rusqlite::Connection;
use teamwire_core::{
    Actor, HarnessId, HostId, LeadRoute, MemberKind, MessageType, Mode, Recipient, Role, TeamName,
    Timestamp,
};
use teamwire_fakes::TestHome;
use teamwire_store::{
    MessageFilter, NewMember, NewMessage, NewTeam, Store, StoreError, database_path,
};

fn at(n: i64) -> Timestamp {
    Timestamp::from_unix_millis(1_790_000_000_000 + n).unwrap()
}

fn role(name: &str) -> Role {
    Role::parse(name).unwrap()
}

fn team() -> TeamName {
    TeamName::parse("alpha").unwrap()
}

fn setup(home: &TestHome, members: &[&str]) -> Store {
    let mut store = Store::open(home.path(), &team()).unwrap();
    store
        .write(|tx| {
            tx.create_team(
                at(0),
                &NewTeam {
                    name: team(),
                    title: "t".to_owned(),
                    lead_harness: HarnessId::from_static("claude-code"),
                    lead_host: HostId::from_static("herdr"),
                    lead_route: LeadRoute::Sink,
                    lead_session: None,
                },
            )?;
            for name in members {
                tx.insert_member(
                    at(1),
                    &NewMember {
                        role: role(name),
                        title: "t".to_owned(),
                        kind: MemberKind::Reviewer,
                        mode: Mode::Pane,
                        harness: HarnessId::from_static("claude-code"),
                        host: HostId::from_static("herdr"),
                        cwd: "/w".into(),
                        repo: None,
                        branch: None,
                        base: None,
                        base_sha: None,
                        check: None,
                        owns: vec!["src/".to_owned()],
                        deadline_s: 60,
                    },
                )?;
            }
            Ok(())
        })
        .unwrap();
    store
}

fn send(store: &mut Store, body: &str) -> Result<String, StoreError> {
    store.write(|tx| {
        tx.insert_message(
            at(10),
            &NewMessage {
                sender: Actor::Member(role("api")),
                kind: MessageType::Note,
                re: None,
                commit: None,
                body: body.to_owned(),
                client_key: None,
                recipients: vec![Recipient::Lead],
            },
        )
        .map(|s| s.message.display_id.to_string())
    })
}

fn raw(home: &TestHome) -> Connection {
    Connection::open(database_path(home.path(), &team())).unwrap()
}

#[test]
fn bad_row_skipped_and_reported() {
    let home = TestHome::new().unwrap();
    let mut store = setup(&home, &["api", "web", "doc"]);
    send(&mut store, "first").unwrap();
    send(&mut store, "second").unwrap();
    {
        let conn = raw(&home);
        // A time nobody could have written, and a JSON field of the wrong shape.
        conn.execute(
            "UPDATE members SET gen_started_at = 'yesterday-ish' WHERE role = 'api'",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE members SET owns = '{\"not\": \"a list\"}' WHERE role = 'doc'",
            [],
        )
        .unwrap();
        // A message whose body no longer matches its hash.
        conn.execute(
            "UPDATE messages SET body = 'edited' WHERE display_id = 'm001'",
            [],
        )
        .unwrap();
    }

    let listing = store.read(|tx| tx.members()).unwrap();
    let ok: Vec<&str> = listing.items.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(ok, ["web"], "a damaged member was read as good");
    let mut bad: Vec<(&str, &str)> = listing
        .bad
        .iter()
        .map(|b| (b.table, b.key.as_str()))
        .collect();
    bad.sort_unstable();
    assert_eq!(bad, [("members", "api"), ("members", "doc")]);
    assert!(listing.bad.iter().all(|b| !b.reason.is_empty()));
    assert!(matches!(
        listing.all_or_corrupt(),
        Err(StoreError::Corrupt(_))
    ));

    // A single lookup of a damaged row is an error, not "not found".
    let one = store.read(|tx| tx.member(&role("api")));
    assert!(matches!(one, Err(StoreError::Corrupt(_))), "{one:?}");
    assert!(store.read(|tx| tx.member(&role("web"))).unwrap().is_some());

    let messages = store
        .read(|tx| tx.messages(&MessageFilter::default()))
        .unwrap();
    assert_eq!(messages.items.len(), 1);
    assert_eq!(messages.items[0].body, "second");
    assert_eq!(messages.bad.len(), 1);
    assert_eq!(messages.bad[0].table, "messages");
    assert!(
        messages.bad[0].reason.contains("hash"),
        "{}",
        messages.bad[0].reason
    );

    // The damage does not stop the store from working on the good rows.
    assert_eq!(send(&mut store, "third").unwrap(), "m003");
}

#[test]
fn counter_damage_never_reuses_id() {
    let home = TestHome::new().unwrap();
    let mut store = setup(&home, &["api"]);
    for body in ["a", "b", "c"] {
        send(&mut store, body).unwrap();
    }
    let events_before: i64 = store
        .read(|tx| {
            Ok(tx
                .connection()
                .query_row("SELECT count(*) FROM events", [], |r| r.get(0))?)
        })
        .unwrap();

    for (counter, damaged) in [("display_id", 1), ("seq:api", 0)] {
        raw(&home)
            .execute(
                "UPDATE counters SET value = ?2 WHERE name = ?1",
                rusqlite::params![counter, damaged],
            )
            .unwrap();
        let refused = send(&mut store, "collides");
        assert!(
            matches!(refused, Err(StoreError::Constraint(_))),
            "{counter}: {refused:?}"
        );
        // The failed transaction left nothing: no row, no event, no counter move.
        let (rows, events, value): (i64, i64, i64) = store
            .read(|tx| {
                let c = tx.connection();
                Ok((
                    c.query_row("SELECT count(*) FROM messages", [], |r| r.get(0))?,
                    c.query_row("SELECT count(*) FROM events", [], |r| r.get(0))?,
                    c.query_row(
                        "SELECT value FROM counters WHERE name = ?1",
                        [counter],
                        |r| r.get(0),
                    )?,
                ))
            })
            .unwrap();
        assert_eq!(
            (rows, events, value),
            (3, events_before, damaged),
            "{counter}"
        );
        // Put the counter right again: numbering continues without a repeat.
        raw(&home)
            .execute("UPDATE counters SET value = 3 WHERE name = ?1", [counter])
            .unwrap();
    }
    assert_eq!(send(&mut store, "d").unwrap(), "m004");
    let mut ids: Vec<String> = store
        .read(|tx| tx.messages(&MessageFilter::default())?.all_or_corrupt())
        .unwrap()
        .iter()
        .map(|m| m.display_id.to_string())
        .collect();
    ids.dedup();
    assert_eq!(ids, ["m001", "m002", "m003", "m004"]);
}
