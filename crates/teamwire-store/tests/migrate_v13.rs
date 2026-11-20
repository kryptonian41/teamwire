//! The step from schema version 12 to 13 rebuilds `members` with the
//! `reconciler` kind and two new columns, `quiet` and `set_label`, on
//! databases that real teams already hold. As for the step to version 9,
//! three other tables name `members` by foreign key, so these tests also
//! check that every one of them still names `members` after the step and
//! that the store still enforces them.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use rusqlite::types::Value;
use teamwire_core::{HarnessId, HostId, MemberKind, Mode, Role, TeamName, Timestamp};
use teamwire_fakes::TestHome;
use teamwire_store::{
    APPLICATION_ID, NewMember, SCHEMA_V1_SQL, SCHEMA_V2_SQL, SCHEMA_V3_SQL, SCHEMA_V4_SQL,
    SCHEMA_V5_SQL, SCHEMA_V6_SQL, SCHEMA_V7_SQL, SCHEMA_V8_SQL, SCHEMA_V9_SQL, SCHEMA_V10_SQL,
    SCHEMA_V11_SQL, SCHEMA_V12_SQL, Store, StoreError, Waiver, database_path,
};

fn team() -> TeamName {
    TeamName::parse("alpha").unwrap()
}

fn at() -> Timestamp {
    Timestamp::from_unix_millis(1_790_000_000_000).unwrap()
}

/// The roles of `version_12_database`, in the order they were inserted.
/// Sorted by name they come out in another order.
const ROLES: [&str; 5] = ["impl", "rev", "look", "closer", "plan"];

/// A version-12 database with one member of each of the five old kinds,
/// inserted in the order of `ROLES`, and a completion, a verify record and
/// a waiver that name members.
fn version_12_database(home: &TestHome) -> PathBuf {
    let path = database_path(home.path(), &team());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let conn = Connection::open(&path).unwrap();
    for step in [
        SCHEMA_V1_SQL,
        SCHEMA_V2_SQL,
        SCHEMA_V3_SQL,
        SCHEMA_V4_SQL,
        SCHEMA_V5_SQL,
        SCHEMA_V6_SQL,
        SCHEMA_V7_SQL,
        SCHEMA_V8_SQL,
        SCHEMA_V9_SQL,
        SCHEMA_V10_SQL,
        SCHEMA_V11_SQL,
        SCHEMA_V12_SQL,
    ] {
        conn.execute_batch(step).unwrap();
    }
    conn.pragma_update(None, "application_id", APPLICATION_ID)
        .unwrap();
    conn.pragma_update(None, "user_version", 12).unwrap();
    conn.execute_batch(
        "INSERT INTO members (role, title, kind, mode, harness, host, place, session_id,
             token_sha256, cwd, repo, branch, base, base_sha, check_cmd, owns, deadline_s,
             state, generation, gen_started_at, gate_told_generation, removed_at, removed_note)
         VALUES
           ('impl', 'Build it', 'implementer', 'worktree', 'claude-code', 'herdr', 'w1:p2',
            'sess-1', zeroblob(32), '/r/.worktrees/impl', '/r', 'alpha/impl', 'main', 'abc123',
            'cargo test', '[\"src/\"]', 1800, 'started', 2, '2026-11-01T00:00:01.000Z', 1,
            NULL, NULL),
           ('rev', 'Review it', 'reviewer', 'pane', 'pi', 'tmux', 'teamwire:1.2', NULL, NULL,
            '/r', NULL, NULL, NULL, NULL, NULL, '[]', 1200, 'removed', 1,
            '2026-11-01T00:00:02.000Z', NULL, '2026-11-01T01:00:00.000Z', 'done'),
           ('look', 'Look around', 'investigator', 'pane', 'pi', 'herdr', NULL, NULL, NULL,
            '/r', NULL, NULL, NULL, NULL, NULL, '[]', 900, 'failed', 1,
            '2026-11-01T00:00:03.000Z', NULL, NULL, NULL),
           ('closer', 'Close out', 'closer', 'pane', 'claude-code', 'herdr', NULL, NULL, NULL,
            '/r', NULL, NULL, NULL, NULL, NULL, '[]', 1800, 'creating', 1,
            '2026-11-01T00:00:04.000Z', NULL, NULL, NULL),
           ('plan', 'Review the plan', 'plan_reviewer', 'pane', 'pi', 'herdr', NULL, NULL, NULL,
            '/r', NULL, NULL, NULL, NULL, NULL, '[]', 1200, 'started', 1,
            '2026-11-01T00:00:05.000Z', NULL, NULL, NULL);
         INSERT INTO completions (ulid, role, generation, n, status, summary, final_message,
             final_source, report_state, delivering_since, route, reason, reason_detail,
             result_sha256, created_at)
         VALUES ('01J9ZQ000000000000000CMP01', 'impl', 1, 1, 'done', 'built it', 'all done',
             'hook', 'delivered', NULL, 'host_typing', NULL, NULL, zeroblob(32),
             '2026-11-01T00:30:00.000Z');
         INSERT INTO verify_records (role, head_sha, verdict, exit_code, seconds, flags,
             log_sha256, recorded_at)
         VALUES ('impl', 'def456', 'FLAGGED', 0, 12, '[\"outside-owns:README.md\"]',
             zeroblob(32), '2026-11-01T00:40:00.000Z');
         INSERT INTO waivers (role, flag_id, head_sha, reason, created_at)
         VALUES ('impl', 'outside-owns:README.md', 'def456', 'the brief asked for it',
             '2026-11-01T00:41:00.000Z');",
    )
    .unwrap();
    path
}

