//! `launch_spec` and `resume_spec`: the argv, and the token file.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests build fixtures from literals and read TOML by key; a missing key should fail the test"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "the test writes the files a repository or launch would, not through the platform"
)]

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::sync::Arc;

use teamwire_core::{
    Address, Config, Harness, HarnessErrorCode, IdentityChannel, LaunchSpec, MemberKind,
    MemberLaunch, OtelConfig, Role, SessionId, TeamName,
};
use teamwire_fakes::FakePlatform;
use teamwire_harness_codex::{CodexCli, CodexConfig};

const TOKEN: &str = "tok-0123456789abcdef";
const SESSION: &str = "019e0000-0000-7000-8000-000000000001";

fn codex() -> CodexCli {
    CodexCli::new(
        Arc::new(FakePlatform::new()),
        CodexConfig {
            codex_home: "/nowhere/codex".into(),
            codex_bin: "codex".into(),
        },
    )
}

/// A member whose home is `<root>/home`, with a contract file on disk.
fn member(root: &Path) -> MemberLaunch {
    let team = TeamName::parse("demo").unwrap();
    let role = Role::parse("api").unwrap();
    let contract = root.join("home/teams/demo/contract/api.md");
    std::fs::create_dir_all(contract.parent().unwrap()).unwrap();
    std::fs::write(&contract, "Report \"done\"\nwith \\ care.\n").unwrap();
    MemberLaunch {
        address: Address::of(&team, &role),
        team,
        role,
        kind: MemberKind::Implementer,
        cwd: "/work".into(),
        session_id: SessionId::parse(SESSION).unwrap(),
        model: None,
        effort: None,
        member_dir: root.join("home/teams/demo/members/api"),
        contract_path: contract,
        identity: vec![("TEAMWIRE_TOKEN".into(), TOKEN.into())],
        otel: OtelConfig::default(),
        teamwire_bin: "/usr/local/bin/teamwire".into(),
        plugins: BTreeMap::new(),
        context_handoff_tokens: None,
        accept_trust: false,
    }
}

/// Every `-c` value of `argv` parsed back, as `key -> value`.
fn overrides(spec: &LaunchSpec) -> BTreeMap<String, toml::Value> {
    let mut out = BTreeMap::new();
    let mut words = spec.argv.iter();
    while let Some(w) = words.next() {
        if w == "-c" {
            let pair = words.next().unwrap();
            let (key, value) = pair.split_once('=').unwrap();
            let doc: toml::Table = toml::from_str(&format!("v = {value}")).unwrap();
            out.insert(key.to_owned(), doc["v"].clone());
        }
    }
    out
}

fn words(spec: &LaunchSpec) -> Vec<&str> {
    spec.argv.iter().map(String::as_str).collect()
}

#[test]
fn a_pane_member_gets_the_fixed_flags_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let spec = codex().launch_spec(&member(dir.path())).unwrap();
    assert_eq!(
        &words(&spec)[..6],
        [
            "codex",
            "--no-daemon",
            "--dangerously-bypass-approvals-and-sandbox",
            "--dangerously-bypass-hook-trust",
            "-C",
            "/work",
        ]
    );
    let keys: Vec<String> = overrides(&spec).into_keys().collect();
    assert_eq!(
        keys,
        [
            "check_for_update_on_startup",
            "developer_instructions",
            "hooks.PostToolUse",
            "hooks.SessionStart",
            "hooks.Stop",
            "mcp_servers.teamwire",
            "plugins",
            "shell_environment_policy.set",
            "tui.disable_paste_burst",
        ]
    );
    assert_eq!(spec.identity_channel, IdentityChannel::HarnessConfig);
    assert!(spec.env.is_empty());
}

#[test]
fn the_contract_text_survives_toml_escaping() {
    let dir = tempfile::tempdir().unwrap();
    let spec = codex().launch_spec(&member(dir.path())).unwrap();
    let o = overrides(&spec);
    assert_eq!(
        o["developer_instructions"].as_str().unwrap(),
        "Report \"done\"\nwith \\ care.\n"
    );
    assert_eq!(o["check_for_update_on_startup"].as_bool(), Some(false));
    assert_eq!(o["tui.disable_paste_burst"].as_bool(), Some(true));
}

