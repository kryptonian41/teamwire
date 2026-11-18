//! The step from schema version 8 to 9 rebuilds `members` with the
//! `plan_reviewer` kind (ADR 0034), on databases that real teams already
//! hold. Unlike `events`, `members` is named by three other tables'
//! foreign keys, so these tests also check that every one of them still
//! names `members` after the step and that the store still enforces them.
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
    SCHEMA_V5_SQL, SCHEMA_V6_SQL, SCHEMA_V7_SQL, SCHEMA_V8_SQL, SCHEMA_VERSION, Store, StoreError,
    Waiver, database_path,
};

fn team() -> TeamName {
    TeamName::parse("alpha").unwrap()
}

fn at() -> Timestamp {
    Timestamp::from_unix_millis(1_790_000_000_000).unwrap()
}

/// A version-8 database with one member of each of the four old kinds, and
/// a completion, a verify record and a waiver that name members.
fn version_8_database(home: &TestHome) -> PathBuf {
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
    ] {
        conn.execute_batch(step).unwrap();
    }
    conn.pragma_update(None, "application_id", APPLICATION_ID)
        .unwrap();
    conn.pragma_update(None, "user_version", 8).unwrap();
    conn.execute_batch(
        "INSERT INTO members (role, title, kind, mode, harness, host, place, session_id,
             token_sha256, cwd, repo, branch, base, base_sha, check_cmd, owns, deadline_s,
             state, generation, gen_started_at, gate_told_generation, removed_at, removed_note)
         VALUES
           ('impl', 'Build it', 'implementer', 'worktree', 'claude-code', 'herdr', 'w1:p2',
            'sess-1', zeroblob(32), '/r/.worktrees/impl', '/r', 'alpha/impl', 'main', 'abc123',
            'cargo test', '[\"src/\"]', 1800, 'started', 2, '2026-10-01T00:00:01.000Z', 1,
            NULL, NULL),
           ('rev', 'Review it', 'reviewer', 'pane', 'pi', 'tmux', 'teamwire:1.2', NULL, NULL,
            '/r', NULL, NULL, NULL, NULL, NULL, '[]', 1200, 'removed', 1,
            '2026-10-01T00:00:02.000Z', NULL, '2026-10-01T01:00:00.000Z', 'done'),
           ('look', 'Look around', 'investigator', 'pane', 'codex', 'herdr', NULL, NULL, NULL,
            '/r', NULL, NULL, NULL, NULL, NULL, '[]', 900, 'failed', 1,
            '2026-10-01T00:00:03.000Z', NULL, NULL, NULL),
           ('closer', 'Close out', 'closer', 'pane', 'claude-code', 'herdr', NULL, NULL, NULL,
            '/r', NULL, NULL, NULL, NULL, NULL, '[]', 1800, 'creating', 1,
            '2026-10-01T00:00:04.000Z', NULL, NULL, NULL);
         INSERT INTO completions (ulid, role, generation, n, status, summary, final_message,
             final_source, report_state, delivering_since, route, reason, reason_detail,
             result_sha256, created_at)
         VALUES ('01J9ZQ000000000000000CMP01', 'impl', 1, 1, 'done', 'built it', 'all done',
             'hook', 'delivered', NULL, 'host_typing', NULL, NULL, zeroblob(32),
             '2026-10-01T00:30:00.000Z');
         INSERT INTO verify_records (role, head_sha, verdict, exit_code, seconds, flags,
             log_sha256, recorded_at)
         VALUES ('impl', 'def456', 'FLAGGED', 0, 12, '[\"outside-owns:README.md\"]',
             zeroblob(32), '2026-10-01T00:40:00.000Z');
         INSERT INTO waivers (role, flag_id, head_sha, reason, created_at)
         VALUES ('impl', 'outside-owns:README.md', 'def456', 'the brief asked for it',
             '2026-10-01T00:41:00.000Z');",
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

