//! `launch_spec` and `resume_spec`: the argv, and the two per-run files.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests build fixtures from literals and read JSON by key; a missing key should fail the test"
)]

use std::sync::Arc;

use serde_json::Value;
use teamwire_core::{
    Address, Config, Harness, HarnessErrorCode, IdentityChannel, LaunchSpec, MemberKind,
    MemberLaunch, OtelConfig, Role, SessionId, TeamName,
};
use teamwire_fakes::FakePlatform;
use teamwire_harness_claude::{ClaudeCode, ClaudeConfig};

const TOKEN: &str = "tok-0123456789abcdef";

fn claude() -> ClaudeCode {
    ClaudeCode::new(
        Arc::new(FakePlatform::new()),
        ClaudeConfig {
            claude_dir: "/nowhere/claude".into(),
            claude_json: "/nowhere/claude.json".into(),
            claude_bin: "claude".into(),
        },
    )
}

fn member() -> MemberLaunch {
    let team = TeamName::parse("demo").unwrap();
    let role = Role::parse("api").unwrap();
    MemberLaunch {
        address: Address::of(&team, &role),
        team,
        role,
        kind: MemberKind::Implementer,
        cwd: "/work".into(),
        session_id: SessionId::parse("6f1c2d3e-0000-4000-8000-000000000001").unwrap(),
        model: None,
        effort: None,
        member_dir: "/home/tw/teams/demo/members/api".into(),
        contract_path: "/home/tw/teams/demo/contract/api.md".into(),
        identity: vec![
            ("TEAMWIRE_TEAM".into(), "demo".into()),
            ("TEAMWIRE_ROLE".into(), "api".into()),
            ("TEAMWIRE_HOME".into(), "/home/tw".into()),
            ("TEAMWIRE_TOKEN".into(), TOKEN.into()),
        ],
        otel: OtelConfig::default(),
        teamwire_bin: "/usr/local/bin/teamwire".into(),
        plugins: std::collections::BTreeMap::new(),
        context_handoff_tokens: None,
    }
}

fn file_json(spec: &LaunchSpec, name: &str) -> Value {
    let file = spec
        .files
        .iter()
        .find(|f| f.path.file_name().is_some_and(|n| n == name))
        .unwrap();
    serde_json::from_slice(&file.bytes).unwrap()
}

fn settings_env(m: &MemberLaunch) -> Value {
    let spec = claude().launch_spec(m).unwrap();
    file_json(&spec, "settings.json")["env"].clone()
}

fn after<'a>(argv: &'a [String], flag: &str) -> &'a str {
    let at = argv.iter().position(|a| a == flag).unwrap();
    &argv[at + 1]
}

#[test]
fn contract_passed_as_file() {
    let m = member();
    let spec = claude().launch_spec(&m).unwrap();
    assert_eq!(
        after(&spec.argv, "--append-system-prompt-file"),
        "/home/tw/teams/demo/contract/api.md"
    );
    // The contract's text never appears; only its path does, and no argument
    // could hold a newline that herdr would refuse.
    assert!(spec.argv.iter().all(|a| !a.contains('\n')));
    assert!(
        spec.files.iter().all(|f| f.path != m.contract_path),
        "launch writes the contract itself"
    );
}

/// The argv a member's Claude Code starts with, apart from the three flags
/// teamwire adds, given a model and an effort.
const BASE_ARGV: [&str; 10] = [
    "-n",
    "tw_demo_api",
    "--permission-mode",
    "bypassPermissions",
    "--append-system-prompt-file",
    "/home/tw/teams/demo/contract/api.md",
    "--model",
    "opus",
    "--effort",
    "high",
];

#[test]
fn argv_is_the_base_argv_plus_teamwire_additions() {
    let mut m = member();
    m.model = Some("opus".into());
    m.effort = Some("high".into());
    let spec = claude().launch_spec(&m).unwrap();

    assert_eq!(spec.argv.first().map(String::as_str), Some("claude"));
    // The three teamwire additions, each a flag and one value.
    let mut rest: Vec<&str> = Vec::new();
    let mut words = spec.argv.iter().skip(1).map(String::as_str);
    while let Some(word) = words.next() {
        if ["--session-id", "--settings", "--mcp-config"].contains(&word) {
            words.next();
        } else {
            rest.push(word);
        }
    }
    assert_eq!(rest, BASE_ARGV);
    assert_eq!(
        after(&spec.argv, "--session-id"),
        "6f1c2d3e-0000-4000-8000-000000000001"
    );
    assert_eq!(
        after(&spec.argv, "--settings"),
        "/home/tw/teams/demo/members/api/settings.json"
    );
    assert_eq!(
        after(&spec.argv, "--mcp-config"),
        "/home/tw/teams/demo/members/api/mcp.json"
    );
}

