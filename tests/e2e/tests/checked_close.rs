//! ADR 0057, through the real binary: a member blocked in generation 1, a
//! reprompt the lead sends to a word that is no role, then the close. A
//! bare close refuses, so a refused reprompt is never archived as a
//! finished generation; `--expect` checks the result the lead needs, and a
//! close whose expectation holds runs.
#![expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
)]

use std::time::Duration;

use serde_json::json;
use teamwire_core::TeamState;
use teamwire_e2e::{World, sh};
use teamwire_fakes::{ClaudeScript, Step};

#[test]
fn a_bare_close_after_a_refused_reprompt_refuses_and_a_checked_close_decides() {
    let step = format!(
        "{}teamwire done --status blocked --summary \"blocked on a missing key\"; echo done-exit=$?\n",
        sh::report(false)
    );
    let w = World::new(&ClaudeScript {
        turns: vec![vec![Step::Run(step)]],
        ..ClaudeScript::default()
    });
    let spec = json!({"team": "demo", "title": "Demo", "children": [
        {"role": "short", "title": "Short", "kind": "investigator", "mode": "pane",
         "cwd": w.repo(), "task": "look at it"}]});
    let launch = w.launch(&spec, &[]);
    assert_eq!(launch.code, 0, "{}", launch.all());
    w.wait_started("demo", &["short"]);
    w.wait_lead_screen(
        "[teamwire:demo] short finished (blocked)",
        Duration::from_secs(30),
    );

    // The lead reprompts a word that is no role: refused, nothing sent.
    let send = w.teamwire(
        &[
            "send",
            "demo",
            "--from",
            "lead",
            "--to",
            "members",
            "--type",
            "reprompt",
            "--text",
            "go on and finish generation 2",
        ],
        None,
    );
    assert_ne!(send.code, 0, "{}", send.all());

    let closing = |args: &[&str]| {
        let mut all = vec!["close", "demo"];
        all.extend_from_slice(args);
        w.teamwire(&all, None)
    };
    let open = || w.store("demo").read(|tx| tx.team()).unwrap().unwrap().state;

    // A bare close and its dry run refuse, and change nothing.
    for args in [&[][..], &["--dry-run"][..]] {
        let run = closing(args);
        assert_eq!(run.code, 1, "{args:?}: {}", run.all());
        assert!(
            run.err.contains("the lead's newest send was refused"),
            "{args:?}: {}",
            run.all()
        );
        assert_eq!(open(), TeamState::Open);
        assert!(!w.home.path().join("archive").exists());
    }

    // The blocked generation 1 is not the generation 2 the lead asked for.
    let run = closing(&["--expect", "short:2:done"]);
    assert_eq!(run.code, 1, "{}", run.all());
    assert!(
        run.err.contains("short is in generation 1, not 2"),
        "{}",
        run.all()
    );
    let run = closing(&["--expect", "short:1:done", "--dry-run"]);
    assert_eq!(run.code, 1, "{}", run.all());
    assert!(
        run.err
            .contains("short's result in generation 1 is blocked, not done"),
        "{}",
        run.all()
    );
    // A word that is no result is a usage error.
    let run = closing(&["--expect", "short:1:skipped"]);
    assert_eq!(run.code, 3, "{}", run.all());
    assert_eq!(open(), TeamState::Open);

    // The dry run of an intentional close names the reason.
    let run = closing(&["--abandon", "the user stopped the work", "--dry-run"]);
    assert_eq!(run.code, 0, "{}", run.all());
    assert!(
        run.out
            .contains("[dry-run] abandon: closing as it is (--abandon): the user stopped the work"),
        "{}",
        run.all()
    );

    // The lead that wants the blocked result says so, and the close runs.
    let run = closing(&["--expect", "short:1:blocked"]);
    assert_eq!(run.code, 0, "{}", run.all());
    assert!(
        run.out
            .contains("expect: results as expected: short:1:blocked"),
        "{}",
        run.all()
    );
    assert!(!w.team_dir("demo").exists());
}

/// No send at all, as when a client checks a call itself and never reaches
/// teamwire: the store holds no refusal, and a bare close still refuses the
/// blocked member. An expectation about the finished member does not
/// accept the blocked one.
#[test]
fn with_no_send_history_a_blocked_member_needs_its_own_expectation() {
    let step = format!(
        "{}if [ \"$TEAMWIRE_ROLE\" = short ]; then\nteamwire done --status blocked --summary \"no key\"\nelse\n{}fi\n",
        sh::report(false),
        sh::done("web finished")
    );
    let w = World::new(&ClaudeScript {
        turns: vec![vec![Step::Run(step)]],
        ..ClaudeScript::default()
    });
    let spec = json!({"team": "demo", "title": "Demo", "children": [
        {"role": "short", "title": "Short", "kind": "investigator", "mode": "pane",
         "cwd": w.repo(), "task": "look at it"},
        {"role": "web", "title": "Web", "kind": "investigator", "mode": "pane",
         "cwd": w.repo(), "task": "look at web"}]});
    let launch = w.launch(&spec, &[]);
    assert_eq!(launch.code, 0, "{}", launch.all());
    w.wait_lead_screen(
        "[teamwire:demo] short finished (blocked)",
        Duration::from_secs(30),
    );
    w.wait_lead_screen(
        "[teamwire:demo] web finished (done)",
        Duration::from_secs(30),
    );

    for args in [
        &["close", "demo"][..],
        &["close", "demo", "--dry-run"][..],
        &["close", "demo", "--expect", "web:1:done"][..],
    ] {
        let run = w.teamwire(args, None);
        assert_eq!(run.code, 1, "{args:?}: {}", run.all());
        assert!(
            run.err
                .contains("short's current result is blocked (generation 1)"),
            "{args:?}: {}",
            run.all()
        );
        assert!(
            !run.err.contains("newest send was refused"),
            "{}",
            run.all()
        );
        assert!(!w.home.path().join("archive").exists());
    }
    let run = w.teamwire(&["close", "demo", "--expect", "short:1:blocked"], None);
    assert_eq!(run.code, 0, "{}", run.all());
    assert!(!w.team_dir("demo").exists());
}