/// The rows of the four tables the step must keep whole.
fn snapshot(path: &Path) -> Vec<Vec<Vec<Value>>> {
    vec![
        rows(path, "members", "role"),
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

fn plan_reviewer(role: &str) -> NewMember {
    NewMember {
        role: Role::parse(role).unwrap(),
        title: "Review the plan".to_owned(),
        kind: MemberKind::PlanReviewer,
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

const RAW_PLAN_REVIEWER: &str = "INSERT INTO members (role, title, kind, mode, harness, host,
    cwd, deadline_s, state, generation, gen_started_at)
    VALUES ('plan', 'Review the plan', 'plan_reviewer', 'pane', 'pi', 'herdr', '/r', 1200,
    'started', 1, '2026-10-01T00:00:05.000Z')";

/// Version 10 adds ticket refs after this step, version 11 the
/// `member.woken` event type (ADR 0043), and version 12 the three dialog
/// event types (ADR 0052), so a store opened here ends at version 12.
#[test]
fn this_build_is_at_schema_version_12() {
    assert_eq!(SCHEMA_VERSION, 12);
}

#[test]
fn a_plan_reviewer_is_refused_before_the_step_and_accepted_after() {
    let home = TestHome::new().unwrap();
    let path = version_8_database(&home);
    let before = Connection::open(&path)
        .unwrap()
        .execute_batch(RAW_PLAN_REVIEWER);
    assert!(
        before.is_err(),
        "a version-8 members table accepted the new kind"
    );

    let mut store = Store::open(home.path(), &team()).unwrap();
    store
        .write(|tx| tx.insert_member(at(), &plan_reviewer("plan")))
        .unwrap();
    let member = store
        .read(|tx| tx.member(&Role::parse("plan").unwrap()))
        .unwrap()
        .unwrap();
    assert_eq!(member.kind, MemberKind::PlanReviewer);
}

#[test]
fn a_version_8_database_keeps_every_member_and_every_row_that_names_one() {
    let home = TestHome::new().unwrap();
    let path = version_8_database(&home);
    let before = snapshot(&path);
    assert_eq!(before.first().unwrap().len(), 4);

    let store = Store::open(home.path(), &team()).unwrap();
    assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
    drop(store);

    assert_eq!(snapshot(&path), before);
    assert_eq!(leftovers(&path), 0);
}

#[test]
fn every_foreign_key_still_names_members_after_the_step() {
    let home = TestHome::new().unwrap();
    let path = version_8_database(&home);
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
         VALUES ('ghost', 'x', 'y', 'z', '2026-10-01T00:00:00.000Z')",
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
    version_8_database(&home);
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
        stored.is_err(),
        "a waiver for a role with no member was stored"
    );
}

#[test]
fn a_read_only_open_reads_a_version_8_database_and_writes_nothing() {
    let home = TestHome::new().unwrap();
    let path = version_8_database(&home);
    let bytes = std::fs::read(&path).unwrap();

    let store = Store::open_read_only(home.path(), &team()).unwrap();
    assert_eq!(store.schema_version().unwrap(), 8);
    #[expect(
        clippy::redundant_closure_for_method_calls,
        reason = "a method path does not satisfy `Store::read`'s closure bound: it takes a reference to a type with its own lifetime"
    )]
    let members = store.read(|tx| tx.members()).unwrap();
    assert_eq!(members.items.len(), 4);
    drop(store);

    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

/// One stored member has a kind that no list holds, so the copy into the
/// new table fails partway through the step. The open must fail and leave
/// version 8 with all its rows, as a crash at that point would.
#[test]
fn a_step_that_fails_midway_leaves_the_version_8_database_whole() {
    let home = TestHome::new().unwrap();
    let path = version_8_database(&home);
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "PRAGMA ignore_check_constraints = ON;
             INSERT INTO members (role, title, kind, mode, harness, host, cwd, deadline_s,
                 state, generation, gen_started_at)
             VALUES ('odd', 'Odd one', 'not_a_kind', 'pane', 'pi', 'herdr', '/r', 60,
                 'started', 1, '2026-10-01T00:00:06.000Z');",
        )
        .unwrap();
    let before = snapshot(&path);
    assert_eq!(before.first().unwrap().len(), 5);

    assert!(Store::open(home.path(), &team()).is_err());

    assert_eq!(snapshot(&path), before);
    let version: u32 = Connection::open(&path)
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 8);
    assert_eq!(leftovers(&path), 0);
}

