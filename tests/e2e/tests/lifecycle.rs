//! The whole lifecycle through the real binary (`docs/test-strategy.md`
//! section 5, first scenario): three members, one message each, a commit
//! each, `done`, the watch, verify and close.

use std::time::Duration;

use serde_json::{Value, json};
use teamwire_e2e::World;
use teamwire_fakes::{ClaudeScript, Step};

/// What every member does on its first turn. It waits for the `go` file so
/// that no message reaches a member that has not started yet (a message to
/// such a member is correctly `undelivered`). Each message is longer than the
/// typed line's excerpt, so it stays unread until the hold reads the inbox.
const WORK: &str = r#"
while [ ! -f "$TEAMWIRE_HOME/go" ]; do sleep 0.1; done
R=$TEAMWIRE_ROLE
PAD=$(printf 'and more %.0s' $(seq 40))
case $R in
  a) teamwire send --to inv --type question --text "a to inv $PAD" ;;
  b) teamwire send --to lead --type question --text "b to lead $PAD" ;;
  inv) teamwire send --to a --type finding --text "inv to a $PAD" ;;
esac
sha=none
if [ "$R" != inv ]; then
  echo "$R" > "$R.txt"; git add -A
  git -c user.name=t -c user.email=t@example.com commit -q -m "work of $R"
  sha=$(git rev-parse --short=7 HEAD)
fi
mkdir -p "$TEAMWIRE_HOME/teams/$TEAMWIRE_TEAM/reports"
printf '# report\n\n- %s work of %s | left: nothing\n\nDecisions I made: none\n' "$sha" "$R" \
  > "$TEAMWIRE_HOME/teams/$TEAMWIRE_TEAM/reports/$R.md"
teamwire done --status done --summary "$R finished"
"#;

fn script() -> ClaudeScript {
    ClaudeScript {
        // The first prompt is the launch task; a later prompt is a delivered
        // message, and a hold for it reads the inbox.
        turns: vec![vec![Step::Run(WORK.to_owned())], vec![]],
        on_hold: vec![Step::Run("teamwire inbox".to_owned())],
        ..ClaudeScript::default()
    }
}

fn spec(w: &World) -> Value {
    json!({"team": "demo", "title": "Demo team", "children": [
        {"role": "a", "title": "Alpha", "kind": "implementer", "mode": "worktree",
         "repo": w.repo(), "task": "build a", "check": "true", "owns": ["a.txt"]},
        {"role": "b", "title": "Beta", "kind": "implementer", "mode": "worktree",
         "repo": w.repo(), "task": "build b", "check": "true", "owns": ["b.txt"]},
        {"role": "inv", "title": "Investigator", "kind": "investigator", "mode": "pane",
         "cwd": w.repo(), "task": "look around"}]})
}

#[test]
fn three_members_run_the_whole_lifecycle() {
    let w = World::new(&script());
    let launch = w.launch(&spec(&w), &[]);
    assert_eq!(launch.code, 0, "{}", launch.all());
    w.wait_started("demo", &["a", "b", "inv"]);

    let mut watch = w.watch("demo", 1);
    watch.wait_for("WATCHING 3 members", Duration::from_secs(20));
    w.write(&w.home.path().join("go"), "");
    watch.wait_for("TEAM-COMPLETE 3/3 done", Duration::from_secs(60));
    assert_eq!(watch.exit_code(Duration::from_secs(10)), Some(0));
    let lines = watch.lines();
    assert_eq!(
        lines.len(),
        2,
        "a healthy team wakes the lead with exactly two watch lines: {lines:?}"
    );
    assert!(
        lines
            .first()
            .is_some_and(|l| l.contains("WATCHING 3 members")),
        "{lines:?}"
    );
    assert!(
        lines
            .get(1)
            .is_some_and(|l| l.contains("TEAM-COMPLETE 3/3 done")),
        "{lines:?}"
    );

    // Route A: the Stop hook of each member typed its result into the
    // lead's pane, one line per result.
    let screen = w.lead_screen();
    for role in ["a", "b", "inv"] {
        let want = format!("[teamwire:demo] {role} finished (done)");
        assert_eq!(screen.matches(&want).count(), 1, "{role}: {screen}");
    }
    assert_eq!(screen.matches("Full result:").count(), 3, "{screen}");
    assert!(
        screen.contains("question from b] b to lead"),
        "the question to the lead is typed into its pane: {screen}"
    );

    let log = w.teamwire(&["log", "demo"], None);
    assert_eq!(log.code, 0, "{}", log.all());
    assert_eq!(log.out.lines().count(), 3, "{}", log.out);
    for (line, state) in [
        ("question b -> lead", "handed_off"),
        ("question a -> inv", "acked"),
        ("finding inv -> a", "acked"),
    ] {
        assert!(
            log.out
                .lines()
                .any(|l| l.contains(line) && l.contains(state)),
            "{line} {state}: {}",
            log.out
        );
    }

    let verify = w.teamwire(&["verify", "demo"], None);
    assert_eq!(verify.code, 0, "{}", verify.all());
    assert!(verify.out.contains("a            PASS"), "{}", verify.out);
    assert!(verify.out.contains("b            PASS"), "{}", verify.out);

    let close = w.teamwire(&["close", "demo"], None);
    assert_eq!(close.code, 0, "{}", close.all());
    assert!(
        close.out.contains("closed team demo. Archive: "),
        "{}",
        close.out
    );
    assert!(!w.repo().join(".worktrees/a").exists());
    assert!(!w.repo().join(".worktrees/b").exists());
    assert!(
        !w.team_dir("demo").exists(),
        "the state directory is deleted"
    );
    let archive = w.home.path().join("archive");
    assert_eq!(std::fs::read_dir(archive).unwrap().count(), 1);
    let branches = w.git(w.repo(), &["branch", "--list"]);
    assert!(
        ["a", "b"]
            .iter()
            .all(|b| branches.lines().any(|l| l.trim() == *b)),
        "close keeps the branches: {branches}"
    );
    assert_eq!(
        w.processes_mentioning(w.home.path()),
        Vec::<String>::new(),
        "nothing of the team is left running"
    );
}