#[test]
fn model_and_effort_are_left_out_when_unset() {
    let spec = claude().launch_spec(&member()).unwrap();
    assert!(!spec.argv.iter().any(|a| a == "--model" || a == "--effort"));
}

#[test]
fn files_are_settings_and_mcp_under_member_dir_and_private() {
    let m = member();
    let spec = claude().launch_spec(&m).unwrap();
    let mut names: Vec<_> = spec
        .files
        .iter()
        .map(|f| f.path.file_name().unwrap().to_str().unwrap().to_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["mcp.json", "settings.json"]);
    for f in &spec.files {
        assert!(f.path.starts_with(&m.member_dir));
        assert_eq!(f.mode, 0o600);
    }
    assert_eq!(spec.identity_channel, IdentityChannel::HarnessConfig);
    assert!(spec.env.is_empty());
}

#[test]
fn settings_env_has_team_role_home_and_token() {
    let env = settings_env(&member());
    assert_eq!(env["TEAMWIRE_TEAM"], "demo");
    assert_eq!(env["TEAMWIRE_ROLE"], "api");
    assert_eq!(env["TEAMWIRE_HOME"], "/home/tw");
    assert_eq!(env["TEAMWIRE_TOKEN"], TOKEN);
}

/// Temporary files go in a folder of the member's own, outside the repository
/// and outside the team directory that close archives.
#[test]
fn settings_env_names_the_members_scratch_folder() {
    let env = settings_env(&member());
    assert_eq!(env["TEAMWIRE_SCRATCH"], "/home/tw/scratch/demo/api");
}

#[test]
fn settings_env_fills_team_role_and_home_when_identity_lacks_them() {
    let mut m = member();
    m.identity.retain(|(k, _)| k == "TEAMWIRE_TOKEN");
    let env = settings_env(&m);
    assert_eq!(env["TEAMWIRE_TEAM"], "demo");
    assert_eq!(env["TEAMWIRE_ROLE"], "api");
    assert_eq!(env["TEAMWIRE_HOME"], "/home/tw");
}

#[test]
fn settings_env_resource_attributes_name_team_and_role() {
    assert_eq!(
        settings_env(&member())["OTEL_RESOURCE_ATTRIBUTES"],
        "teamwire.team=demo,teamwire.role=api"
    );
}

#[test]
fn settings_env_keeps_the_launchers_resource_attributes() {
    let mut m = member();
    m.identity.push((
        "OTEL_RESOURCE_ATTRIBUTES".into(),
        "deployment.environment=dev".into(),
    ));
    assert_eq!(
        settings_env(&m)["OTEL_RESOURCE_ATTRIBUTES"],
        "deployment.environment=dev,teamwire.team=demo,teamwire.role=api"
    );
}

#[test]
fn settings_env_has_no_otlp_variable_when_export_is_off() {
    let env = settings_env(&member());
    let names: Vec<&String> = env.as_object().unwrap().keys().collect();
    assert!(names.iter().all(|k| !k.starts_with("OTEL_EXPORTER_OTLP_")));
}

#[test]
fn settings_env_has_otlp_endpoint_when_export_is_on() {
    let mut m = member();
    m.otel = OtelConfig {
        endpoint: Some("http://127.0.0.1:4318".into()),
        headers: Vec::new(),
    };
    let env = settings_env(&m);
    assert_eq!(env["OTEL_EXPORTER_OTLP_ENDPOINT"], "http://127.0.0.1:4318");
    assert!(env.get("OTEL_EXPORTER_OTLP_HEADERS").is_none());
}

#[test]
fn settings_env_has_otlp_headers_when_export_is_on() {
    let mut m = member();
    m.otel = OtelConfig {
        endpoint: Some("http://127.0.0.1:4318".into()),
        headers: vec![
            ("x-api-key".into(), "a,b".into()),
            ("x-team".into(), "t".into()),
        ],
    };
    assert_eq!(
        settings_env(&m)["OTEL_EXPORTER_OTLP_HEADERS"],
        "x-api-key=a%2Cb,x-team=t"
    );
}