/// An open that finds the database already at the current version runs no
/// step, so it never turns foreign key checks off. Its writes must be
/// checked too.
#[test]
fn a_store_opened_on_a_current_database_enforces_foreign_keys() {
    let home = TestHome::new().unwrap();
    version_8_database(&home);
    drop(Store::open(home.path(), &team()).unwrap());

    let mut store = Store::open(home.path(), &team()).unwrap();
    assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
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

/// The store writes with foreign key checks on, so a completion for a role
/// with no member means the file was damaged. The step runs with the checks
/// off, so only the key check before its commit can refuse such a file.
#[test]
fn a_completion_that_names_no_member_fails_the_open_and_keeps_version_8() {
    let home = TestHome::new().unwrap();
    let path = version_8_database(&home);
    // The bundled SQLite starts every connection with the checks on.
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "PRAGMA foreign_keys = OFF;
             INSERT INTO completions (ulid, role, generation, n, status, summary,
                 final_source, report_state, created_at)
             VALUES ('01J9ZQ000000000000000CMP02', 'ghost', 1, 1, 'done', 'gone',
                 'none', 'pending', '2026-10-01T00:50:00.000Z');",
        )
        .unwrap();
    assert_eq!(rows(&path, "completions", "ulid").len(), 2);
    let before = snapshot(&path);

    let refused = Store::open(home.path(), &team()).unwrap_err();
    assert!(
        matches!(&refused, StoreError::Corrupt(text) if text.contains("completions")),
        "{refused:?}"
    );

    assert_eq!(snapshot(&path), before);
    let version: u32 = Connection::open(&path)
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 8);
    assert_eq!(leftovers(&path), 0);
}

/// The rebuilt table has the old table's columns, each with its type,
/// NOT NULL, default and place in the primary key, and stays STRICT.
#[test]
fn every_column_of_members_is_the_same_after_the_step() {
    let home = TestHome::new().unwrap();
    let path = version_8_database(&home);
    let columns = rows(&path, "pragma_table_xinfo('members')", "cid");
    assert_eq!(columns.len(), 23);
    let table = rows(&path, "pragma_table_list('members')", "name");
    assert_eq!(table.len(), 1);

    drop(Store::open(home.path(), &team()).unwrap());

    assert_eq!(rows(&path, "pragma_table_xinfo('members')", "cid"), columns);
    assert_eq!(rows(&path, "pragma_table_list('members')", "name"), table);
}

/// A member row that every CHECK of `members` takes.
const GOOD_MEMBER: [(&str, &str); 13] = [
    ("role", "'checked'"),
    ("title", "'Check it'"),
    ("kind", "'plan_reviewer'"),
    ("mode", "'pane'"),
    ("harness", "'pi'"),
    ("host", "'herdr'"),
    ("token_sha256", "zeroblob(32)"),
    ("cwd", "'/r'"),
    ("owns", "'[\"src/\"]'"),
    ("deadline_s", "60"),
    ("state", "'started'"),
    ("generation", "1"),
    ("gen_started_at", "'2026-10-01T00:00:07.000Z'"),
];

/// Inserts `GOOD_MEMBER` with `column` set to the SQL value `value`.
fn insert_member_with(conn: &Connection, column: &str, value: &str) -> rusqlite::Result<()> {
    let names: Vec<&str> = GOOD_MEMBER.iter().map(|(c, _)| *c).collect();
    let values: Vec<&str> = GOOD_MEMBER
        .iter()
        .map(|(c, v)| if *c == column { value } else { *v })
        .collect();
    conn.execute_batch(&format!(
        "INSERT INTO members ({}) VALUES ({})",
        names.join(", "),
        values.join(", ")
    ))
}

#[test]
fn every_other_check_of_members_still_refuses_a_row_that_breaks_it() {
    let home = TestHome::new().unwrap();
    let path = version_8_database(&home);
    drop(Store::open(home.path(), &team()).unwrap());
    let conn = Connection::open(&path).unwrap();

    for (column, bad) in [
        ("mode", "'not_a_mode'"),
        ("state", "'not_a_state'"),
        ("token_sha256", "zeroblob(31)"),
        ("owns", "'[\"src/\"'"),
        ("deadline_s", "0"),
        ("generation", "0"),
    ] {
        let refused = insert_member_with(&conn, column, bad)
            .unwrap_err()
            .to_string();
        assert!(
            refused.contains("CHECK constraint failed") && refused.contains(column),
            "{column}: {refused}"
        );
    }
    // The same row with no broken value is stored, so each refusal above
    // came from the one value that broke its CHECK.
    insert_member_with(&conn, "", "").unwrap();
}
