//! Two questions callers used to answer by hand: which open teams a lead
//! session leads (`open_teams`), and whether a token is a member's
//! (`ReadTx::token_matches`).
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]
#![expect(
    clippy::indexing_slicing,
    reason = "a test that indexes past the end panics and fails, which is the intended outcome"
)]

use teamwire_core::{
    HarnessId, HostId, LeadRoute, MemberKind, Mode, Role, SessionId, SessionKind, TeamName,
    Timestamp,
};
use teamwire_fakes::TestHome;
use teamwire_store::{CloseEnd, NewMember, NewTeam, Store, open_teams, token_hash};

fn at(n: i64) -> Timestamp {
    Timestamp::from_unix_millis(1_790_000_000_000 + n).unwrap()
}

fn session(id: &str) -> SessionId {
    SessionId::parse(id).unwrap()
}

fn role(name: &str) -> Role {
    Role::parse(name).unwrap()
}

/// A team named `name` led by `lead`, with no row at all when `row` is false.
fn team(home: &TestHome, name: &str, lead: Option<&str>, row: bool) -> Store {
    let name = TeamName::parse(name).unwrap();
    let mut store = Store::open(home.path(), &name).unwrap();
    if row {
        store
            .write(|tx| {
                tx.create_team(
                    at(0),
                    &NewTeam {
                        name,
                        title: "t".to_owned(),
                        lead_harness: HarnessId::from_static("claude-code"),
                        lead_host: HostId::from_static("herdr"),
                        lead_route: LeadRoute::Sink,
                        lead_session: lead.map(|s| (session(s), SessionKind::Interactive)),
                    },
                )
                .map(|_| ())
            })
            .unwrap();
    }
    store
}

fn names(found: &[(TeamName, teamwire_store::TeamRead)]) -> Vec<String> {
    found.iter().map(|(n, _)| n.as_str().to_owned()).collect()
}

#[test]
fn open_teams_lists_the_open_teams_a_session_leads_and_reports_a_rowless_one() {
    let home = TestHome::new().unwrap();
    drop(team(&home, "mine", Some("s1"), true));
    drop(team(&home, "theirs", Some("s2"), true));
    drop(team(&home, "nolead", None, true));
    drop(team(&home, "empty", None, false));
    let mut closed = team(&home, "done", Some("s1"), true);
    closed
        .write(|tx| {
            tx.begin_close(at(1), teamwire_core::Pid::new(1))?;
            tx.finish_close(at(2), &CloseEnd::Completed).map(|_| ())
        })
        .unwrap();

    let led = open_teams(home.path(), Some(&session("s1"))).unwrap();
    assert_eq!(names(&led), ["empty", "mine"]);
    assert!(matches!(led[0].1, Ok(None)), "a store with no team row");
    let mine = led[1].1.as_ref().unwrap().as_ref().unwrap();
    assert_eq!(mine.row.name.as_str(), "mine");

    let every = open_teams(home.path(), None).unwrap();
    assert_eq!(names(&every), ["empty", "mine", "nolead", "theirs"]);
}

#[test]
fn open_teams_reports_a_store_that_cannot_be_read_and_lists_none_without_teams() {
    let home = TestHome::new().unwrap();
    assert!(open_teams(home.path(), None).unwrap().is_empty());
    drop(team(&home, "mine", Some("s1"), true));
    let broken = home.path().join("teams/broken");
    std::fs::create_dir_all(&broken).unwrap();
    #[expect(
        clippy::disallowed_methods,
        reason = "the test writes text where a team's database belongs, on purpose"
    )]
    std::fs::write(broken.join("team.db"), b"not a database at all, just text").unwrap();
    let found = open_teams(home.path(), Some(&session("s1"))).unwrap();
    assert_eq!(names(&found), ["broken", "mine"]);
    assert!(found[0].1.is_err(), "{:?}", found[0].1);
}

fn member(name: &str) -> NewMember {
    NewMember {
        role: role(name),
        title: "t".to_owned(),
        kind: MemberKind::Implementer,
        mode: Mode::Pane,
        harness: HarnessId::from_static("claude-code"),
        host: HostId::from_static("herdr"),
        cwd: "/w".into(),
        repo: None,
        branch: None,
        base: None,
        base_sha: None,
        check: None,
        owns: Vec::new(),
        deadline_s: 60,
    }
}

#[test]
fn token_matches_only_a_live_member_s_own_token() {
    let home = TestHome::new().unwrap();
    let mut store = team(&home, "alpha", None, true);
    store
        .write(|tx| {
            for name in ["api", "web", "gone"] {
                tx.insert_member(at(1), &member(name))?;
            }
            tx.set_member_token_hash(&role("api"), &token_hash("secret"))?;
            tx.set_member_token_hash(&role("gone"), &token_hash("old"))?;
            tx.remove_member(at(2), &role("gone"), None, &serde_json::Value::Null)
                .map(|_| ())
        })
        .unwrap();
    let check = |who: &str, token: &str| {
        store
            .read(|tx| tx.token_matches(&role(who), token))
            .unwrap()
    };
    assert_eq!(check("api", "secret"), Some(true));
    assert_eq!(check("api", "secrets"), Some(false));
    assert_eq!(check("api", ""), Some(false));
    assert_eq!(check("web", "secret"), None, "no stored hash");
    assert_eq!(check("gone", "old"), None, "a removed member");
    assert_eq!(check("nobody", "secret"), None, "no such member");
}
