//! When the lead's tool hook adds a note (ADR 0035): member work done by the
//! session that leads an open team, and nothing else.
#![expect(
    clippy::unwrap_used,
    reason = "a failing unwrap in an integration test fails the test"
)]

use std::path::PathBuf;

use teamwire_core::{
    Clock, HarnessId, HostId, LeadRoute, MemberKind, Mode, Role, SessionId, SessionKind, TeamName,
    ToolAction, ToolCall,
};
use teamwire_fakes::{FakeClock, TestHome};
use teamwire_report::lead_tool_note;
use teamwire_store::{NewMember, NewTeam, Store};

struct Rig {
    home: TestHome,
    clock: FakeClock,
}

impl Rig {
    fn new() -> Self {
        Self {
            home: TestHome::new().unwrap(),
            clock: FakeClock::new(),
        }
    }

    /// An open team led from `lead-1`, whose worktree member `api` works in
    /// `<repo>/.worktrees/api`.
    fn team(&self, name: &str, repo: &str) -> Store {
        let team = TeamName::parse(name).unwrap();
        let mut store = Store::open(self.home.path(), &team).unwrap();
        let now = self.clock.now();
        store
            .write(|tx| {
                tx.create_team(
                    now,
                    &NewTeam {
                        name: team.clone(),
                        title: name.to_owned(),
                        lead_harness: HarnessId::from_static("claude-code"),
                        lead_host: HostId::from_static("herdr"),
                        lead_route: LeadRoute::Sink,
                        lead_session: Some((
                            SessionId::parse("lead-1").unwrap(),
                            SessionKind::Interactive,
                        )),
                    },
                )?;
                tx.insert_member(
                    now,
                    &NewMember {
                        role: Role::parse("api").unwrap(),
                        title: "api".to_owned(),
                        kind: MemberKind::Implementer,
                        mode: Mode::Worktree,
                        harness: HarnessId::from_static("claude-code"),
                        host: HostId::from_static("herdr"),
                        cwd: PathBuf::from(repo).join(".worktrees/api"),
                        repo: Some(PathBuf::from(repo)),
                        branch: Some(format!("{name}/api")),
                        base: Some("main".to_owned()),
                        base_sha: None,
                        check: Some("cargo test".to_owned()),
                        owns: vec!["src/".to_owned()],
                        deadline_s: 1800,
                    },
                )
            })
            .unwrap();
        store
    }

    fn note(&self, session: &str, action: ToolAction) -> Option<String> {
        self.note_from(session, false, action)
    }

    fn note_from(&self, session: &str, subagent: bool, action: ToolAction) -> Option<String> {
        let call = ToolCall {
            session: SessionId::parse(session).unwrap(),
            subagent,
            cwd: Some(PathBuf::from("/r")),
            action,
        };
        lead_tool_note(self.home.path(), &call)
    }
}

fn edit(path: &str) -> ToolAction {
    ToolAction::Edit(vec![PathBuf::from(path)])
}

fn shell(line: &str) -> ToolAction {
    ToolAction::Shell(line.to_owned())
}

#[test]
fn an_edit_in_the_teams_repository_gets_a_note_that_names_the_team() {
    let rig = Rig::new();
    rig.team("alpha", "/r");
    let note = rig.note("lead-1", edit("/r/src/lib.rs")).unwrap();
    assert!(
        note.starts_with("[teamwire:alpha] You lead team alpha, so leave this edit to /r/src/lib.rs to its members"),
        "{note}"
    );
    assert!(note.ends_with("If the user asked you to do it yourself, carry on."));
    // A member's worktree is inside the repository, and a relative path is
    // read against the session's folder.
    assert!(
        rig.note("lead-1", edit("/r/.worktrees/api/src/a.rs"))
            .is_some()
    );
    assert!(rig.note("lead-1", edit("src/main.rs")).is_some());
}

#[test]
fn an_edit_outside_the_repository_or_in_teamwires_state_gets_none() {
    let rig = Rig::new();
    rig.team("alpha", "/r");
    assert_eq!(rig.note("lead-1", edit("/elsewhere/plan.md")), None);
    let brief = rig.home.path().join("teams/alpha/briefs/api.md");
    assert_eq!(rig.note("lead-1", ToolAction::Edit(vec![brief])), None);
}

#[test]
fn a_build_or_a_git_write_gets_a_note_for_every_team_the_session_leads() {
    let rig = Rig::new();
    rig.team("alpha", "/r");
    rig.team("beta", "/s");
    let note = rig.note("lead-1", shell("cd /x && cargo test")).unwrap();
    assert!(
        note.starts_with("[teamwire:alpha] You lead teams alpha, beta, so leave this build or test run to their members"),
        "{note}"
    );
    let note = rig.note("lead-1", shell("git merge alpha/api")).unwrap();
    assert!(note.contains("leave this git merge to"), "{note}");
}

#[test]
fn reads_another_session_a_subagent_or_a_closed_team_get_none() {
    let rig = Rig::new();
    let mut store = rig.team("alpha", "/r");
    assert_eq!(rig.note("lead-1", ToolAction::Other), None);
    assert_eq!(rig.note("lead-1", shell("git log --oneline")), None);
    assert_eq!(rig.note("someone-else", shell("cargo test")), None);
    assert_eq!(rig.note_from("lead-1", true, shell("cargo test")), None);
    store
        .write(|tx| {
            tx.connection()
                .execute("UPDATE team SET state = 'closed'", [])
                .map(|_| ())
                .map_err(Into::into)
        })
        .unwrap();
    assert_eq!(rig.note("lead-1", shell("cargo test")), None);
}
