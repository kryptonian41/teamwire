//! `fake-claude` started by `fake-herdr` (so with a cleared environment), and
//! started directly: its session file, its settings `env`, its Stop hook and
//! its faults.

#![expect(
    clippy::unwrap_used,
    clippy::panic,
    clippy::disallowed_methods,
    reason = "test helpers build fixtures from literals and fail the test with a message; one test starts fake-claude directly"
)]

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use teamwire_fakes::{ClaudeFault, ClaudeScript, FakeBins, FakeHerdrEnv, Startup, Step};

/// Logs each Stop hook's input line, and the role and token the hook saw.
const LOGGING_HOOK: &str = r#"cat >> "$TMPDIR/hook-inputs.jsonl"; printf '\n' >> "$TMPDIR/hook-inputs.jsonl"; printf '%s/%s\n' "$TEAMWIRE_ROLE" "$TEAMWIRE_TOKEN" >> "$TMPDIR/hook-env.txt""#;

fn settings_with(env: &FakeHerdrEnv, hook: &str, vars: &Value) -> String {
    let doc = json!({
        "env": vars,
        "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": hook }] }] },
    });
    env.write_file("member-settings.json", &doc.to_string())
        .unwrap()
        .display()
        .to_string()
}

fn identity() -> Value {
    json!({ "TEAMWIRE_ROLE": "api", "TEAMWIRE_TOKEN": "tok-from-settings" })
}

