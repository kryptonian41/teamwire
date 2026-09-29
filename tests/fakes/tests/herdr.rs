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
    assert_eq!(s(&json(&closed), "/pane_id"), pane_id);
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
    let text = s(&json(&read), "/text");
    assert!(text.contains("> hello agent"), "{text}");
    assert!(text.contains("worked on it"), "{text}");

    let get = env.run(&["agent", "get", &pane_id]).unwrap();
    assert_eq!(s(&json(&get), "/agent/agent_status"), "idle");
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
                ("HERDR_PANE_ID", "w1:p1"),
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
        "HERDR_PANE_ID",
        "TEAMWIRE_CLAUDE_DIR",
    ] {
        assert!(!dump.contains(absent), "{absent} leaked into {dump}");
    }
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
    let screen = s(
        &json(
            &env.run(&["agent", "read", "tw_demo_api", "--lines", "20"])
                .unwrap(),
        ),
        "/text",
    );
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
fn a_stalled_prompt_fails_but_the_text_was_typed() {
    let mut env = FakeHerdrEnv::new().unwrap();
    let pane_id = pane(&env, "/tmp");
    assert_eq!(start(&env, "tw_demo_api", &pane_id, "s").code, Some(0));
    env.set_faults(&HerdrFaults::new().stalled_prompt(Some("tw_demo_api"), Some(1)))
        .unwrap();
    let out = env
        .run(&["agent", "prompt", "tw_demo_api", "did this arrive"])
        .unwrap();
    assert_eq!(out.error_code().as_deref(), Some("agent_prompt_stalled"));
    for _ in 0..200 {
        let text = s(
            &json(&env.run(&["agent", "read", "tw_demo_api"]).unwrap()),
            "/text",
        );
        if text.contains("> did this arrive") {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("the stalled prompt was not typed into the agent");
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
    assert_eq!(no_pane.error_code().as_deref(), Some("pane_not_found"));
    let codex = env
        .run(&["agent", "start", "x", "--kind", "codex", "--pane", &pane_id])
        .unwrap();
    assert_eq!(
        codex.error_code().as_deref(),
        Some("agent_kind_unsupported")
    );
    let unknown = env.run(&["frobnicate", "now"]).unwrap();
    assert_eq!(unknown.error_code().as_deref(), Some("unknown_command"));
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
        Some("agent_name_in_use")
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