/// A contract with every character that needs an escape in a TOML string.
const AWKWARD: &str = "line one\n\tTabbed \"quoted\" back\\slash\r\nbell\u{7} end \u{e9}\n";

#[test]
fn no_argument_holds_a_newline_or_other_control_character() {
    let dir = tempfile::tempdir().unwrap();
    let m = member(dir.path());
    std::fs::write(&m.contract_path, AWKWARD).unwrap();
    let spec = codex().launch_spec(&m).unwrap();
    // herdr refuses an argument it cannot encode for the shell.
    for arg in &spec.argv {
        assert!(!arg.chars().any(char::is_control), "{arg:?}");
    }
}

#[test]
fn every_override_parses_back_to_its_value() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = member(dir.path());
    std::fs::write(&m.contract_path, AWKWARD).unwrap();
    m.accept_trust = true;
    m.plugins.insert("a.b@c d".into(), true);
    let o = overrides(&codex().launch_spec(&m).unwrap());
    assert_eq!(o["developer_instructions"].as_str(), Some(AWKWARD));
    assert_eq!(
        o["projects"]["/work"]["trust_level"].as_str(),
        Some("trusted")
    );
    // A key that is not a bare key is quoted, then read back whole.
    assert_eq!(o["plugins"]["a.b@c d"]["enabled"].as_bool(), Some(true));
    assert_eq!(
        o["plugins"]["teamwire@teamwire"]["enabled"].as_bool(),
        Some(false)
    );
}

#[test]
fn model_and_effort_appear_only_when_set() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = member(dir.path());
    m.model = Some("gpt-5.5".into());
    m.effort = Some("high".into());
    let spec = codex().launch_spec(&m).unwrap();
    let w = words(&spec);
    let at = w.iter().position(|a| *a == "-m").unwrap();
    assert_eq!(w[at + 1], "gpt-5.5");
    assert_eq!(
        overrides(&spec)["model_reasoning_effort"].as_str(),
        Some("high")
    );
    let bare = codex().launch_spec(&member(dir.path())).unwrap();
    assert!(!words(&bare).contains(&"-m"));
    assert!(!overrides(&bare).contains_key("model_reasoning_effort"));
}

#[test]
fn trust_is_passed_only_when_accepted_and_names_the_cwd() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = member(dir.path());
    assert!(!overrides(&codex().launch_spec(&m).unwrap()).contains_key("projects"));
    m.accept_trust = true;
    let o = overrides(&codex().launch_spec(&m).unwrap());
    assert_eq!(
        o["projects"]["/work"]["trust_level"].as_str(),
        Some("trusted")
    );
}

#[test]
fn a_worktree_member_trusts_the_main_repository() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = member(dir.path());
    let repo = dir.path().join("repo");
    let tree = repo.join(".worktrees/api");
    std::fs::create_dir_all(&tree).unwrap();
    std::fs::write(
        tree.join(".git"),
        format!("gitdir: {}/.git/worktrees/api\n", repo.display()),
    )
    .unwrap();
    m.cwd = tree;
    m.accept_trust = true;
    let o = overrides(&codex().launch_spec(&m).unwrap());
    let key = repo.display().to_string();
    assert_eq!(
        o["projects"][key.as_str()]["trust_level"].as_str(),
        Some("trusted")
    );
    assert_eq!(o["projects"].as_table().unwrap().len(), 1);
}

