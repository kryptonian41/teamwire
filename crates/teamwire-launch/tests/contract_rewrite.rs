//! `teamwire amend` rewrites a member's seat rules file with its new check
//! and owns, so a handoff or reopen session reads the new scope (ADR 0046).
#![expect(
    clippy::unwrap_used,
    reason = "an integration-test file unwraps; a failure there fails the test"
)]

use std::path::{Path, PathBuf};

use teamwire_core::{
    HarnessId, HostId, Member, MemberKind, MemberState, Mode, Platform, Role, TeamName, Timestamp,
};
use teamwire_fakes::{FakePlatform, TestHome};
use teamwire_launch::{ContractParts, render_contract, rewrite_contract};

fn member(check: &str, owns: &[&str]) -> Member {
    Member {
        role: Role::parse("api").unwrap(),
        title: "the api".to_owned(),
        kind: MemberKind::Implementer,
        mode: Mode::Worktree,
        harness: HarnessId::from_static("claude-code"),
        host: HostId::from_static("herdr"),
        place: None,
        session_id: None,
        cwd: PathBuf::from("/work"),
        repo: None,
        branch: None,
        base: None,
        base_sha: None,
        check: Some(check.to_owned()),
        owns: owns.iter().map(|o| (*o).to_owned()).collect(),
        deadline_s: 1800,
        state: MemberState::Started,
        generation: 1,
        gen_started_at: Timestamp::from_unix_millis(0).unwrap(),
        gate_told_generation: None,
        removed_at: None,
        removed_note: None,
        quiet: false,
        set: None,
    }
}

fn rendered(home: &Path, team: &TeamName, m: &Member, bin: &Path) -> String {
    let team_dir = home.join("teams").join(team.as_str());
    render_contract(&ContractParts {
        team,
        role: &m.role,
        kind: m.kind,
        report_file: &team_dir.join("reports").join("api.md"),
        outbox_dir: &team_dir.join("outbox"),
        teamwire_bin: bin,
        check: m.check.as_deref(),
        owns: &m.owns,
        quiet: m.quiet,
    })
}

#[test]
fn rewrite_contract_writes_the_new_scope() {
    let home = TestHome::new().unwrap();
    let team = TeamName::parse("demo").unwrap();
    let bin = Path::new("/opt/x/teamwire");
    let path = home.path().join("teams/demo/contract/api.md");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let old = member("cargo test -p a", &["a/"]);
    let platform = FakePlatform::new();
    platform
        .write_atomic(&path, rendered(home.path(), &team, &old, bin).as_bytes())
        .unwrap();

    let new = member("cargo test -p b", &["b/"]);
    let written = rewrite_contract(&platform, home.path(), &team, &new, bin).unwrap();

    assert_eq!(written, path);
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text, rendered(home.path(), &team, &new, bin));
    assert!(text.contains("cargo test -p b"), "{text}");
    assert!(!text.contains("cargo test -p a"), "{text}");
}

/// An amend rewrites the seat from the stored row, so a member whose spec
/// set `quiet` keeps its quiet parts after the rewrite.
#[test]
fn rewrite_contract_keeps_a_quiet_members_quiet_parts() {
    let home = TestHome::new().unwrap();
    let team = TeamName::parse("demo").unwrap();
    let bin = Path::new("/opt/x/teamwire");
    let quiet = Member {
        kind: MemberKind::Investigator,
        mode: Mode::Pane,
        quiet: true,
        ..member("cargo test -p b", &[])
    };
    let platform = FakePlatform::new();
    let path = rewrite_contract(&platform, home.path(), &team, &quiet, bin).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("You send no messages, and members do not message you"),
        "{text}"
    );
    assert!(!text.contains("/opt/x/teamwire send"), "{text}");
}
