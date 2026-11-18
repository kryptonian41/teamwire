//! The shared harness contract suite against `CodexCli`. The member's
//! identity holds only the token: `TEAMWIRE_TEAM` and the rest travel in
//! `-c` values, and the suite wants each identity value in a launch file.

#![expect(
    clippy::disallowed_methods,
    reason = "the test writes Codex's files as Codex would, not through the platform"
)]

use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{
    Address, DialogKind, MemberKind, MemberLaunch, OtelConfig, Pid, Role, RunOutput, SessionId,
    TeamName,
};
use teamwire_fakes::FakePlatform;
use teamwire_fakes::contract::{
    HarnessSetup, PermissionSetup, harness_contract, permission_contract,
};
use teamwire_harness_codex::{CodexCli, CodexConfig};

#[test]
fn codex_passes_the_harness_contract() {
    let dir = tempfile::tempdir().unwrap();
    let live = "019e0000-0000-7000-8000-0000000000aa";
    std::fs::create_dir_all(dir.path().join("thread-writer-locks")).unwrap();
    std::fs::write(
        dir.path().join(format!("thread-writer-locks/{live}.lock")),
        "",
    )
    .unwrap();
    let platform = Arc::new(FakePlatform::new());
    platform.set_alive(Pid::new(77), true);
    // is_alive and processes_for_session each ask lsof once for the live session.
    for _ in 0..2 {
        platform.queue_run(Ok(RunOutput {
            exit_code: 0,
            stdout: b"77\n".to_vec(),
            stderr: Vec::new(),
            timed_out: false,
        }));
    }
    let codex = CodexCli::new(
        platform,
        CodexConfig {
            codex_home: dir.path().to_path_buf(),
            codex_bin: "codex".into(),
        },
    );
    let team = TeamName::parse("demo").unwrap();
    let role = Role::parse("api").unwrap();
    let contract = dir.path().join("contract.md");
    std::fs::write(&contract, "be good").unwrap();
    let member = MemberLaunch {
        address: Address::of(&team, &role),
        team,
        role,
        kind: MemberKind::Implementer,
        cwd: dir.path().to_path_buf(),
        session_id: SessionId::parse("019e0000-0000-7000-8000-000000000001").unwrap(),
        model: None,
        effort: None,
        member_dir: dir.path().join("home/teams/demo/members/api"),
        contract_path: contract,
        identity: vec![("TEAMWIRE_TOKEN".into(), "tok-contract-secret".into())],
        otel: OtelConfig::default(),
        teamwire_bin: "/usr/local/bin/teamwire".into(),
        plugins: std::collections::BTreeMap::new(),
        context_handoff_tokens: None,
        accept_trust: false,
    };
    harness_contract(&HarnessSetup {
        harness: &codex,
        member,
        turn_end_stdin: br#"{"session_id":"member-1","stop_hook_active":false}"#.to_vec(),
        turn_end_session: SessionId::parse("member-1").unwrap(),
        prompt_screen: "› Ask Codex to do anything\n\n  ? for shortcuts   100% context left".into(),
        dialog_screen: "Trust this folder?\n› 1. Trust and continue\n  2. Quit".into(),
        live_session: Some(SessionId::parse(live).unwrap()),
        unknown_session: SessionId::parse("never-heard-of").unwrap(),
        timeout: Duration::from_secs(5),
    });
}

// Copied from the snapshot files of Codex's TUI, as in `tests/screen.rs`.
const APPROVAL: &str = "
  Would you like to run the following command?

  $ echo hello world


› 1. Yes, proceed (y)
  2. Yes, and don't ask again for commands that start with `echo hello world` (p)
  3. No, and tell Codex what to do differently (esc)

  Press enter to confirm or esc to cancel";

const UPDATE: &str = "
  Update available · 0.0.0 → 9.9.9
  Release notes: https://github.com/openai/codex/releases/latest

› 1. Update now (runs `npm install -g @openai/codex`)
  2. Skip
  3. Skip until next version

  enter continue · esc skip";

/// Codex names its dialogs from the screen and has no permission hook: its
/// members run with approvals off, so `dialog answer` refuses them.
#[test]
fn codex_passes_the_permission_contract_with_no_hook() {
    let codex = CodexCli::new(
        Arc::new(FakePlatform::new()),
        CodexConfig {
            codex_home: "/nowhere/codex".into(),
            codex_bin: "codex".into(),
        },
    );
    permission_contract(&PermissionSetup {
        harness: Box::new(codex),
        prompt_screen:
            "\n› Ask Codex to do anything\n\n  ? for shortcuts            100% context left".into(),
        named_screens: vec![
            (APPROVAL.to_owned(), DialogKind::Permission),
            (UPDATE.to_owned(), DialogKind::Update),
        ],
        answerable_stdin: None,
        other_tool_stdin: None,
    });
}