#[test]
fn the_token_is_only_in_its_private_file() {
    let dir = tempfile::tempdir().unwrap();
    let m = member(dir.path());
    let spec = codex().launch_spec(&m).unwrap();
    assert!(
        spec.argv.iter().all(|a| !a.contains(TOKEN)),
        "{:?}",
        spec.argv
    );
    assert!(!format!("{spec:?}").contains(TOKEN));
    assert_eq!(spec.files.len(), 1, "{:?}", spec.files);
    let file = &spec.files[0];
    assert_eq!(file.path, m.member_dir.join("token"));
    assert_eq!(file.bytes, TOKEN.as_bytes());
    assert_eq!(file.mode, 0o600);
    // Launch writes the file; check that the mode it is given sticks.
    std::fs::create_dir_all(&m.member_dir).unwrap();
    std::fs::write(&file.path, &file.bytes).unwrap();
    std::fs::set_permissions(&file.path, std::fs::Permissions::from_mode(file.mode)).unwrap();
    assert_eq!(
        std::fs::metadata(&file.path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

/// The agent's shell gets the member's scratch folder, as a Claude member's
/// and a pi member's do.
#[test]
fn the_shell_environment_names_the_members_scratch_folder() {
    let dir = tempfile::tempdir().unwrap();
    let m = member(dir.path());
    let o = overrides(&codex().launch_spec(&m).unwrap());
    let scratch = dir.path().join("home/scratch/demo/api");
    assert_eq!(
        o["shell_environment_policy.set"]
            .get("TEAMWIRE_SCRATCH")
            .and_then(|v| v.as_str()),
        Some(scratch.to_str().unwrap())
    );
}

#[test]
fn identity_and_servers_name_the_token_file() {
    let dir = tempfile::tempdir().unwrap();
    let m = member(dir.path());
    let home = dir.path().join("home");
    let token = m.member_dir.join("token").display().to_string();
    let o = overrides(&codex().launch_spec(&m).unwrap());
    let set = &o["shell_environment_policy.set"];
    assert_eq!(set["TEAMWIRE_TEAM"].as_str(), Some("demo"));
    assert_eq!(set["TEAMWIRE_ROLE"].as_str(), Some("api"));
    assert_eq!(set["TEAMWIRE_HOME"].as_str(), Some(home.to_str().unwrap()));
    assert_eq!(set["TEAMWIRE_TOKEN_FILE"].as_str(), Some(token.as_str()));
    let mcp = &o["mcp_servers.teamwire"];
    assert_eq!(mcp["command"].as_str(), Some("/usr/local/bin/teamwire"));
    let args: Vec<&str> = mcp["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    assert_eq!(
        args,
        [
            "--home",
            home.to_str().unwrap(),
            "mcp",
            "--team",
            "demo",
            "--role",
            "api"
        ]
    );
    assert_eq!(
        mcp["env"]["TEAMWIRE_TOKEN_FILE"].as_str(),
        Some(token.as_str())
    );
}

#[test]
fn hooks_run_one_quoted_command_that_sets_the_identity() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = member(dir.path());
    m.teamwire_bin = "/opt/my tools/teamwire".into();
    let o = overrides(&codex().launch_spec(&m).unwrap());
    let home = dir.path().join("home");
    let token = m.member_dir.join("token");
    let stop = &o["hooks.Stop"][0]["hooks"][0];
    assert_eq!(stop["type"].as_str(), Some("command"));
    let config = Config::default();
    assert_eq!(
        stop["timeout"].as_integer(),
        i64::try_from(config.hook_timeout_s).ok()
    );
    // Codex must not kill the hook before its own delivery budget ends.
    assert!(config.hook_budget_s < config.hook_timeout_s);
    assert_eq!(
        stop["command"].as_str().unwrap(),
        format!(
            "TEAMWIRE_TEAM=demo TEAMWIRE_ROLE=api TEAMWIRE_HOME={h} TEAMWIRE_TOKEN_FILE={t} '/opt/my tools/teamwire' --home {h} hook turn-end --team demo --role api",
            h = home.display(),
            t = token.display(),
        )
    );
    for (key, event) in [
        ("hooks.PostToolUse", "post-tool"),
        ("hooks.SessionStart", "session-start"),
    ] {
        let hook = &o[key][0]["hooks"][0];
        assert!(
            hook["command"]
                .as_str()
                .unwrap()
                .ends_with(&format!("hook {event} --team demo --role api"))
        );
        assert!(hook.get("timeout").is_none(), "{key}");
    }
}

#[test]
fn a_path_with_a_space_or_quote_is_quoted_in_the_hook_command() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = member(dir.path());
    m.member_dir = dir.path().join("my home/teams/demo/members/api");
    m.teamwire_bin = "/it's/teamwire".into();
    let o = overrides(&codex().launch_spec(&m).unwrap());
    let command = o["hooks.Stop"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap()
        .to_owned();
    let home = dir.path().join("my home");
    assert!(
        command.contains(&format!("TEAMWIRE_HOME='{}'", home.display())),
        "{command}"
    );
    assert!(command.contains("'/it'\\''s/teamwire'"), "{command}");
}

#[test]
fn the_lead_plugin_is_always_off_and_other_plugins_keep_their_switch() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = member(dir.path());
    let plain = overrides(&codex().launch_spec(&m).unwrap());
    assert_eq!(
        plain["plugins"]["teamwire@teamwire"]["enabled"].as_bool(),
        Some(false)
    );
    m.plugins.insert("teamwire@teamwire".into(), true);
    m.plugins.insert("lint@acme".into(), true);
    m.plugins.insert("noisy@acme".into(), false);
    let o = overrides(&codex().launch_spec(&m).unwrap());
    assert_eq!(
        o["plugins"]["teamwire@teamwire"]["enabled"].as_bool(),
        Some(false)
    );
    assert_eq!(o["plugins"]["lint@acme"]["enabled"].as_bool(), Some(true));
    assert_eq!(o["plugins"]["noisy@acme"]["enabled"].as_bool(), Some(false));
}

#[test]
fn a_member_without_a_token_or_a_contract_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = member(dir.path());
    m.identity.clear();
    assert_eq!(
        codex().launch_spec(&m).unwrap_err().code,
        HarnessErrorCode::Refused
    );
    let mut m = member(dir.path());
    m.contract_path = dir.path().join("missing.md");
    assert_eq!(
        codex().launch_spec(&m).unwrap_err().code,
        HarnessErrorCode::Io
    );
}

