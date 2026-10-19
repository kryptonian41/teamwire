//! `fake-herdr` driven as a process: its JSON envelopes, its state, its
//! faults and its call log.

#![expect(
    clippy::unwrap_used,
    clippy::panic,
    clippy::disallowed_methods,
    reason = "test helpers build fixtures from literals and fail the test with a message; one test signals the agent with kill(1)"
)]

use serde_json::Value;
use teamwire_fakes::{ClaudeScript, FakeHerdrEnv, HerdrFaults, Startup, Step};

fn json(out: &teamwire_fakes::CliOutput) -> Value {
    out.result()
        .unwrap_or_else(|| panic!("no result in {out:?}"))
}

fn s(v: &Value, pointer: &str) -> String {
    v.pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// A workspace with one pane, returning the pane id.
fn pane(env: &FakeHerdrEnv, cwd: &str) -> String {
    let out = env
        .run(&[
            "workspace",
            "create",
            "--cwd",
            cwd,
            "--label",
            "Team",
            "--no-focus",
        ])
        .unwrap();
    s(&json(&out), "/root_pane/pane_id")
}

fn settings(env: &FakeHerdrEnv) -> String {
    env.write_file("settings.json", r#"{"env":{},"hooks":{}}"#)
        .unwrap()
        .display()
        .to_string()
}

fn start(env: &FakeHerdrEnv, name: &str, pane: &str, session: &str) -> teamwire_fakes::CliOutput {
    let settings = settings(env);
    env.run(&[
        "agent",
        "start",
        name,
        "--kind",
        "claude",
        "--pane",
        pane,
        "--timeout",
        "20000",
        "--",
        "-n",
        name,
        "--session-id",
        session,
        "--settings",
        &settings,
    ])
    .unwrap()
}

#[test]
fn workspace_tab_and_pane_commands_use_herdrs_envelope() {
    let env = FakeHerdrEnv::new().unwrap();
    let out = env
        .run(&[
            "workspace",
            "create",
            "--cwd",
            "/tmp",
            "--label",
            "Team",
            "--no-focus",
        ])
        .unwrap();
    assert_eq!(out.code, Some(0));
    let doc: Value = serde_json::from_str(out.stdout.trim()).unwrap();
    assert_eq!(doc.pointer("/id").unwrap(), "cli:workspace:create");
    let result = json(&out);
    let ws = s(&result, "/workspace/workspace_id");
    assert_eq!(s(&result, "/workspace/label"), "Team");
    let first_tab = s(&result, "/tab/tab_id");
    assert!(first_tab.starts_with(&ws));

    let tab = env
        .run(&[
            "tab",
            "create",
            "--workspace",
            &ws,
            "--cwd",
            "/tmp",
            "--label",
            "api",
            "--no-focus",
        ])
        .unwrap();
    let tab_id = s(&json(&tab), "/tab/tab_id");
    let renamed = env
        .run(&["tab", "rename", &tab_id, "implementer api"])
        .unwrap();
    assert_eq!(s(&json(&renamed), "/tab/label"), "implementer api");

    let list = env.run(&["pane", "list", "--workspace", &ws]).unwrap();
    assert_eq!(
        json(&list)
            .pointer("/panes")
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let pane_id = s(&json(&tab), "/root_pane/pane_id");
    assert_eq!(
        s(
            &json(&env.run(&["pane", "get", &pane_id]).unwrap()),
            "/pane/tab_id"
        ),
        tab_id
    );

    let missing = env.run(&["workspace", "get", "w999"]).unwrap();
    assert_eq!(missing.code, Some(1));
    assert_eq!(missing.error_code().as_deref(), Some("workspace_not_found"));
    assert_eq!(
        env.run(&["pane", "get", "w1:p999"])
            .unwrap()
            .error_code()
            .as_deref(),
        Some("pane_not_found")
    );
    let bad_tab = env.run(&["tab", "create", "--workspace", "w999"]).unwrap();
    assert_eq!(bad_tab.error_code().as_deref(), Some("workspace_not_found"));
}

#[test]
fn closing_the_last_pane_closes_its_tab_and_workspace() {
    let env = FakeHerdrEnv::new().unwrap();
    let pane_id = pane(&env, "/tmp");
    let ws = pane_id.split(':').next().unwrap().to_owned();
    assert_eq!(env.run(&["workspace", "get", &ws]).unwrap().code, Some(0));
    let closed = env.run(&["pane", "close", &pane_id]).unwrap();
    assert_eq!(s(&json(&closed), "/type"), "ok");
    assert_eq!(
        env.run(&["workspace", "get", &ws])
            .unwrap()
            .error_code()
            .as_deref(),
        Some("workspace_not_found")
    );
    assert_eq!(
        env.run(&["pane", "close", &pane_id])
            .unwrap()
            .error_code()
            .as_deref(),
        Some("pane_not_found")
    );
}

#[test]
fn agent_lifecycle_start_prompt_read_list_get_close() {
    let mut env = FakeHerdrEnv::new().unwrap();
    env.set_script(&ClaudeScript {
        turns: vec![vec![Step::Print("worked on it".into())]],
        ..ClaudeScript::default()
    })
    .unwrap();
    let pane_id = pane(&env, "/tmp");
    let started = start(&env, "tw_demo_api", &pane_id, "sess-1");
    assert_eq!(started.code, Some(0), "{started:?}");
    let agent = json(&started);
    assert_eq!(s(&agent, "/agent/name"), "tw_demo_api");
    assert_eq!(s(&agent, "/agent/agent_status"), "idle");
    assert_eq!(s(&agent, "/agent/agent_session/value"), "sess-1");
    assert_eq!(s(&agent, "/agent/pane_id"), pane_id);

    let prompt = env
        .run(&[
            "agent",
            "prompt",
            "tw_demo_api",
            "hello agent",
            "--wait",
            "--timeout",
            "20000",
        ])
        .unwrap();
    assert_eq!(prompt.code, Some(0), "{prompt:?}");
    let read = env
        .run(&[
            "agent",
            "read",
            "tw_demo_api",
            "--source",
            "recent-unwrapped",
            "--lines",
            "40",
        ])
        .unwrap();
    let text = read.stdout.clone();
    assert!(text.contains("> hello agent"), "{text}");
    assert!(text.contains("worked on it"), "{text}");

    // After a turn real herdr shows `done` (ready for input, not yet seen).
    let get = env.run(&["agent", "get", &pane_id]).unwrap();
    assert_eq!(s(&json(&get), "/agent/agent_status"), "done");
    assert_eq!(
        json(&get).pointer("/agent/interactive_ready").unwrap(),
        true
    );
    let list = json(&env.run(&["agent", "list"]).unwrap());
    assert_eq!(
        list.pointer("/agents").unwrap().as_array().unwrap().len(),
        1
    );
    assert_eq!(env.session_files().len(), 1);

    env.run(&["pane", "close", &pane_id]).unwrap();
    let after = env.run(&["agent", "get", "tw_demo_api"]).unwrap();
    assert_eq!(after.error_code().as_deref(), Some("agent_not_found"));
    let list = json(&env.run(&["agent", "list"]).unwrap());
    assert!(
        list.pointer("/agents")
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn the_started_agent_gets_a_cleared_environment() {
    let mut env = FakeHerdrEnv::new().unwrap();
    env.set_script(&ClaudeScript {
        turns: vec![vec![Step::Run("env > \"$TMPDIR/agent-env.txt\"".into())]],
        ..ClaudeScript::default()
    })
    .unwrap();
    let pane_id = pane(&env, "/tmp");
    let settings = settings(&env);
    let started = env
        .run_with(
            &[
                "agent",
                "start",
                "tw_demo_api",
                "--kind",
                "claude",
                "--pane",
                &pane_id,
                "--",
                "--settings",
                &settings,
            ],
            &[
                ("TEAMWIRE_TOKEN", "leak"),
                ("TEAMWIRE_ROLE", "api"),
                ("HERDR_PANE_ID", "caller-pane"),
                ("FAKE_KEEP", "yes"),
            ],
        )
        .unwrap();
    assert_eq!(started.code, Some(0), "{started:?}");
    let prompt = env
        .run(&[
            "agent",
            "prompt",
            "tw_demo_api",
            "dump env",
            "--wait",
            "--timeout",
            "20000",
        ])
        .unwrap();
    assert_eq!(prompt.code, Some(0), "{prompt:?}");
    let dump = std::fs::read_to_string(env.root().join("tmp/agent-env.txt")).unwrap();
    for present in [
        "PATH=",
        "HOME=",
        "TMPDIR=",
        "FAKE_KEEP=yes",
        "FAKE_PANE_DIR=",
    ] {
        assert!(dump.contains(present), "{present} missing from {dump}");
    }
    for absent in [
        "TEAMWIRE_TOKEN",
        "TEAMWIRE_ROLE",
        "caller-pane",
        "TEAMWIRE_CLAUDE_DIR",
    ] {
        assert!(!dump.contains(absent), "{absent} leaked into {dump}");
    }
    // The agent gets its own pane's variable, as real herdr sets it.
    assert!(
        dump.contains(&format!("HERDR_PANE_ID={pane_id}")),
        "own pane id missing from {dump}"
    );
}

#[test]
fn the_escalated_trust_dialog_blocks_start_and_prompts_until_answered() {
    let mut env = FakeHerdrEnv::new().unwrap();
    env.set_script(&ClaudeScript {
        startup: Startup::EscalatedTrust,
        ..ClaudeScript::default()
    })
    .unwrap();
    let pane_id = pane(&env, "/tmp");
    let started = start(&env, "tw_demo_api", &pane_id, "sess-2");
    assert_eq!(
        started.error_code().as_deref(),
        Some("agent_not_ready"),
        "{started:?}"
    );
    let get = env.run(&["agent", "get", "tw_demo_api"]).unwrap();
    assert_eq!(s(&json(&get), "/agent/agent_status"), "blocked");
    let screen = env
        .run(&["agent", "read", "tw_demo_api", "--lines", "20"])
        .unwrap()
        .stdout;
    assert!(
        screen.contains(teamwire_fakes::ESCALATED_TRUST_TEXT),
        "{screen}"
    );

    let refused = env.run(&["agent", "prompt", "tw_demo_api", "hi"]).unwrap();
    assert_eq!(refused.error_code().as_deref(), Some("agent_blocked"));

    let bad = env
        .run(&["agent", "send-keys", "tw_demo_api", "not-a-key"])
        .unwrap();
    assert_eq!(bad.error_code().as_deref(), Some("invalid_key"));
    assert_eq!(
        env.run(&["agent", "send-keys", "tw_demo_api", "down", "enter"])
            .unwrap()
            .code,
        Some(0)
    );
    for _ in 0..200 {
        let status = s(
            &json(&env.run(&["agent", "get", "tw_demo_api"]).unwrap()),
            "/agent/agent_status",
        );
        if status == "idle" {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("the agent never became idle after the dialog was answered");
}

#[test]
fn busy_twice_then_success() {
    let mut env = FakeHerdrEnv::new().unwrap();
    env.set_faults(&HerdrFaults::new().error("agent start", None, Some(2), "agent_pane_busy"))
        .unwrap();
    let pane_id = pane(&env, "/tmp");
    for _ in 0..2 {
        let out = start(&env, "tw_demo_api", &pane_id, "s");
        assert_eq!(out.code, Some(1));
        assert_eq!(out.error_code().as_deref(), Some("agent_pane_busy"));
    }
    assert_eq!(start(&env, "tw_demo_api", &pane_id, "s").code, Some(0));
    let starts: Vec<_> = env
        .calls()
        .into_iter()
        .filter(|c| c.command() == "agent start")
        .collect();
    assert_eq!(starts.len(), 3);
    assert_eq!(
        starts.first().unwrap().fault.as_deref(),
        Some("error:agent_pane_busy")
    );
    assert_eq!(starts.last().unwrap().fault, None);
}

#[test]
fn a_stalled_prompt_with_wait_fails_but_the_text_was_typed() {
    let mut env = FakeHerdrEnv::new().unwrap();
    let pane_id = pane(&env, "/tmp");
    assert_eq!(start(&env, "tw_demo_api", &pane_id, "s").code, Some(0));
    env.set_faults(&HerdrFaults::new().stalled_prompt(Some("tw_demo_api"), Some(1)))
        .unwrap();
    let out = env
        .run(&[
            "agent",
            "prompt",
            "tw_demo_api",
            "did this arrive",
            "--wait",
        ])
        .unwrap();
    assert_eq!(out.error_code().as_deref(), Some("agent_prompt_stalled"));
    wait_for_screen(&env, "> did this arrive");
}

fn wait_for_screen(env: &FakeHerdrEnv, needle: &str) {
    for _ in 0..200 {
        let text = env.run(&["agent", "read", "tw_demo_api"]).unwrap().stdout;
        if text.contains(needle) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("`{needle}` never reached the agent's screen");
}

#[test]
fn a_stall_fault_without_wait_does_not_stall() {
    let mut env = FakeHerdrEnv::new().unwrap();
    let pane_id = pane(&env, "/tmp");
    assert_eq!(start(&env, "tw_demo_api", &pane_id, "s").code, Some(0));
    env.set_faults(&HerdrFaults::new().stalled_prompt(Some("tw_demo_api"), None))
        .unwrap();
    let out = env
        .run(&["agent", "prompt", "tw_demo_api", "no wait here"])
        .unwrap();
    assert_eq!(out.code, Some(0), "{out:?}");
    wait_for_screen(&env, "> no wait here");
}

#[test]
fn a_timed_out_prompt_fails_with_timeout_but_the_text_was_typed() {
    let mut env = FakeHerdrEnv::new().unwrap();
    let pane_id = pane(&env, "/tmp");
    assert_eq!(start(&env, "tw_demo_api", &pane_id, "s").code, Some(0));
    env.set_faults(&HerdrFaults::new().timed_out_prompt(Some("tw_demo_api"), Some(1)))
        .unwrap();
    let out = env
        .run(&["agent", "prompt", "tw_demo_api", "did this arrive"])
        .unwrap();
    assert_eq!(out.code, Some(1));
    assert_eq!(out.error_code().as_deref(), Some("timeout"));
    wait_for_screen(&env, "> did this arrive");
    // The rule fired once; the next prompt works.
    let again = env
        .run(&["agent", "prompt", "tw_demo_api", "second"])
        .unwrap();
    assert_eq!(again.code, Some(0), "{again:?}");
}

fn signal(name: &str, pid: &str) {
    std::process::Command::new("kill")
        .args([name, pid])
        .status()
        .unwrap();
}

#[test]
fn a_prompt_with_wait_stalls_when_the_agent_shows_no_activity() {
    let env = FakeHerdrEnv::new().unwrap();
    let pane_id = pane(&env, "/tmp");
    assert_eq!(start(&env, "tw_demo_api", &pane_id, "s").code, Some(0));
    let session_file = env.session_files().pop().unwrap();
    let pid = session_file
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    // A stopped process reads no prompt and shows no activity.
    signal("-STOP", &pid);
    let out = env
        .run_with(
            &[
                "agent",
                "prompt",
                "tw_demo_api",
                "hi",
                "--wait",
                "--timeout",
                "20000",
            ],
            &[("FAKE_HERDR_WAIT_MS", "300")],
        )
        .unwrap();
    signal("-CONT", &pid);
    assert_eq!(
        out.error_code().as_deref(),
        Some("agent_prompt_stalled"),
        "{out:?}"
    );
}

#[test]
fn a_hang_runs_past_the_callers_timeout_and_garbage_is_not_json() {
    let mut env = FakeHerdrEnv::new().unwrap();
    env.set_faults(
        &HerdrFaults::new()
            .hang("pane list", None, Some(1))
            .garbage("agent list", None, None),
    )
    .unwrap();
    let started = std::time::Instant::now();
    let out = env
        .run_with(&["pane", "list"], &[("FAKE_HERDR_HANG_MS", "400")])
        .unwrap();
    assert!(started.elapsed() >= std::time::Duration::from_millis(400));
    assert_eq!(out.error_code().as_deref(), Some("timeout"));
    assert_eq!(env.run(&["pane", "list"]).unwrap().code, Some(0));

    let garbage = env.run(&["agent", "list"]).unwrap();
    assert_eq!(garbage.code, Some(0));
    assert!(serde_json::from_str::<Value>(garbage.stdout.trim()).is_err());
    assert!(garbage.result().is_none());
}

#[test]
fn agent_start_refuses_bad_input_like_herdr() {
    let env = FakeHerdrEnv::new().unwrap();
    let pane_id = pane(&env, "/tmp");
    let newline = env
        .run(&[
            "agent",
            "start",
            "tw_demo_api",
            "--kind",
            "claude",
            "--pane",
            &pane_id,
            "--",
            "line one\nline two",
        ])
        .unwrap();
    assert_eq!(
        newline.error_code().as_deref(),
        Some("invalid_agent_argument")
    );
    let no_pane = start(&env, "tw_demo_api", "w9:p9", "s");
    assert_eq!(
        no_pane.error_code().as_deref(),
        Some("agent_pane_not_found")
    );
    let other = env
        .run(&[
            "agent", "start", "x", "--kind", "gemini", "--pane", &pane_id,
        ])
        .unwrap();
    assert_eq!(
        other.error_code().as_deref(),
        Some("unsupported_agent_kind")
    );
}

#[test]
fn a_codex_agent_meets_the_same_newline_rule() {
    let env = FakeHerdrEnv::new().unwrap();
    let pane_id = pane(&env, "/tmp");
    let run = |arg: &str| {
        env.run(&[
            "agent",
            "start",
            "tw_demo_api",
            "--kind",
            "codex",
            "--pane",
            &pane_id,
            "--",
            arg,
        ])
        .unwrap()
    };
    assert_eq!(
        run("a=\"\"\"\nb\"\"\"").error_code().as_deref(),
        Some("invalid_agent_argument")
    );
    assert_eq!(run("a=\"b\\nc\"").code, Some(0));
}

#[test]
fn a_busy_pane_and_a_taken_name_are_refused() {
    let env = FakeHerdrEnv::new().unwrap();
    let a = pane(&env, "/tmp");
    assert_eq!(start(&env, "tw_demo_api", &a, "s1").code, Some(0));
    assert_eq!(
        start(&env, "tw_demo_web", &a, "s2").error_code().as_deref(),
        Some("agent_pane_busy")
    );
    let ws = a.split(':').next().unwrap();
    let tab = env
        .run(&["tab", "create", "--workspace", ws, "--cwd", "/tmp"])
        .unwrap();
    let b = s(&json(&tab), "/root_pane/pane_id");
    assert_eq!(
        start(&env, "tw_demo_api", &b, "s3").error_code().as_deref(),
        Some("agent_name_taken")
    );
}

#[test]
fn every_call_is_logged_with_its_kind() {
    let env = FakeHerdrEnv::new().unwrap();
    let pane_id = pane(&env, "/tmp");
    env.run(&["pane", "get", &pane_id]).unwrap();
    env.run(&["agent", "list"]).unwrap();
    env.run(&["tab", "rename", "w1:t2", "x"]).unwrap();
    let calls = env.calls();
    let kinds: Vec<(String, String)> = calls
        .iter()
        .map(|c| (c.command(), c.kind.clone()))
        .collect();
    assert_eq!(
        kinds,
        [
            ("workspace create".to_owned(), "write".to_owned()),
            ("pane get".to_owned(), "read".to_owned()),
            ("agent list".to_owned(), "read".to_owned()),
            ("tab rename".to_owned(), "write".to_owned()),
        ]
    );
    assert_eq!(calls.first().unwrap().argv.first().unwrap(), "workspace");
}

fn run_ok(env: &FakeHerdrEnv, args: &[&str]) -> Value {
    let out = env.run(args).unwrap();
    assert_eq!(out.code, Some(0), "{args:?}: {out:?}");
    json(&out)
}

#[test]
fn result_types_and_fields_follow_herdrs_schema() {
    let env = FakeHerdrEnv::new().unwrap();
    let ws = run_ok(
        &env,
        &[
            "workspace",
            "create",
            "--cwd",
            "/tmp",
            "--label",
            "T",
            "--no-focus",
        ],
    );
    for key in [
        "workspace_id",
        "number",
        "label",
        "focused",
        "pane_count",
        "tab_count",
        "active_tab_id",
        "agent_status",
    ] {
        assert!(
            ws.pointer(&format!("/workspace/{key}")).is_some(),
            "workspace.{key}"
        );
    }
    for key in [
        "tab_id",
        "workspace_id",
        "number",
        "label",
        "focused",
        "pane_count",
        "agent_status",
    ] {
        assert!(ws.pointer(&format!("/tab/{key}")).is_some(), "tab.{key}");
    }
    for key in [
        "pane_id",
        "terminal_id",
        "workspace_id",
        "tab_id",
        "focused",
        "agent_status",
        "revision",
    ] {
        assert!(
            ws.pointer(&format!("/root_pane/{key}")).is_some(),
            "root_pane.{key}"
        );
    }
    let pane_id = s(&ws, "/root_pane/pane_id");
    let ws_id = s(&ws, "/workspace/workspace_id");
    let tab_id = s(&ws, "/tab/tab_id");

    let list = run_ok(&env, &["workspace", "list"]);
    assert_eq!(s(&list, "/type"), "workspace_list");
    assert_eq!(
        list.pointer("/workspaces")
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        s(
            &run_ok(&env, &["workspace", "rename", &ws_id, "New"]),
            "/type"
        ),
        "workspace_info"
    );
    assert_eq!(
        s(&run_ok(&env, &["tab", "rename", &tab_id, "Tab"]), "/type"),
        "tab_info"
    );
    let renamed = run_ok(&env, &["pane", "rename", &pane_id, "my pane"]);
    assert_eq!(s(&renamed, "/type"), "pane_info");
    assert_eq!(s(&renamed, "/pane/label"), "my pane");
    assert_eq!(
        s(&run_ok(&env, &["pane", "close", &pane_id]), "/type"),
        "ok"
    );
}

#[test]
fn agent_results_use_herdrs_type_names_shapes_and_ids() {
    let mut env = FakeHerdrEnv::new().unwrap();
    env.set_script(&ClaudeScript::default()).unwrap();
    let pane_id = pane(&env, "/tmp");
    let started = start(&env, "tw_demo_api", &pane_id, "sess-9");
    let result = json(&started);
    assert_eq!(s(&result, "/type"), "agent_started");
    assert!(result.pointer("/argv").unwrap().as_array().unwrap().len() >= 3);
    let doc: Value = serde_json::from_str(started.stdout.trim()).unwrap();
    assert_eq!(doc.pointer("/id").unwrap(), "cli:agent:start");

    let prompted = run_ok(
        &env,
        &[
            "agent",
            "prompt",
            "tw_demo_api",
            "hi",
            "--wait",
            "--timeout",
            "20000",
        ],
    );
    assert_eq!(s(&prompted, "/type"), "agent_prompted");
    // `agent read` is the one command whose success is not a JSON envelope:
    // herdr 0.9.0 prints the screen lines as they are, with no final newline.
    let read = env
        .run(&[
            "agent",
            "read",
            "tw_demo_api",
            "--source",
            "recent",
            "--lines",
            "5",
        ])
        .unwrap();
    assert_eq!(read.code, Some(0), "{read:?}");
    assert!(read.result().is_none(), "no envelope: {read:?}");
    assert!(!read.stdout.ends_with('\n'), "{read:?}");
    assert!(read.stdout.lines().count() <= 5, "{read:?}");
    assert!(read.stdout.lines().any(|l| l.trim() == "> hi"), "{read:?}");
    assert_eq!(
        s(
            &run_ok(&env, &["agent", "send-keys", "tw_demo_api", "esc"]),
            "/type"
        ),
        "ok"
    );

    // The pane list carries the agent's session, as team_parent.py reads it.
    let panes = run_ok(&env, &["pane", "list"]);
    let first = panes.pointer("/panes/0").unwrap();
    assert_eq!(first.pointer("/agent_session/value").unwrap(), "sess-9");
    assert_eq!(first.pointer("/agent").unwrap(), "claude");

    // Errors carry the command id.
    let missing = env.run(&["agent", "get", "nobody"]).unwrap();
    let err: Value = serde_json::from_str(missing.stderr.trim()).unwrap();
    assert_eq!(err.pointer("/id").unwrap(), "cli:agent:get");
    assert_eq!(err.pointer("/error/code").unwrap(), "agent_not_found");
}

#[test]
fn an_agent_without_a_name_is_listed_without_a_name_key() {
    let mut env = FakeHerdrEnv::new().unwrap();
    env.set_script(&ClaudeScript::default()).unwrap();
    let pane_id = pane(&env, "/tmp");
    assert_eq!(start(&env, "tw_demo_api", &pane_id, "s").code, Some(0));
    let unnamed = env.add_unnamed_agent().unwrap();
    let list = run_ok(&env, &["agent", "list"]);
    let agents = list.pointer("/agents").unwrap().as_array().unwrap();
    assert_eq!(agents.len(), 2);
    let bare = agents
        .iter()
        .find(|a| a.pointer("/pane_id").unwrap() == unnamed.as_str())
        .unwrap();
    assert!(bare.get("name").is_none());
    assert_eq!(bare.pointer("/agent_status").unwrap(), "idle");
    assert!(agents.iter().any(|a| a.get("name").is_some()));
}

#[test]
fn interactive_ready_is_true_for_idle_working_and_done_and_false_when_blocked() {
    let mut env = FakeHerdrEnv::new().unwrap();
    env.set_script(&ClaudeScript {
        startup: Startup::Trust,
        ..ClaudeScript::default()
    })
    .unwrap();
    let pane_id = pane(&env, "/tmp");
    let _ = start(&env, "tw_demo_api", &pane_id, "s");
    let blocked = run_ok(&env, &["agent", "get", "tw_demo_api"]);
    assert_eq!(s(&blocked, "/agent/agent_status"), "blocked");
    assert_eq!(blocked.pointer("/agent/interactive_ready").unwrap(), false);
    // Idle: an agent with no dialog.
    let other = FakeHerdrEnv::new().unwrap();
    let pane2 = pane(&other, "/tmp");
    assert_eq!(start(&other, "tw_demo_api", &pane2, "s").code, Some(0));
    let idle = run_ok(&other, &["agent", "get", "tw_demo_api"]);
    assert_eq!(s(&idle, "/agent/agent_status"), "idle");
    assert_eq!(idle.pointer("/agent/interactive_ready").unwrap(), true);
}

#[test]
fn agent_start_checks_kind_and_timeout_like_herdr() {
    let env = FakeHerdrEnv::new().unwrap();
    let pane_id = pane(&env, "/tmp");
    let no_kind = env
        .run(&["agent", "start", "x", "--pane", &pane_id])
        .unwrap();
    assert_eq!(no_kind.code, Some(2));
    assert_eq!(no_kind.stderr.trim(), "missing required --kind");
    for bad in ["3000", "100", "300001"] {
        let out = env
            .run(&[
                "agent",
                "start",
                "x",
                "--kind",
                "claude",
                "--pane",
                &pane_id,
                "--timeout",
                bad,
            ])
            .unwrap();
        assert_eq!(
            out.error_code().as_deref(),
            Some("invalid_timeout"),
            "{bad}"
        );
    }
    // The pane is looked up before the timeout is checked, as in herdr.
    let out = env
        .run(&[
            "agent",
            "start",
            "x",
            "--kind",
            "claude",
            "--pane",
            "w9:p9",
            "--timeout",
            "100",
        ])
        .unwrap();
    assert_eq!(out.error_code().as_deref(), Some("agent_pane_not_found"));
}

#[test]
fn syntax_errors_print_usage_and_exit_two() {
    let env = FakeHerdrEnv::new().unwrap();
    for (args, text) in [
        (
            &["agent", "start"][..],
            "usage: herdr agent start <name> --kind KIND --pane ID [--timeout MS] [-- <agent-args...>]",
        ),
        (&["agent", "get"][..], "usage: herdr agent get <target>"),
        (
            &["agent", "prompt", "x"][..],
            "usage: herdr agent prompt <target> <text> [--wait] [--until STATUS]... [--timeout MS]",
        ),
    ] {
        let out = env.run(args).unwrap();
        assert_eq!(out.code, Some(2), "{args:?}");
        assert_eq!(out.stderr.trim(), text);
        assert!(out.stdout.is_empty());
    }
    let unknown = env.run(&["frobnicate", "now"]).unwrap();
    assert_eq!(unknown.code, Some(2));
    assert!(unknown.stderr.contains("unknown command: frobnicate now"));
    // A syntax error never reaches the server, so it is not in the call log.
    assert!(env.calls().is_empty());
}

#[test]
fn flags_and_values_herdr_refuses_exit_two_with_the_usage_line() {
    let env = FakeHerdrEnv::new().unwrap();
    for (args, usage) in [
        (
            &["agent", "list", "--no-such-flag"][..],
            "usage: herdr agent list",
        ),
        (
            &["agent", "read", "x", "--source", "bogus-source"][..],
            "usage: herdr agent read <target>",
        ),
        (
            &["agent", "prompt", "x", "hi", "--until", "asleep"][..],
            "usage: herdr agent prompt <target> <text>",
        ),
        (
            &["pane", "close", "w1:p1", "--wait"][..],
            "usage: herdr pane close <pane>",
        ),
    ] {
        let out = env.run(args).unwrap();
        assert_eq!(out.code, Some(2), "{args:?}: {out:?}");
        assert!(out.stderr.contains(usage), "{args:?}: {out:?}");
        assert!(out.stdout.is_empty());
    }
    assert!(env.calls().is_empty());
}

#[test]
fn the_flags_the_adapter_and_herdr_accept_are_accepted() {
    let env = FakeHerdrEnv::new().unwrap();
    let pane_id = pane(&env, "/tmp");
    assert_eq!(start(&env, "tw_demo_api", &pane_id, "s").code, Some(0));
    for source in ["visible", "recent", "recent-unwrapped", "detection"] {
        let out = env
            .run(&[
                "agent",
                "read",
                "tw_demo_api",
                "--source",
                source,
                "--lines",
                "5",
            ])
            .unwrap();
        assert_eq!(out.code, Some(0), "{source}: {out:?}");
    }
    let out = env
        .run(&[
            "agent",
            "read",
            "tw_demo_api",
            "--format",
            "text",
            "--strip-ansi",
            "--ansi",
        ])
        .unwrap();
    assert_eq!(out.code, Some(0), "{out:?}");
}

#[test]
fn a_malformed_or_missing_faults_file_is_a_misconfiguration() {
    let env = FakeHerdrEnv::new().unwrap();
    let bad = env.write_file("bad-faults.json", "{\"rules\": [").unwrap();
    let typo = env
        .write_file(
            "typo-faults.json",
            r#"{"rules":[{"cmd":"agent list","tims":1,"action":"hang"}]}"#,
        )
        .unwrap();
    let missing = env.root().join("no-such-faults.json");
    for path in [bad, typo, missing] {
        let out = env
            .run_with(
                &["agent", "list"],
                &[("FAKE_HERDR_FAULTS", &path.display().to_string())],
            )
            .unwrap();
        assert_eq!(out.code, Some(2), "{path:?}: {out:?}");
        assert_eq!(
            out.error_code().as_deref(),
            Some("fake_misconfigured"),
            "{path:?}: {out:?}"
        );
    }
}

#[test]
fn an_empty_prompt_is_refused_before_the_agent_is_looked_up() {
    let env = FakeHerdrEnv::new().unwrap();
    let out = env.run(&["agent", "prompt", "nobody", ""]).unwrap();
    assert_eq!(out.error_code().as_deref(), Some("empty_agent_prompt"));
}

#[test]
fn agent_not_found_fault_answers_prompt_and_get_on_a_live_agent() {
    let mut env = FakeHerdrEnv::new().unwrap();
    let pane_id = pane(&env, "/tmp");
    assert_eq!(start(&env, "tw_demo_api", &pane_id, "s").code, Some(0));
    env.set_faults(&HerdrFaults::new().agent_not_found("agent get", "tw_demo_api"))
        .unwrap();
    let out = env.run(&["agent", "get", "tw_demo_api"]).unwrap();
    assert_eq!(out.code, Some(1));
    assert_eq!(out.error_code().as_deref(), Some("agent_not_found"));
    assert_eq!(
        env.calls().last().unwrap().fault.as_deref(),
        Some("error:agent_not_found")
    );
    // The agent is still there: another command and another target are untouched.
    assert_eq!(env.run(&["agent", "list"]).unwrap().code, Some(0));
    assert_eq!(env.run(&["agent", "get", &pane_id]).unwrap().code, Some(0));
}

/// A program for `FAKE_CLAUDE_BIN` that never reports ready, so `agent start`
/// waits for it until its own limit.
fn slow_claude(env: &FakeHerdrEnv) -> String {
    use std::os::unix::fs::PermissionsExt;
    let path = env
        .write_file("slow-claude", "#!/bin/sh\nexec sleep 60\n")
        .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.display().to_string()
}

#[test]
fn agent_list_answers_while_an_agent_start_waits_for_readiness() {
    let env = FakeHerdrEnv::new().unwrap();
    let pane_id = pane(&env, "/tmp");
    let slow = slow_claude(&env);
    std::thread::scope(|scope| {
        let starting = scope.spawn(|| {
            env.run_with(
                &[
                    "agent", "start", "slow", "--kind", "claude", "--pane", &pane_id,
                ],
                &[
                    ("FAKE_CLAUDE_BIN", &slow),
                    ("FAKE_HERDR_START_WAIT_MS", "6000"),
                ],
            )
            .unwrap()
        });
        // The slow agent is in the state file once the pane lists it.
        let began = std::time::Instant::now();
        loop {
            let out = env.run(&["agent", "list"]).unwrap();
            assert_eq!(out.code, Some(0), "{out:?}");
            let listed = json(&out)["agents"]
                .as_array()
                .is_some_and(|a| !a.is_empty());
            if listed {
                break;
            }
            assert!(began.elapsed().as_secs() < 5, "the agent was never listed");
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(
            began.elapsed().as_secs() < 5,
            "agent list waited for agent start"
        );
        let started = starting.join().unwrap();
        assert_eq!(
            started.error_code().as_deref(),
            Some("timeout"),
            "{started:?}"
        );
    });
}

#[test]
fn a_killed_agent_start_leaves_no_stuck_lock() {
    let env = FakeHerdrEnv::new().unwrap();
    let pane_id = pane(&env, "/tmp");
    let slow = slow_claude(&env);
    let mut child = std::process::Command::new(env.root().join("bin").join("herdr"))
        .args([
            "agent", "start", "slow", "--kind", "claude", "--pane", &pane_id,
        ])
        .env_clear()
        .envs(env.env())
        .env("FAKE_CLAUDE_BIN", &slow)
        .env("FAKE_HERDR_START_WAIT_MS", "20000")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));
    child.kill().unwrap();
    child.wait().unwrap();
    let began = std::time::Instant::now();
    let out = env.run(&["agent", "list"]).unwrap();
    assert_eq!(out.code, Some(0), "{out:?}");
    assert!(began.elapsed().as_secs() < 3, "agent list waited on a lock");
}

#[test]
fn a_call_waits_for_a_held_state_lock_and_runs_when_it_is_released() {
    let env = FakeHerdrEnv::new().unwrap();
    let lock_path = env.state_path().with_extension("lock");
    let held = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .unwrap();
    held.lock().unwrap();
    std::thread::scope(|scope| {
        let call = scope.spawn(|| env.run(&["agent", "list"]).unwrap());
        std::thread::sleep(std::time::Duration::from_millis(500));
        assert!(!call.is_finished(), "the call ignored the held lock");
        drop(held);
        assert_eq!(call.join().unwrap().code, Some(0));
    });
}

/// Puts a `sleep` the test owns in the pane as its agent, with a status file
/// that says the agent exited, as a pid the system has reused would look.
fn pane_with_an_exited_agent_on_a_sleep(env: &FakeHerdrEnv) -> (String, std::process::Child) {
    let pane_id = pane(env, "/tmp");
    let sleeper = std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .unwrap();
    let mut state: Value =
        serde_json::from_str(&std::fs::read_to_string(env.state_path()).unwrap()).unwrap();
    state["panes"][0]["agent"] = serde_json::json!({
        "name": "tw_demo_api", "kind": "claude", "pid": sleeper.id(),
        "session_hint": null, "fixed": false,
    });
    std::fs::write(env.state_path(), state.to_string()).unwrap();
    let dir = env
        .root()
        .join("fake-herdr-panes")
        .join(pane_id.replace(':', "_"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("status.json"), r#"{"status":"exited","seq":3}"#).unwrap();
    (pane_id, sleeper)
}

/// True when the child still runs. `kill -0` would say yes for a child that
/// was killed and not yet waited for.
fn is_running(child: &mut std::process::Child) -> bool {
    std::thread::sleep(std::time::Duration::from_millis(300));
    child.try_wait().unwrap().is_none()
}

#[test]
fn dropping_the_world_leaves_a_process_alone_when_its_pane_says_exited() {
    let env = FakeHerdrEnv::new().unwrap();
    let (_, mut sleeper) = pane_with_an_exited_agent_on_a_sleep(&env);
    drop(env);
    let alive = is_running(&mut sleeper);
    sleeper.kill().unwrap();
    sleeper.wait().unwrap();
    assert!(alive, "the drop signalled a pid whose agent had exited");
}

#[test]
fn pane_close_leaves_a_process_alone_when_its_pane_says_exited() {
    let env = FakeHerdrEnv::new().unwrap();
    let (pane_id, mut sleeper) = pane_with_an_exited_agent_on_a_sleep(&env);
    let out = env.run(&["pane", "close", &pane_id]).unwrap();
    let alive = is_running(&mut sleeper);
    sleeper.kill().unwrap();
    sleeper.wait().unwrap();
    assert_eq!(out.code, Some(0), "{out:?}");
    assert!(alive, "pane close signalled a pid whose agent had exited");
}
