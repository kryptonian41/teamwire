//! Reading decisions across running teams and archives, in a temporary home
//! with made-up reports.
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods,
    reason = "a failing unwrap or index in an integration test fails the test, and the test writes the reports a member would write"
)]

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use teamwire_core::{
    HarnessId, HostId, LeadRoute, MemberKind, Mode, Role, TeamName, TicketRef, Timestamp,
};
use teamwire_fakes::TestHome;
use teamwire_report::{DecisionsError, DecisionsQuery, RepoScope, read_decisions};
use teamwire_store::{NewMember, NewTeam, Store, database_path};

fn at(secs: i64) -> Timestamp {
    Timestamp::from_unix_millis(1_790_000_000_000 + secs * 1000).unwrap()
}

/// A team created at `secs` with one implementer per role.
fn team(home: &Path, name: &str, secs: i64, roles: &[&str]) -> Store {
    team_in(home, name, secs, roles, &home.join("work"))
}

/// Like `team`, with every member's `cwd` set to `cwd`.
fn team_in(home: &Path, name: &str, secs: i64, roles: &[&str], cwd: &Path) -> Store {
    let team = TeamName::parse(name).unwrap();
    let mut store = Store::open(home, &team).unwrap();
    store
        .write(|tx| {
            tx.create_team(
                at(secs),
                &NewTeam {
                    name: team.clone(),
                    title: format!("Team {name}"),
                    lead_harness: HarnessId::from_static("claude-code"),
                    lead_host: HostId::from_static("herdr"),
                    lead_route: LeadRoute::Sink,
                    lead_session: None,
                },
            )?;
            for role in roles {
                tx.insert_member(
                    at(secs),
                    &NewMember {
                        role: Role::parse(role).unwrap(),
                        title: format!("the {role}"),
                        kind: MemberKind::Implementer,
                        mode: Mode::Pane,
                        harness: HarnessId::from_static("claude-code"),
                        host: HostId::from_static("herdr"),
                        cwd: cwd.to_path_buf(),
                        repo: None,
                        branch: None,
                        base: None,
                        base_sha: None,
                        check: None,
                        owns: vec![],
                        deadline_s: 1800,
                    },
                )?;
            }
            Ok(())
        })
        .unwrap();
    store
}

fn report(dir: &Path, role: &str, text: &str) {
    let reports = dir.join("reports");
    std::fs::create_dir_all(&reports).unwrap();
    std::fs::write(reports.join(format!("{role}.md")), text).unwrap();
}

/// Copies `store` into `archive/<folder>` the way a close does, with only
/// `team.db` and a checksum file that is `age_s` seconds old.
fn archive(home: &Path, store: &Store, folder: &str, age_s: u64) -> PathBuf {
    let dir = home.join("archive").join(folder);
    std::fs::create_dir_all(&dir).unwrap();
    store.backup_to(&dir.join("team.db")).unwrap();
    let sums = std::fs::File::create(dir.join("SHA256SUMS")).unwrap();
    sums.set_modified(SystemTime::now() - Duration::from_secs(age_s))
        .unwrap();
    dir
}

fn query(team: Option<&str>, role: Option<&str>, grep: Option<&str>) -> DecisionsQuery {
    DecisionsQuery {
        team: team.map(|t| TeamName::parse(t).unwrap()),
        role: role.map(str::to_owned),
        grep: grep.map(str::to_owned),
        reference: None,
        scope: None,
    }
}

#[test]
fn a_running_team_lists_its_members_in_role_order_with_a_missing_report_marked() {
    let home = TestHome::new().unwrap();
    team(home.path(), "alpha", 1, &["web", "api", "docs"]);
    let dir = home.path().join("teams/alpha");
    report(&dir, "web", "## Decisions I made\n- used a table\n");
    report(&dir, "api", "Decisions I made: none\n");
    let view = read_decisions(home.path(), &query(Some("alpha"), None, None)).unwrap();
    assert_eq!(view.errors, Vec::<String>::new());
    let t = &view.teams[0];
    assert_eq!((t.team.as_str(), t.live), ("alpha", true));
    let roles: Vec<_> = t.members.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, ["api", "docs", "web"]);
    assert_eq!(t.members[0].blocks[0].text, "none");
    assert_eq!(t.members[1].report, None);
    assert_eq!(t.members[2].blocks[0].text, "- used a table");
    assert!(
        t.members[2]
            .report
            .as_ref()
            .unwrap()
            .ends_with("reports/web.md")
    );
}