fn pane(env: &FakeHerdrEnv) -> String {
    let out = env
        .run(&["workspace", "create", "--cwd", "/tmp", "--no-focus"])
        .unwrap();
    out.result()
        .unwrap()
        .pointer("/root_pane/pane_id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

fn start(env: &FakeHerdrEnv, settings: &str, session: &str) -> String {
    let pane_id = pane(env);
    let out = env
        .run(&[
            "agent",
            "start",
            "tw_demo_api",
            "--kind",
            "claude",
            "--pane",
            &pane_id,
            "--timeout",
            "20000",
            "--",
            "-n",
            "tw_demo_api",
            "--session-id",
            session,
            "--settings",
            settings,
        ])
        .unwrap();
    assert_eq!(out.code, Some(0), "{out:?}");
    pane_id
}

fn prompt(env: &FakeHerdrEnv, text: &str) {
    let out = env
        .run(&[
            "agent",
            "prompt",
            "tw_demo_api",
            text,
            "--wait",
            "--timeout",
            "20000",
        ])
        .unwrap();
    assert_eq!(out.code, Some(0), "{out:?}");
}

fn tmp(env: &FakeHerdrEnv, name: &str) -> std::path::PathBuf {
    env.root().join("tmp").join(name)
}

fn wait_for_file(path: &Path) -> String {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if let Ok(text) = std::fs::read_to_string(path)
            && !text.is_empty()
        {
            return text;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("{} never appeared", path.display());
}

fn hook_inputs(env: &FakeHerdrEnv) -> Vec<Value> {
    std::fs::read_to_string(tmp(env, "hook-inputs.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn script(turn: Vec<Step>) -> ClaudeScript {
    ClaudeScript {
        turns: vec![turn],
        ..ClaudeScript::default()
    }
}

#[test]
fn settings_env_reaches_shell_steps_and_hooks_although_fake_herdr_cleared_the_environment() {
    let mut env = FakeHerdrEnv::new().unwrap();
    env.set_script(&script(vec![Step::Run(
        r#"printf '%s/%s' "$TEAMWIRE_ROLE" "$TEAMWIRE_TOKEN" > "$TMPDIR/step-env.txt""#.into(),
    )]))
    .unwrap();
    let settings = settings_with(&env, LOGGING_HOOK, &identity());
    // fake-herdr itself is started with the variables in its environment.
    let pane_id = pane(&env);
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
            &[("TEAMWIRE_ROLE", "wrong"), ("TEAMWIRE_TOKEN", "leaked")],
        )
        .unwrap();
    assert_eq!(started.code, Some(0), "{started:?}");
    prompt(&env, "do the step");

    assert_eq!(
        std::fs::read_to_string(tmp(&env, "step-env.txt")).unwrap(),
        "api/tok-from-settings"
    );
    assert_eq!(
        wait_for_file(&tmp(&env, "hook-env.txt")).trim(),
        "api/tok-from-settings"
    );
}

#[test]
fn the_stop_hook_gets_the_json_claude_code_sends() {
    let mut env = FakeHerdrEnv::new().unwrap();
    env.set_script(&ClaudeScript {
        turns: vec![vec![Step::Print("thinking".into())]],
        final_message: Some("All done. See the report.".into()),
        ..ClaudeScript::default()
    })
    .unwrap();
    let settings = settings_with(&env, LOGGING_HOOK, &identity());
    start(&env, &settings, "sess-hook");
    prompt(&env, "go");
    wait_for_file(&tmp(&env, "hook-env.txt"));
    let inputs = hook_inputs(&env);
    assert_eq!(inputs.len(), 1);
    let input = inputs.first().unwrap();
    assert_eq!(input.get("session_id").unwrap(), "sess-hook");
    assert_eq!(input.get("stop_hook_active").unwrap(), false);
    assert_eq!(input.get("hook_event_name").unwrap(), "Stop");
    assert_eq!(
        input.get("last_assistant_message").unwrap(),
        "All done. See the report."
    );
    let transcript = input.get("transcript_path").unwrap().as_str().unwrap();
    assert!(transcript.ends_with("sess-hook.jsonl"), "{transcript}");
    assert!(input.get("agent_id").is_none());
    assert!(Path::new(transcript).is_file());
}

#[test]
fn the_session_file_is_written_and_removed_on_exit() {
    let env = FakeHerdrEnv::new().unwrap();
    let settings = settings_with(&env, LOGGING_HOOK, &identity());
    start(&env, &settings, "sess-file");
    let files = env.session_files();
    assert_eq!(files.len(), 1);
    let doc: Value =
        serde_json::from_str(&std::fs::read_to_string(files.first().unwrap()).unwrap()).unwrap();
    assert_eq!(doc.get("sessionId").unwrap(), "sess-file");
    assert_eq!(doc.get("name").unwrap(), "tw_demo_api");
    assert_eq!(doc.get("kind").unwrap(), "interactive");
    assert_eq!(doc.get("status").unwrap(), "idle");
    let pid = files
        .first()
        .unwrap()
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(doc.get("pid").unwrap().to_string(), pid);

    let out = env
        .run(&["agent", "prompt", "tw_demo_api", "/exit"])
        .unwrap();
    assert_eq!(out.code, Some(0), "{out:?}");
    let begun = Instant::now();
    while !env.session_files().is_empty() {
        assert!(
            begun.elapsed() < Duration::from_secs(10),
            "the session file was never removed"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn a_hook_that_holds_the_turn_runs_the_hold_steps_and_stops_again() {
    let mut env = FakeHerdrEnv::new().unwrap();
    env.set_script(&ClaudeScript {
        turns: vec![vec![]],
        on_hold: vec![Step::Run(r#"echo added > "$TMPDIR/hold-step""#.into())],
        ..ClaudeScript::default()
    })
    .unwrap();
    let hook = format!(
        r#"{LOGGING_HOOK}; if [ ! -f "$TMPDIR/held" ]; then touch "$TMPDIR/held"; echo '{{"decision":"block","reason":"add your progress line"}}'; fi"#
    );
    let settings = settings_with(&env, &hook, &identity());
    start(&env, &settings, "sess-hold");
    prompt(&env, "finish");
    let inputs = hook_inputs(&env);
    let active: Vec<bool> = inputs
        .iter()
        .map(|i| i.get("stop_hook_active").unwrap().as_bool().unwrap())
        .collect();
    assert_eq!(active, [false, true]);
    assert!(tmp(&env, "hold-step").is_file());
    let screen = env
        .run(&["agent", "read", "tw_demo_api", "--lines", "30"])
        .unwrap();
    assert!(
        screen
            .stdout
            .contains("Stop hook feedback: add your progress line")
    );
}

fn run_fault(fault: ClaudeFault) -> FakeHerdrEnv {
    let mut env = FakeHerdrEnv::new().unwrap();
    env.set_script(&ClaudeScript {
        turns: vec![vec![]],
        fault,
        ..ClaudeScript::default()
    })
    .unwrap();
    let settings = settings_with(&env, LOGGING_HOOK, &identity());
    start(&env, &settings, "sess-fault");
    env
}

#[test]
fn stop_twice_runs_the_hook_twice_for_one_turn() {
    let env = run_fault(ClaudeFault::StopTwice);
    prompt(&env, "go");
    assert_eq!(hook_inputs(&env).len(), 2);
}

#[test]
fn a_wrong_session_id_reaches_the_hook() {
    let env = run_fault(ClaudeFault::WrongSession);
    prompt(&env, "go");
    let inputs = hook_inputs(&env);
    assert_eq!(
        inputs.first().unwrap().get("session_id").unwrap(),
        "not-sess-fault"
    );
}

#[test]
fn exit_without_stop_never_runs_the_hook_and_ends_the_session() {
    let env = run_fault(ClaudeFault::ExitWithoutStop);
    let out = env.run(&["agent", "prompt", "tw_demo_api", "go"]).unwrap();
    assert_eq!(out.code, Some(0), "{out:?}");
    let begun = Instant::now();
    while !env.session_files().is_empty() {
        assert!(
            begun.elapsed() < Duration::from_secs(10),
            "the session never ended"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(hook_inputs(&env).is_empty());
}

#[test]
fn never_stop_leaves_the_agent_working_with_no_hook() {
    let env = run_fault(ClaudeFault::NeverStop);
    let out = env.run(&["agent", "prompt", "tw_demo_api", "go"]).unwrap();
    assert_eq!(out.code, Some(0), "{out:?}");
    std::thread::sleep(Duration::from_millis(300));
    let get = env.run(&["agent", "get", "tw_demo_api"]).unwrap();
    let status = get
        .result()
        .unwrap()
        .pointer("/agent/agent_status")
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(status, "working");
    assert!(hook_inputs(&env).is_empty());
}

#[test]
fn a_harness_without_a_turn_end_hook_never_runs_it() {
    let mut env = FakeHerdrEnv::new().unwrap();
    env.set_script(&ClaudeScript {
        turns: vec![vec![]],
        no_hook: true,
        ..ClaudeScript::default()
    })
    .unwrap();
    let settings = settings_with(&env, LOGGING_HOOK, &identity());
    start(&env, &settings, "sess-nohook");
    prompt(&env, "go");
    assert!(hook_inputs(&env).is_empty());
}

#[test]
fn the_plain_trust_dialog_is_answered_with_down_and_enter() {
    let mut env = FakeHerdrEnv::new().unwrap();
    env.set_script(&ClaudeScript {
        startup: Startup::Trust,
        ..ClaudeScript::default()
    })
    .unwrap();
    let settings = settings_with(&env, LOGGING_HOOK, &identity());
    let pane_id = pane(&env);
    let out = env
        .run(&[
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
        ])
        .unwrap();
    assert_eq!(out.error_code().as_deref(), Some("agent_not_ready"));
    let screen = env
        .run(&["agent", "read", "tw_demo_api", "--lines", "20"])
        .unwrap();
    let text = screen.stdout;
    assert!(text.contains(teamwire_fakes::PLAIN_TRUST_TEXT));
    assert!(text.contains(teamwire_fakes::TRUST_OPTION_TEXT));
    assert!(!text.contains("pre-approves"));
    env.run(&["agent", "send-keys", "tw_demo_api", "down"])
        .unwrap();
    env.run(&["agent", "send-keys", "tw_demo_api", "enter"])
        .unwrap();
    let begun = Instant::now();
    loop {
        let get = env.run(&["agent", "get", "tw_demo_api"]).unwrap();
        if get.result().is_some_and(|r| {
            r.pointer("/agent/agent_status").and_then(Value::as_str) == Some("idle")
        }) {
            break;
        }
        assert!(
            begun.elapsed() < Duration::from_secs(10),
            "never became idle"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn run_direct(env_vars: &[(&str, &str)], stdin: &str) -> std::process::Output {
    use std::io::Write as _;
    let bins = FakeBins::ensure().unwrap();
    let mut child = std::process::Command::new(bins.claude())
        .env_clear()
        .envs(env_vars.iter().copied())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn fake_claude_refuses_to_run_without_a_directory_for_its_session_file() {
    let out = run_direct(
        &[("PATH", "/usr/bin:/bin"), ("HOME", "/nonexistent-home")],
        "hello\n",
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("never uses ~/.claude"));
    assert!(!Path::new("/nonexistent-home/.claude").exists());
}

#[test]
fn fake_claude_reads_prompts_from_stdin_when_not_in_a_pane() {
    let dir = tempfile::tempdir().unwrap();
    let claude_dir = dir.path().display().to_string();
    let out = run_direct(
        &[
            ("PATH", "/usr/bin:/bin"),
            ("TEAMWIRE_CLAUDE_DIR", &claude_dir),
        ],
        "first\nsecond\n",
    );
    assert_eq!(out.status.code(), Some(0));
    let screen = String::from_utf8_lossy(&out.stdout);
    assert!(
        screen.contains("> first") && screen.contains("> second"),
        "{screen}"
    );
    assert!(
        std::fs::read_dir(dir.path().join("sessions"))
            .unwrap()
            .next()
            .is_none()
    );
}

fn blocked_at_dialog(startup: Startup) -> FakeHerdrEnv {
    let mut env = FakeHerdrEnv::new().unwrap();
    env.set_script(&ClaudeScript {
        startup,
        ..ClaudeScript::default()
    })
    .unwrap();
    let settings = settings_with(&env, LOGGING_HOOK, &identity());
    let pane_id = pane(&env);
    env.run(&[
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
    ])
    .unwrap();
    env
}

fn agent_status(env: &FakeHerdrEnv) -> Option<String> {
    let get = env.run(&["agent", "get", "tw_demo_api"]).unwrap();
    get.result()?
        .pointer("/agent/agent_status")?
        .as_str()
        .map(str::to_owned)
}

#[test]
fn the_digit_one_does_not_trust_the_folder() {
    let env = blocked_at_dialog(Startup::Trust);
    env.run(&["agent", "send-keys", "tw_demo_api", "1"])
        .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    // Still waiting at the dialog: a digit is not an answer.
    assert_eq!(agent_status(&env).as_deref(), Some("blocked"));
    // And Enter now takes the preselected "No, exit".
    env.run(&["agent", "send-keys", "tw_demo_api", "enter"])
        .unwrap();
    let begun = Instant::now();
    while !env.session_files().is_empty() {
        assert!(
            begun.elapsed() < Duration::from_secs(10),
            "the session never ended"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn enter_alone_takes_no_exit_and_never_trusts() {
    let env = blocked_at_dialog(Startup::Trust);
    env.run(&["agent", "send-keys", "tw_demo_api", "enter"])
        .unwrap();
    let begun = Instant::now();
    while !env.session_files().is_empty() {
        assert!(
            begun.elapsed() < Duration::from_secs(10),
            "the session never ended"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    assert_ne!(agent_status(&env).as_deref(), Some("idle"));
}

#[test]
fn the_escalated_dialog_prints_every_escalation_marker_and_the_plain_ones() {
    let env = blocked_at_dialog(Startup::EscalatedTrust);
    let screen = env
        .run(&["agent", "read", "tw_demo_api", "--lines", "30"])
        .unwrap();
    let text = screen.stdout;
    for marker in teamwire_fakes::TRUST_ESCALATION_MARKERS {
        assert!(text.contains(marker), "{marker} missing from {text}");
    }
    for marker in teamwire_fakes::TRUST_MARKERS {
        assert!(text.contains(marker), "{marker} missing from {text}");
    }
}

fn session_doc(env: &FakeHerdrEnv) -> Value {
    let file = env.session_files().pop().unwrap();
    serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap()
}

#[test]
fn session_file_status_values_are_the_ones_claude_code_writes() {
    let mut env = FakeHerdrEnv::new().unwrap();
    env.set_script(&script(vec![Step::SleepMs(1500)])).unwrap();
    let settings = settings_with(&env, LOGGING_HOOK, &identity());
    start(&env, &settings, "sess-status");
    assert_eq!(session_doc(&env).get("status").unwrap(), "idle");
    env.run(&["agent", "prompt", "tw_demo_api", "work"])
        .unwrap();
    let begun = Instant::now();
    while session_doc(&env).get("status").unwrap() != "busy" {
        assert!(begun.elapsed() < Duration::from_secs(5), "never busy");
        std::thread::sleep(Duration::from_millis(20));
    }
    while session_doc(&env).get("status").unwrap() != "idle" {
        assert!(
            begun.elapsed() < Duration::from_secs(10),
            "never idle again"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_dialog_shows_as_waiting_with_waiting_for() {
    let env = blocked_at_dialog(Startup::Trust);
    let doc = session_doc(&env);
    assert_eq!(doc.get("status").unwrap(), "waiting");
    assert_eq!(doc.get("waitingFor").unwrap(), "input needed");
}

#[test]
fn the_stop_hook_input_has_prompt_id_and_background_tasks() {
    let mut env = FakeHerdrEnv::new().unwrap();
    env.set_script(&ClaudeScript {
        turns: vec![vec![Step::BackgroundTasks(vec!["build".into()])], vec![]],
        ..ClaudeScript::default()
    })
    .unwrap();
    let settings = settings_with(&env, LOGGING_HOOK, &identity());
    start(&env, &settings, "sess-bg");
    prompt(&env, "first");
    prompt(&env, "second");
    let inputs = hook_inputs(&env);
    assert_eq!(inputs.len(), 2);
    let first = inputs.first().unwrap();
    assert_eq!(first.get("prompt_id").unwrap(), "prompt-0");
    let tasks = first.get("background_tasks").unwrap().as_array().unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks.first().unwrap().get("id").unwrap(), "build");
    let second = inputs.last().unwrap();
    assert_eq!(second.get("prompt_id").unwrap(), "prompt-1");
    assert!(
        second
            .get("background_tasks")
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn additional_context_from_a_stop_hook_holds_the_turn_like_a_block() {
    let mut env = FakeHerdrEnv::new().unwrap();
    env.set_script(&ClaudeScript {
        turns: vec![vec![]],
        ..ClaudeScript::default()
    })
    .unwrap();
    let hook = format!(
        r#"{LOGGING_HOOK}; if [ ! -f "$TMPDIR/held" ]; then touch "$TMPDIR/held"; echo '{{"hookSpecificOutput":{{"hookEventName":"Stop","additionalContext":"read the note"}}}}'; fi"#
    );
    let settings = settings_with(&env, &hook, &identity());
    start(&env, &settings, "sess-ctx");
    prompt(&env, "finish");
    let active: Vec<bool> = hook_inputs(&env)
        .iter()
        .map(|i| i.get("stop_hook_active").unwrap().as_bool().unwrap())
        .collect();
    assert_eq!(active, [false, true]);
}

#[test]
fn a_hook_that_always_holds_runs_eight_times_and_then_the_turn_ends() {
    let mut env = FakeHerdrEnv::new().unwrap();
    env.set_script(&ClaudeScript {
        turns: vec![vec![]],
        ..ClaudeScript::default()
    })
    .unwrap();
    let hook = format!(r#"{LOGGING_HOOK}; echo '{{"decision":"block","reason":"again"}}'"#);
    let settings = settings_with(&env, &hook, &identity());
    start(&env, &settings, "sess-eight");
    prompt(&env, "finish");
    assert_eq!(hook_inputs(&env).len(), 8);
}

#[test]
fn agents_json_lists_both_row_shapes_and_starts_no_session() {
    let mut env = FakeHerdrEnv::new().unwrap();
    env.set_script(&ClaudeScript::default()).unwrap();
    let settings = settings_with(&env, LOGGING_HOOK, &identity());
    start(&env, &settings, "sess-list");
    env.add_session_file(
        4242,
        &json!({"pid": 4242, "sessionId": "bg-1", "cwd": "/tmp", "startedAt": 1, "kind": "bg",
                "name": "background job", "status": "blocked", "jobId": "job1"}),
    )
    .unwrap();
    let before = env.session_files().len();
    let out = env.run_claude(&["agents", "--json"]).unwrap();
    assert_eq!(out.code, Some(0), "{out:?}");
    let rows: Vec<Value> = serde_json::from_str(out.stdout.trim()).unwrap();
    assert_eq!(rows.len(), 2);
    let interactive = rows
        .iter()
        .find(|r| r.get("sessionId").unwrap() == "sess-list")
        .unwrap();
    assert_eq!(interactive.get("status").unwrap(), "idle");
    assert!(interactive.get("state").is_none());
    let background = rows
        .iter()
        .find(|r| r.get("sessionId").unwrap() == "bg-1")
        .unwrap();
    assert_eq!(background.get("state").unwrap(), "blocked");
    assert!(background.get("status").is_none());
    assert_eq!(background.get("id").unwrap(), "job1");
    assert_eq!(
        env.session_files().len(),
        before,
        "listing must start no session"
    );
}

#[test]
fn the_agent_gets_its_panes_herdr_variables() {
    let mut env = FakeHerdrEnv::new().unwrap();
    env.set_script(&script(vec![Step::Run(
        r#"printf '%s|%s' "$HERDR_ENV" "$HERDR_PANE_ID" > "$TMPDIR/herdr-env.txt""#.into(),
    )]))
    .unwrap();
    let settings = settings_with(&env, LOGGING_HOOK, &identity());
    let pane_id = pane(&env);
    let started = env
        .run(&[
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
        ])
        .unwrap();
    assert_eq!(started.code, Some(0), "{started:?}");
    prompt(&env, "do the step");
    assert_eq!(
        wait_for_file(&tmp(&env, "herdr-env.txt")),
        format!("1|{pane_id}")
    );
}

#[test]
fn dropping_the_env_ends_a_step_shell_that_never_finishes() {
    let pid_file;
    {
        let mut env = FakeHerdrEnv::new().unwrap();
        env.set_script(&script(vec![Step::Run(
            r#"echo $$ > "$TMPDIR/step.pid"; while true; do sleep 0.1; done"#.into(),
        )]))
        .unwrap();
        let settings = settings_with(&env, LOGGING_HOOK, &identity());
        let pane_id = pane(&env);
        let started = env
            .run(&[
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
            ])
            .unwrap();
        assert_eq!(started.code, Some(0), "{started:?}");
        // The step never finishes, so the prompt is sent without --wait.
        let sent = env
            .run(&["agent", "prompt", "tw_demo_api", "loop"])
            .unwrap();
        assert_eq!(sent.code, Some(0), "{sent:?}");
        pid_file = tmp(&env, "step.pid");
        wait_for_file(&pid_file);
        let pid = std::fs::read_to_string(&pid_file).unwrap();
        drop(env);
        let pid = pid.trim().to_owned();
        let mut gone = false;
        for _ in 0..50 {
            let alive = std::process::Command::new("kill")
                .args(["-0", &pid])
                .status()
                .is_ok_and(|s| s.success());
            if !alive {
                gone = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(gone, "the step shell {pid} outlived the fake environment");
    }
}

/// Releasing the owner lock stands in for the test process dying by a
/// signal: the kernel releases its lock the same way, and no `Drop` runs.
/// Both an agent waiting for a prompt and one held by `never_stop` end.
#[test]
fn agents_end_once_the_owner_lock_is_released() {
    let mut env = FakeHerdrEnv::new().unwrap();
    let (_, own_lock) = env
        .env()
        .into_iter()
        .find(|(k, _)| k == "FAKE_OWNER_LOCK")
        .unwrap();
    let held = std::fs::File::open(&own_lock).unwrap().try_lock_shared();
    assert!(
        matches!(held, Err(std::fs::TryLockError::WouldBlock)),
        "the env does not hold its own lock: {held:?}"
    );

    env.set_script(&ClaudeScript {
        turns: vec![vec![]],
        fault: ClaudeFault::NeverStop,
        ..ClaudeScript::default()
    })
    .unwrap();
    let lock_path = env.root().join("test-owner.lock");
    let lock = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .unwrap();
    lock.lock().unwrap();
    let lock_var = lock_path.display().to_string();
    let settings = settings_with(&env, LOGGING_HOOK, &identity());
    for name in ["tw_demo_api", "tw_demo_web"] {
        let pane_id = pane(&env);
        let started = env
            .run_with(
                &[
                    "agent",
                    "start",
                    name,
                    "--kind",
                    "claude",
                    "--pane",
                    &pane_id,
                    "--timeout",
                    "20000",
                    "--",
                    "-n",
                    name,
                    "--settings",
                    &settings,
                ],
                &[("FAKE_OWNER_LOCK", &lock_var)],
            )
            .unwrap();
        assert_eq!(started.code, Some(0), "{started:?}");
    }
    let sent = env.run(&["agent", "prompt", "tw_demo_api", "go"]).unwrap();
    assert_eq!(sent.code, Some(0), "{sent:?}");

    // Longer than one owner check, so a check that misreads a held lock
    // would already have ended them.
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(
        env.session_files().len(),
        2,
        "an agent ended while its owner lived"
    );

    drop(lock);
    let begun = Instant::now();
    while !env.session_files().is_empty() {
        assert!(
            begun.elapsed() < Duration::from_secs(10),
            "agents outlived their owner: {:?}",
            env.session_files()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}