/// Every row of `table`, every column as stored, in a fixed order.
fn rows(path: &Path, table: &str, order: &str) -> Vec<Vec<Value>> {
    let conn = Connection::open(path).unwrap();
    let mut stmt = conn
        .prepare(&format!("SELECT * FROM {table} ORDER BY {order}"))
        .unwrap();
    let width = stmt.column_count();
    stmt.query_map([], |r| (0..width).map(|i| r.get::<_, Value>(i)).collect())
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// The rows of the three tables that name members, which the step must keep
/// exactly as they are.
fn children(path: &Path) -> Vec<Vec<Vec<Value>>> {
    vec![
        rows(path, "completions", "ulid"),
        rows(path, "verify_records", "id"),
        rows(path, "waivers", "role, flag_id, head_sha"),
    ]
}

/// How many schema objects mention a name the step may use for the old or
/// the new table while it works.
fn leftovers(path: &Path) -> i64 {
    Connection::open(path)
        .unwrap()
        .query_row(
            "SELECT count(*) FROM sqlite_schema
             WHERE name LIKE 'members\\_%' ESCAPE '\\'
                OR sql LIKE '%members\\_old%' ESCAPE '\\'
                OR sql LIKE '%members\\_new%' ESCAPE '\\'",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

fn user_version(path: &Path) -> u32 {
    Connection::open(path)
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}

fn reconciler(role: &str) -> NewMember {
    NewMember {
        role: Role::parse(role).unwrap(),
        title: "Reconcile the set".to_owned(),
        kind: MemberKind::Reconciler,
        mode: Mode::Pane,
        harness: HarnessId::from_static("pi"),
        host: HostId::from_static("herdr"),
        cwd: PathBuf::from("/r"),
        repo: None,
        branch: None,
        base: None,
        base_sha: None,
        check: None,
        owns: Vec::new(),
        deadline_s: 1200,
    }
}

fn store_members(store: &Store) -> Vec<teamwire_core::Member> {
    #[expect(
        clippy::redundant_closure_for_method_calls,
        reason = "a method path does not satisfy `Store::read`'s closure bound: it takes a reference to a type with its own lifetime"
    )]
    let listing = store.read(|tx| tx.members()).unwrap();
    assert!(listing.bad.is_empty(), "{:?}", listing.bad);
    listing.items
}

#[test]
fn a_reconciler_is_refused_before_the_step_and_accepted_after() {
    let home = TestHome::new().unwrap();
    let path = version_12_database(&home);
    let before = Connection::open(&path).unwrap().execute_batch(
        "INSERT INTO members (role, title, kind, mode, harness, host, cwd, deadline_s, state,
             generation, gen_started_at)
         VALUES ('rec', 'Reconcile', 'reconciler', 'pane', 'pi', 'herdr', '/r', 1200,
             'started', 1, '2026-11-01T00:00:06.000Z')",
    );
    assert!(
        before.is_err(),
        "a version-12 members table accepted the new kind"
    );

    let mut store = Store::open(home.path(), &team()).unwrap();
    store
        .write(|tx| tx.insert_member(at(), &reconciler("rec")))
        .unwrap();
    let member = store
        .read(|tx| tx.member(&Role::parse("rec").unwrap()))
        .unwrap()
        .unwrap();
    assert_eq!(member.kind, MemberKind::Reconciler);
    assert!(member.is_quiet());
}