#[test]
fn role_keeps_one_member() {
    let home = TestHome::new().unwrap();
    team(home.path(), "alpha", 1, &["web", "api"]);
    let view = read_decisions(home.path(), &query(Some("alpha"), Some("api"), None)).unwrap();
    assert_eq!(view.teams[0].members.len(), 1);
    assert_eq!(view.teams[0].members[0].role, "api");
}

#[test]
fn a_closed_team_reads_its_newest_archive_only() {
    let home = TestHome::new().unwrap();
    let other = TestHome::new().unwrap();
    let store = team(other.path(), "beta", 2, &["web"]);
    let old = archive(home.path(), &store, "beta-2026-09-01", 3600);
    let new = archive(home.path(), &store, "beta-2026-09-02", 60);
    report(&old, "web", "## Decisions I made\n- from the old archive\n");
    report(&new, "web", "## Decisions I made\n- from the new archive\n");
    let view = read_decisions(home.path(), &query(Some("beta"), None, None)).unwrap();
    assert_eq!(view.teams.len(), 1);
    assert!(!view.teams[0].live);
    assert_eq!(view.teams[0].path, new.display().to_string());
    let texts: Vec<_> = view.teams[0].members[0]
        .blocks
        .iter()
        .map(|b| b.text.as_str())
        .collect();
    assert_eq!(texts, ["- from the new archive"]);
}

#[test]
fn a_name_with_no_team_and_no_archive_is_refused() {
    let home = TestHome::new().unwrap();
    let err = read_decisions(home.path(), &query(Some("nope"), None, None)).unwrap_err();
    assert_eq!(err, DecisionsError::NoSuchTeam("nope".to_owned()));
    assert_eq!(
        read_decisions(home.path(), &query(None, None, None)).unwrap_err(),
        DecisionsError::NoFilter
    );
}

#[test]
fn grep_reaches_running_teams_and_archives_ignoring_case_and_skips_misses() {
    let home = TestHome::new().unwrap();
    let other = TestHome::new().unwrap();
    team(home.path(), "alpha", 1, &["web", "api", "docs"]);
    let dir = home.path().join("teams/alpha");
    report(
        &dir,
        "web",
        "## Decisions I made\n- kept the STASH out\n- second\n",
    );
    report(&dir, "api", "## Decisions I made\n- unrelated\n");
    let gamma = team(other.path(), "gamma", 3, &["db"]);
    let g = archive(home.path(), &gamma, "gamma-2026-09-30", 60);
    report(&g, "db", "Decisions I made:\n- no stash here\n");
    let view = read_decisions(home.path(), &query(None, None, Some("stash"))).unwrap();
    let found: Vec<_> = view
        .teams
        .iter()
        .map(|t| (t.team.as_str(), t.members.len()))
        .collect();
    assert_eq!(found, [("alpha", 1), ("gamma", 1)]);
    // The whole matching block prints, not only the matching line.
    assert_eq!(
        view.teams[0].members[0].blocks[0].text,
        "- kept the STASH out\n- second"
    );
}

#[test]
fn a_team_running_and_archived_is_read_once_from_the_running_copy() {
    let home = TestHome::new().unwrap();
    let store = team(home.path(), "alpha", 1, &["web"]);
    report(
        &home.path().join("teams/alpha"),
        "web",
        "## Decisions I made\n- live copy\n",
    );
    let a = archive(home.path(), &store, "alpha-2026-09-30", 60);
    report(&a, "web", "## Decisions I made\n- archived copy\n");
    let view = read_decisions(home.path(), &query(None, None, Some("copy"))).unwrap();
    assert_eq!(view.teams.len(), 1);
    assert!(view.teams[0].live);
    assert_eq!(view.teams[0].members[0].blocks[0].text, "- live copy");
}