#[test]
fn resume_puts_the_session_after_resume_and_lists_the_token_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = member(dir.path());
    m.identity = vec![("TEAMWIRE_TOKEN".into(), "-".into())];
    m.accept_trust = true;
    let id = SessionId::parse(SESSION).unwrap();
    let spec = codex().resume_spec(&m, &id).unwrap();
    assert_eq!(&words(&spec)[..3], ["codex", "resume", SESSION]);
    assert!(words(&spec).contains(&"--no-daemon"));
    assert!(overrides(&spec).contains_key("projects"));
    assert_eq!(spec.files.len(), 1);
    assert_eq!(spec.files[0].path, m.member_dir.join("token"));
}

fn declare(dir: &Path, rel: &str) {
    let file = dir.join(rel);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, "").unwrap();
}

#[test]
fn a_folder_that_declares_its_own_codex_config_is_refused() {
    let id = SessionId::parse(SESSION).unwrap();
    for file in [".codex/config.toml", ".codex/hooks.json"] {
        // In the cwd itself.
        let dir = tempfile::tempdir().unwrap();
        let mut m = member(dir.path());
        m.cwd = dir.path().join("work");
        declare(&m.cwd, file);
        for err in [
            codex().launch_spec(&m).unwrap_err(),
            codex().resume_spec(&m, &id).unwrap_err(),
        ] {
            assert_eq!(err.code, HarnessErrorCode::Refused);
            assert!(err.detail.contains(file), "{err}");
            assert!(err.detail.contains("unreviewed"), "{err}");
        }
        // At the main repository's root, above a worktree cwd.
        let dir = tempfile::tempdir().unwrap();
        let mut m = member(dir.path());
        let repo = dir.path().join("repo");
        let tree = repo.join(".worktrees/api");
        std::fs::create_dir_all(&tree).unwrap();
        std::fs::write(
            tree.join(".git"),
            format!("gitdir: {}/.git/worktrees/api\n", repo.display()),
        )
        .unwrap();
        declare(&repo, file);
        m.cwd = tree;
        assert_eq!(
            codex().launch_spec(&m).unwrap_err().code,
            HarnessErrorCode::Refused
        );
    }
}

#[test]
fn a_clean_folder_and_a_config_above_the_trust_root_are_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let mut m = member(dir.path());
    m.cwd = dir.path().join("work");
    std::fs::create_dir_all(&m.cwd).unwrap();
    // Above the cwd, which is its own trust root: not checked.
    declare(dir.path(), ".codex/config.toml");
    assert!(codex().launch_spec(&m).is_ok());
}