#[test]
fn a_version_12_database_keeps_every_member_and_every_row_that_names_one() {
    let home = TestHome::new().unwrap();
    let path = version_12_database(&home);
    let members_before = rows(&path, "members", "role");
    assert_eq!(members_before.len(), 5);
    let children_before = children(&path);

    let store = Store::open(home.path(), &team()).unwrap();
    assert_eq!(store.schema_version().unwrap(), 13);
    let read: Vec<(String, bool, Option<String>)> = store_members(&store)
        .into_iter()
        .map(|m| (m.role.as_str().to_owned(), m.quiet, m.set))
        .collect();
    let want: Vec<(String, bool, Option<String>)> = ROLES
        .iter()
        .map(|r| ((*r).to_owned(), false, None))
        .collect();
    assert_eq!(read, want, "every member, in the order it was inserted");
    drop(store);

    // Each row keeps its version 12 values in order, then takes the two
    // defaults: `quiet` 0 and `set_label` NULL.
    let members_after = rows(&path, "members", "role");
    assert_eq!(members_after.len(), members_before.len());
    for (after, before) in members_after.iter().zip(&members_before) {
        assert_eq!(after.get(..before.len()), Some(before.as_slice()));
        assert_eq!(
            after.get(before.len()..),
            Some([Value::Integer(0), Value::Null].as_slice())
        );
    }
    assert_eq!(children(&path), children_before);
    assert_eq!(leftovers(&path), 0);
}

#[test]
fn every_foreign_key_still_names_members_after_the_step() {
    let home = TestHome::new().unwrap();
    let path = version_12_database(&home);
    drop(Store::open(home.path(), &team()).unwrap());

    let conn = Connection::open(&path).unwrap();
    for table in ["completions", "verify_records", "waivers"] {
        let targets: Vec<String> = conn
            .prepare(&format!(
                "SELECT \"table\" FROM pragma_foreign_key_list('{table}')"
            ))
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(targets, ["members"], "{table}");
    }
    let broken: i64 = conn
        .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(broken, 0);

    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    let ghost = conn.execute_batch(
        "INSERT INTO waivers (role, flag_id, head_sha, reason, created_at)
         VALUES ('ghost', 'x', 'y', 'z', '2026-11-01T00:00:00.000Z')",
    );
    assert!(
        ghost.is_err(),
        "a waiver for a role with no member was stored"
    );
}

/// The step turns foreign key checks off while it rebuilds the table. The
/// store that ran the step must have them on again for its own writes.
#[test]
fn the_store_that_ran_the_step_still_enforces_foreign_keys() {
    let home = TestHome::new().unwrap();
    version_12_database(&home);
    let mut store = Store::open(home.path(), &team()).unwrap();
    let waiver = Waiver {
        role: Role::parse("ghost").unwrap(),
        flag_id: "outside-owns:x".to_owned(),
        head_sha: "abc".to_owned(),
        reason: "none".to_owned(),
        created_at: at(),
    };
    let stored = store.write(|tx| tx.insert_waiver(at(), &waiver));
    assert!(
        matches!(stored, Err(StoreError::Constraint(_))),
        "{stored:?}"
    );
}

/// A read-only open never migrates. It reads the two new columns as their
/// defaults, every member in the order it was inserted, and leaves every
/// byte of the file as it was.
#[test]
fn a_read_only_open_reads_a_version_12_database_and_writes_nothing() {
    let home = TestHome::new().unwrap();
    let path = version_12_database(&home);
    let bytes = std::fs::read(&path).unwrap();

    let store = Store::open_read_only(home.path(), &team()).unwrap();
    assert_eq!(store.schema_version().unwrap(), 12);
    let members = store_members(&store);
    let roles: Vec<&str> = members.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, ROLES);
    assert!(members.iter().all(|m| !m.quiet && m.set.is_none()));
    let rev = store
        .read(|tx| tx.member(&Role::parse("rev").unwrap()))
        .unwrap()
        .unwrap();
    assert!(rev.is_quiet() && !rev.quiet && rev.set.is_none());
    drop(store);

    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

/// One stored member has a kind that no list holds, so the copy into the
/// new table fails partway through the step. The open must fail and leave
/// version 12 with all its rows, as a crash at that point would.
#[test]
fn a_step_that_fails_midway_leaves_the_version_12_database_whole() {
    let home = TestHome::new().unwrap();
    let path = version_12_database(&home);
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "PRAGMA ignore_check_constraints = ON;
             INSERT INTO members (role, title, kind, mode, harness, host, cwd, deadline_s,
                 state, generation, gen_started_at)
             VALUES ('odd', 'Odd one', 'not_a_kind', 'pane', 'pi', 'herdr', '/r', 60,
                 'started', 1, '2026-11-01T00:00:06.000Z');",
        )
        .unwrap();
    let members = rows(&path, "members", "role");
    assert_eq!(members.len(), 6);
    let others = children(&path);

    assert!(Store::open(home.path(), &team()).is_err());

    assert_eq!(rows(&path, "members", "role"), members);
    assert_eq!(children(&path), others);
    assert_eq!(user_version(&path), 12);
    assert_eq!(leftovers(&path), 0);
}