#[test]
fn teams_print_oldest_first() {
    let home = TestHome::new().unwrap();
    for (name, secs) in [("zeta", 1), ("alpha", 5)] {
        team(home.path(), name, secs, &["web"]);
        report(
            &home.path().join("teams").join(name),
            "web",
            "## Decisions I made\n- x\n",
        );
    }
    let view = read_decisions(home.path(), &query(None, None, Some("x"))).unwrap();
    let names: Vec<_> = view.teams.iter().map(|t| t.team.as_str()).collect();
    assert_eq!(names, ["zeta", "alpha"]);
}

#[test]
fn an_unreadable_database_goes_to_errors_and_the_rest_still_reads() {
    let home = TestHome::new().unwrap();
    team(home.path(), "alpha", 1, &["web"]);
    report(
        &home.path().join("teams/alpha"),
        "web",
        "## Decisions I made\n- x\n",
    );
    let broken = home.path().join("teams/broken");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(broken.join("team.db"), b"not a database").unwrap();
    let view = read_decisions(home.path(), &query(None, None, Some("x"))).unwrap();
    assert_eq!(view.teams.len(), 1);
    assert_eq!(view.errors.len(), 1, "{:?}", view.errors);
    assert!(view.errors[0].contains("broken"));
}

#[test]
fn limit_blocks_cuts_in_print_order_and_says_so() {
    let home = TestHome::new().unwrap();
    team(home.path(), "alpha", 1, &["api", "web"]);
    let dir = home.path().join("teams/alpha");
    report(
        &dir,
        "api",
        "## Decisions I made\n- a\n## Decisions I made\n- b\n",
    );
    report(&dir, "web", "## Decisions I made\n- c\n");
    let mut view = read_decisions(home.path(), &query(Some("alpha"), None, None)).unwrap();
    assert!(view.limit_blocks(2));
    assert_eq!(view.teams[0].members.len(), 1);
    assert_eq!(view.teams[0].members[0].blocks.len(), 2);
    let mut whole = read_decisions(home.path(), &query(Some("alpha"), None, None)).unwrap();
    assert!(!whole.limit_blocks(3));
    assert_eq!(whole.teams[0].members.len(), 2);
}

fn with_refs(home: &Path, name: &str, secs: i64, refs: &[(&str, &str)]) {
    let mut store = team(home, name, secs, &["web"]);
    let refs: Vec<TicketRef> = refs
        .iter()
        .map(|(p, id)| TicketRef {
            provider: (*p).to_owned(),
            id: (*id).to_owned(),
        })
        .collect();
    store.write(|tx| tx.set_refs(&refs)).unwrap();
    report(
        &home.join("teams").join(name),
        "web",
        "## Decisions I made\n- x\n",
    );
}

fn teams_with_ref(home: &Path, reference: &str) -> Vec<String> {
    let q = DecisionsQuery {
        reference: Some(reference.to_owned()),
        ..DecisionsQuery::default()
    };
    read_decisions(home, &q)
        .unwrap()
        .teams
        .into_iter()
        .map(|t| t.team)
        .collect()
}

#[test]
fn ref_matches_the_provider_exactly_and_a_bare_id_under_any_provider() {
    let home = TestHome::new().unwrap();
    with_refs(home.path(), "alpha", 1, &[("linear", "GKA-580")]);
    with_refs(
        home.path(),
        "beta",
        2,
        &[("jira", "GKA-580"), ("github", "#7")],
    );
    with_refs(home.path(), "gamma", 3, &[]);
    let h = home.path();
    assert_eq!(teams_with_ref(h, "linear:GKA-580"), ["alpha"]);
    assert_eq!(teams_with_ref(h, "LINEAR:gka-580"), Vec::<String>::new());
    assert_eq!(teams_with_ref(h, "linear:gka-580"), ["alpha"]);
    assert_eq!(teams_with_ref(h, "GKA-580"), ["alpha", "beta"]);
    assert_eq!(teams_with_ref(h, "gka-580"), ["alpha", "beta"]);
    assert_eq!(teams_with_ref(h, "jira:#7"), Vec::<String>::new());
    assert_eq!(teams_with_ref(h, "github:#7"), ["beta"]);
}