#[test]
fn otel_env_matches_what_launch_puts_in_settings() {
    let mut m = member();
    m.otel = OtelConfig {
        endpoint: Some("http://collector:4318".into()),
        headers: Vec::new(),
    };
    let vars = claude().otel_env(&m.team, &m.role, &m.otel);
    let endpoint = vars
        .iter()
        .find(|(k, _)| k == "OTEL_EXPORTER_OTLP_ENDPOINT");
    assert_eq!(
        endpoint.map(|(_, v)| v.as_str()),
        Some("http://collector:4318")
    );
    let attrs = vars
        .iter()
        .find(|(k, _)| k == "OTEL_RESOURCE_ATTRIBUTES")
        .unwrap();
    assert!(attrs.1.ends_with("teamwire.team=demo,teamwire.role=api"));
}

#[test]
fn stop_hook_names_team_and_role_and_has_the_configured_timeout() {
    let spec = claude().launch_spec(&member()).unwrap();
    let hook = &file_json(&spec, "settings.json")["hooks"]["Stop"][0]["hooks"][0];
    assert_eq!(hook["type"], "command");
    assert_eq!(
        hook["command"],
        "/usr/local/bin/teamwire hook claude-stop --team demo --role api"
    );
    let config = Config::default();
    assert_eq!(hook["timeout"], config.hook_timeout_s);
    // Claude must not kill the hook before its own delivery budget ends.
    assert!(config.hook_budget_s < config.hook_timeout_s);
}

#[test]
fn post_tool_hook_names_team_and_role() {
    let spec = claude().launch_spec(&member()).unwrap();
    let hook = &file_json(&spec, "settings.json")["hooks"]["PostToolUse"][0]["hooks"][0];
    assert_eq!(
        hook["command"],
        "/usr/local/bin/teamwire hook claude-post-tool --team demo --role api"
    );
}

/// The permission hook answers a dialog only within Claude Code's own 120 s
/// countdown: it waits 100 s, and Claude Code ends it at 110 s (ADR 0052).
#[test]
fn permission_hook_names_team_and_role_and_stops_before_the_countdown() {
    let spec = claude().launch_spec(&member()).unwrap();
    let settings = file_json(&spec, "settings.json");
    let groups = &settings["hooks"]["PermissionRequest"];
    assert!(groups.is_array(), "no PermissionRequest hook in {settings}");
    let groups = groups.as_array().unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].get("matcher"), None, "every tool, no matcher");
    let hook = &groups[0]["hooks"][0];
    assert_eq!(hook["type"], "command");
    assert_eq!(
        hook["command"],
        "/usr/local/bin/teamwire hook permission --team demo --role api"
    );
    assert_eq!(hook["timeout"], 110);
    assert_eq!(teamwire_core::PERMISSION_HOOK_TIMEOUT_S, 110);
    let wait = teamwire_core::PERMISSION_WAIT.as_secs();
    assert!(wait < teamwire_core::PERMISSION_HOOK_TIMEOUT_S);
    // Claude Code denies a dialog with a countdown on its own at 120 s.
    let countdown_s = 120;
    assert!(teamwire_core::PERMISSION_HOOK_TIMEOUT_S < countdown_s);
}

#[test]
fn hook_command_quotes_a_binary_path_with_a_space() {
    let mut m = member();
    m.teamwire_bin = "/Users/A B/bin/teamwire".into();
    let spec = claude().launch_spec(&m).unwrap();
    let hook = &file_json(&spec, "settings.json")["hooks"]["Stop"][0]["hooks"][0];
    assert!(
        hook["command"]
            .as_str()
            .unwrap()
            .starts_with("'/Users/A B/bin/teamwire' hook claude-stop")
    );
}

#[test]
fn mcp_json_starts_the_teamwire_server_with_the_token_in_its_env_not_argv() {
    let spec = claude().launch_spec(&member()).unwrap();
    let server = &file_json(&spec, "mcp.json")["mcpServers"]["teamwire"];
    assert_eq!(server["command"], "/usr/local/bin/teamwire");
    let args: Vec<&str> = server["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    assert_eq!(
        args,
        [
            "--home", "/home/tw", "mcp", "--team", "demo", "--role", "api"
        ]
    );
    assert!(!args.contains(&TOKEN));
    assert_eq!(server["env"]["TEAMWIRE_TOKEN"], TOKEN);
}

#[test]
fn mcp_json_derives_the_home_when_identity_lacks_it() {
    let mut m = member();
    m.identity.retain(|(k, _)| k != "TEAMWIRE_HOME");
    let spec = claude().launch_spec(&m).unwrap();
    let args = &file_json(&spec, "mcp.json")["mcpServers"]["teamwire"]["args"];
    assert_eq!(args[0], "--home");
    assert_eq!(args[1], "/home/tw");
}