/// The store writes with foreign key checks on, so a completion for a role
/// with no member means the file was damaged. The step runs with the checks
/// off, so only the key check before its commit can refuse such a file.
#[test]
fn a_completion_that_names_no_member_fails_the_open_and_keeps_version_12() {
    let home = TestHome::new().unwrap();
    let path = version_12_database(&home);
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "PRAGMA foreign_keys = OFF;
             INSERT INTO completions (ulid, role, generation, n, status, summary,
                 final_source, report_state, created_at)
             VALUES ('01J9ZQ000000000000000CMP02', 'ghost', 1, 1, 'done', 'gone',
                 'none', 'pending', '2026-11-01T00:50:00.000Z');",
        )
        .unwrap();
    let members = rows(&path, "members", "role");
    let others = children(&path);

    let refused = Store::open(home.path(), &team()).unwrap_err();
    assert!(
        matches!(&refused, StoreError::Corrupt(text) if text.contains("completions")),
        "{refused:?}"
    );

    assert_eq!(rows(&path, "members", "role"), members);
    assert_eq!(children(&path), others);
    assert_eq!(user_version(&path), 12);
    assert_eq!(leftovers(&path), 0);
}

/// The rebuilt table keeps every version 12 column with its type, NOT NULL,
/// default and place, stays STRICT, and adds exactly `quiet` and
/// `set_label` after them.
#[test]
fn every_column_of_members_is_kept_and_two_are_added() {
    let home = TestHome::new().unwrap();
    let path = version_12_database(&home);
    let columns = rows(&path, "pragma_table_xinfo('members')", "cid");
    assert_eq!(columns.len(), 23);
    let strict: i64 = Connection::open(&path)
        .unwrap()
        .query_row("SELECT strict FROM pragma_table_list('members')", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(strict, 1);

    drop(Store::open(home.path(), &team()).unwrap());

    let after = rows(&path, "pragma_table_xinfo('members')", "cid");
    assert_eq!(after.get(..columns.len()), Some(columns.as_slice()));
    let text = |s: &str| Value::Text(s.to_owned());
    // cid, name, type, notnull, dflt_value, pk, hidden
    assert_eq!(
        after.get(columns.len()..),
        Some(
            [
                vec![
                    Value::Integer(23),
                    text("quiet"),
                    text("INTEGER"),
                    Value::Integer(1),
                    text("0"),
                    Value::Integer(0),
                    Value::Integer(0),
                ],
                vec![
                    Value::Integer(24),
                    text("set_label"),
                    text("TEXT"),
                    Value::Integer(0),
                    Value::Null,
                    Value::Integer(0),
                    Value::Integer(0),
                ],
            ]
            .as_slice()
        )
    );
    let strict: i64 = Connection::open(&path)
        .unwrap()
        .query_row("SELECT strict FROM pragma_table_list('members')", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(strict, 1);
}

#[test]
fn the_quiet_column_takes_only_0_and_1() {
    let home = TestHome::new().unwrap();
    let path = version_12_database(&home);
    drop(Store::open(home.path(), &team()).unwrap());
    let conn = Connection::open(&path).unwrap();
    let insert = |role: &str, quiet: &str| {
        conn.execute_batch(&format!(
            "INSERT INTO members (role, title, kind, mode, harness, host, cwd, deadline_s,
                 state, generation, gen_started_at, quiet)
             VALUES ('{role}', '{role}', 'investigator', 'pane', 'pi', 'herdr', '/r', 60,
                 'started', 1, '2026-11-01T00:00:07.000Z', {quiet})"
        ))
    };
    insert("zero", "0").unwrap();
    insert("one", "1").unwrap();
    for (role, bad) in [("two", "2"), ("none", "NULL")] {
        let refused = insert(role, bad).unwrap_err().to_string();
        assert!(refused.contains("quiet"), "{bad}: {refused}");
    }
}

/// The marks launch writes after the step are read back, and a member
/// whose spec set `quiet` is quiet whatever its kind.
#[test]
fn marks_written_after_the_step_are_read_back() {
    let home = TestHome::new().unwrap();
    version_12_database(&home);
    let mut store = Store::open(home.path(), &team()).unwrap();
    let rec = Role::parse("rec").unwrap();
    let look = Role::parse("look").unwrap();
    store
        .write(|tx| {
            tx.insert_member(at(), &reconciler("rec"))?;
            tx.set_member_marks(&rec, false, Some("rev1"))?;
            tx.set_member_marks(&look, true, Some("rev1"))
        })
        .unwrap();
    let read = |role: &Role| store.read(|tx| tx.member(role)).unwrap().unwrap();
    let (rec, look) = (read(&rec), read(&look));
    assert_eq!((rec.quiet, rec.set.as_deref()), (false, Some("rev1")));
    assert!(rec.is_quiet());
    assert_eq!((look.quiet, look.set.as_deref()), (true, Some("rev1")));
    assert!(look.is_quiet());
    assert_eq!(look.kind, MemberKind::Investigator);
}