#[test]
fn a_colon_that_is_not_after_a_provider_name_belongs_to_the_id() {
    let home = TestHome::new().unwrap();
    with_refs(home.path(), "alpha", 1, &[("custom", "A:1")]);
    assert_eq!(teams_with_ref(home.path(), "custom:A:1"), ["alpha"]);
    assert_eq!(teams_with_ref(home.path(), "A:1"), ["alpha"]);
    assert_eq!(teams_with_ref(home.path(), "9x:A:1"), Vec::<String>::new());
}

#[test]
fn the_refs_field_lists_the_recorded_refs_and_a_team_without_refs_never_matches() {
    let home = TestHome::new().unwrap();
    with_refs(home.path(), "alpha", 1, &[("linear", "GKA-580")]);
    with_refs(home.path(), "old", 2, &[]);
    let view = read_decisions(home.path(), &query(Some("alpha"), None, None)).unwrap();
    assert_eq!(view.teams[0].refs[0].to_string(), "linear:GKA-580");
    assert_eq!(teams_with_ref(home.path(), "GKA-580"), ["alpha"]);
}

#[test]
fn limit_blocks_keeps_a_member_before_the_cut_even_with_no_report() {
    let home = TestHome::new().unwrap();
    team(home.path(), "alpha", 1, &["api", "web"]);
    report(
        &home.path().join("teams/alpha"),
        "web",
        "## Decisions I made\n- a\n## Decisions I made\n- b\n",
    );
    let mut view = read_decisions(home.path(), &query(Some("alpha"), None, None)).unwrap();
    assert!(view.limit_blocks(1));
    let members = &view.teams[0].members;
    assert_eq!(members.len(), 2);
    assert_eq!(
        (members[0].role.as_str(), members[0].report.clone()),
        ("api", None)
    );
    assert_eq!(members[1].blocks.len(), 1);
}