#[test]
fn a_member_without_a_token_is_refused() {
    let mut m = member();
    m.identity.retain(|(k, _)| k != "TEAMWIRE_TOKEN");
    let err = claude().launch_spec(&m).unwrap_err();
    assert_eq!(err.code, HarnessErrorCode::Refused);
}

#[test]
fn debug_of_the_spec_hides_the_token() {
    let spec = claude().launch_spec(&member()).unwrap();
    assert!(!format!("{spec:?}").contains(TOKEN));
}

#[test]
fn resume_spec_resumes_the_given_session_with_the_same_files() {
    let m = member();
    let other = SessionId::parse("6f1c2d3e-0000-4000-8000-0000000000ff").unwrap();
    let fresh = claude().launch_spec(&m).unwrap();
    let resumed = claude().resume_spec(&m, &other).unwrap();
    assert_eq!(after(&resumed.argv, "--resume"), other.as_str());
    assert!(!resumed.argv.iter().any(|a| a == "--session-id"));
    assert_eq!(resumed.files, fresh.files);
    assert_eq!(
        after(&resumed.argv, "--settings"),
        after(&fresh.argv, "--settings")
    );
}

#[test]
fn plugins_become_enabled_plugins_in_settings_beside_the_lead_plugin_switched_off() {
    let mut m = member();
    m.plugins = [("ponytail@ponytail".to_owned(), false)].into();
    let spec = claude().launch_spec(&m).unwrap();
    let settings = file_json(&spec, "settings.json");
    assert_eq!(
        settings["enabledPlugins"],
        serde_json::json!({"ponytail@ponytail": false, "teamwire@teamwire": false})
    );
}

#[test]
fn a_member_never_loads_the_lead_plugin_whatever_the_spec_says() {
    let mut m = member();
    m.plugins = [("teamwire@teamwire".to_owned(), true)].into();
    let settings = file_json(&claude().launch_spec(&m).unwrap(), "settings.json");
    assert_eq!(
        settings["enabledPlugins"],
        serde_json::json!({"teamwire@teamwire": false})
    );
}

#[test]
fn no_plugins_leaves_only_the_lead_plugin_switched_off() {
    let m = member();
    let spec = claude().launch_spec(&m).unwrap();
    let settings = file_json(&spec, "settings.json");
    let keys: Vec<&String> = settings.as_object().unwrap().keys().collect();
    assert_eq!(
        keys,
        [
            "autoCompactEnabled",
            "autoCompactWindow",
            "enabledPlugins",
            "env",
            "hooks"
        ]
    );
    assert_eq!(
        settings["enabledPlugins"],
        serde_json::json!({"teamwire@teamwire": false})
    );
}

fn backstop(model: Option<&str>, tokens: Option<u64>) -> (serde_json::Value, serde_json::Value) {
    let mut m = member();
    m.model = model.map(str::to_owned);
    m.context_handoff_tokens = tokens;
    let settings = file_json(&claude().launch_spec(&m).unwrap(), "settings.json");
    (
        settings["autoCompactEnabled"].clone(),
        settings["autoCompactWindow"].clone(),
    )
}

#[test]
fn the_backstop_is_twice_the_default_handoff_point_of_the_model() {
    // Opus 5.5: window 1M, handoff 40% = 400,000, backstop 800,000.
    assert_eq!(
        backstop(Some("claude-opus-5-5"), None),
        (serde_json::json!(true), serde_json::json!(800_000))
    );
    // An Opus 4.6 member has a 200K window: handoff 80,000, so 160,000.
    assert_eq!(backstop(Some("claude-opus-4-6"), None).1, 160_000);
}

#[test]
fn the_backstop_is_twice_the_configured_threshold_but_at_least_100k() {
    assert_eq!(backstop(Some("opus"), Some(30_000)).1, 100_000);
    assert_eq!(backstop(Some("opus"), Some(200_000)).1, 400_000);
}

#[test]
fn the_backstop_never_exceeds_the_window() {
    assert_eq!(
        backstop(Some("claude-sonnet-4-6"), Some(150_000)).1,
        200_000
    );
}

#[test]
fn a_member_with_no_model_gets_the_backstop_of_a_large_window() {
    assert_eq!(backstop(None, None).1, 800_000);
    assert_eq!(backstop(Some("mystery-1"), None).1, 800_000);
}