/// A team database as a version-4 binary left it: the v1 to v4 tables, no
/// `refs` column, one team row. A store connection empties a fresh database
/// first, because this test has no `SQLite` library of its own.
fn version_4_team(home: &Path, name: &str) -> PathBuf {
    let mut store = Store::open(home, &TeamName::parse(name).unwrap()).unwrap();
    store
        .write(|tx| {
            let conn = tx.connection();
            let tables: Vec<String> = conn
                .prepare(
                    "SELECT name FROM sqlite_schema WHERE type = 'table'
                       AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\'",
                )?
                .query_map([], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            for table in tables {
                conn.execute_batch(&format!("DROP TABLE {table};"))?;
            }
            for step in [
                teamwire_store::SCHEMA_V1_SQL,
                teamwire_store::SCHEMA_V2_SQL,
                teamwire_store::SCHEMA_V3_SQL,
                teamwire_store::SCHEMA_V4_SQL,
            ] {
                conn.execute_batch(step)?;
            }
            conn.execute_batch(&format!(
                "INSERT INTO team (id, ulid, name, title, state, lead_harness, lead_host, lead_route, created_at)
                 VALUES (1, '01J9ZQ0000000000000000TEAM', '{name}', 'Old', 'closed', 'claude-code', 'herdr', 'sink',
                         '2026-10-01T00:00:00.000Z');
                 PRAGMA user_version = 4;"
            ))?;
            Ok(())
        })
        .unwrap();
    drop(store);
    database_path(home, &TeamName::parse(name).unwrap())
}

#[test]
fn an_archive_from_before_schema_v5_reads_with_no_refs_and_never_matches_a_ref() {
    let home = TestHome::new().unwrap();
    let other = TestHome::new().unwrap();
    let db = version_4_team(other.path(), "old");
    let dir = home.path().join("archive/old-2026-09-30");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(&db, dir.join("team.db")).unwrap();
    std::fs::write(dir.join("SHA256SUMS"), b"").unwrap();
    let view = read_decisions(home.path(), &query(Some("old"), None, None)).unwrap();
    assert_eq!(view.errors, Vec::<String>::new());
    assert_eq!(view.teams[0].refs, Vec::<TicketRef>::new());
    assert_eq!(teams_with_ref(home.path(), "GKA-580"), Vec::<String>::new());
    assert_eq!(
        teams_with_ref(home.path(), "linear:GKA-580"),
        Vec::<String>::new()
    );
}

/// The scope of team `name`: its own members' directories.
fn scope_of(home: &Path, name: &str, user_home: Option<&str>) -> RepoScope {
    let store = Store::open_read_only(home, &TeamName::parse(name).unwrap()).unwrap();
    let (team, members) = store
        .read(|tx| Ok((tx.team()?.unwrap(), tx.members()?.items)))
        .unwrap();
    RepoScope::new(team.ulid, &members, user_home.map(PathBuf::from))
}

fn scoped_grep(home: &Path, scope: RepoScope) -> Vec<String> {
    let q = DecisionsQuery {
        grep: Some("x".to_owned()),
        scope: Some(scope),
        ..DecisionsQuery::default()
    };
    read_decisions(home, &q)
        .unwrap()
        .teams
        .into_iter()
        .map(|t| t.team)
        .collect()
}

fn repo_team(home: &Path, name: &str, secs: i64, cwd: &str) {
    team_in(home, name, secs, &["web"], Path::new(cwd));
    report(
        &home.join("teams").join(name),
        "web",
        "## Decisions I made\n- x\n",
    );
}

#[test]
fn a_scope_keeps_only_teams_that_share_a_repository() {
    let home = TestHome::new().unwrap();
    repo_team(home.path(), "alpha", 1, "/nonexistent/repo_a");
    repo_team(home.path(), "beta", 2, "/nonexistent/repo_a/crates/x");
    repo_team(home.path(), "gamma", 3, "/nonexistent/repo_b");
    let scope = scope_of(home.path(), "alpha", None);
    assert_eq!(scoped_grep(home.path(), scope.clone()), ["alpha", "beta"]);
    let q = DecisionsQuery {
        team: Some(TeamName::parse("gamma").unwrap()),
        scope: Some(scope),
        ..DecisionsQuery::default()
    };
    assert_eq!(
        read_decisions(home.path(), &q).unwrap_err(),
        DecisionsError::OtherRepo("gamma".to_owned())
    );
}

#[test]
fn a_worktree_path_matches_its_main_repository() {
    let home = TestHome::new().unwrap();
    repo_team(home.path(), "alpha", 1, "/nonexistent/proj/sub");
    repo_team(home.path(), "beta", 2, "/nonexistent/proj/.worktrees/beta");
    let scope = scope_of(home.path(), "alpha", None);
    assert_eq!(scoped_grep(home.path(), scope), ["alpha", "beta"]);
}

#[test]
fn a_team_rooted_at_the_home_directory_or_root_matches_nothing() {
    let home = TestHome::new().unwrap();
    repo_team(home.path(), "alpha", 1, "/nonexistent/me/proj");
    repo_team(home.path(), "rooted", 2, "/nonexistent/me");
    repo_team(home.path(), "atroot", 3, "/");
    // Neither side may be the home directory: the rooted team shares nothing
    // with alpha, and a caller rooted there shares nothing with anyone.
    let scope = scope_of(home.path(), "alpha", Some("/nonexistent/me"));
    assert_eq!(scoped_grep(home.path(), scope), ["alpha"]);
    let scope = scope_of(home.path(), "rooted", Some("/nonexistent/me"));
    assert_eq!(scoped_grep(home.path(), scope), ["rooted"]);
    let scope = scope_of(home.path(), "atroot", Some("/nonexistent/me"));
    assert_eq!(scoped_grep(home.path(), scope), ["atroot"]);
}
