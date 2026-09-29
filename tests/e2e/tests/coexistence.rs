//! The Python tooling's global Stop hook (`~/.claude/hooks/team-report.sh`)
//! is also present. Its first checks find no manifest for a teamwire member,
//! so it does nothing, and teamwire's own delivery is not doubled.

use std::time::Duration;

use serde_json::json;
use teamwire_e2e::{World, sh};
use teamwire_fakes::{ClaudeScript, Step};

/// The shape of `team-report.sh`: leave at once unless `~/.claude/team`
/// exists, then look for the pane or session in a Python team's
/// `manifest.json`, and only then read a `done/<role>.json` file and print
/// or write anything. It logs each run so the test can see it ran.
const PYTHON_HOOK: &str = r#"#!/bin/sh
echo "ran pane=${HERDR_PANE_ID:-none}" >> "$HOME/python-hook.log"
[ -d "$HOME/.claude/team" ] || exit 0
input=$(cat)
sid=$(printf '%s' "$input" | sed -n 's/.*"session_id"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)
for manifest in "$HOME"/.claude/team/*/manifest.json; do
  [ -f "$manifest" ] || continue
  if grep -qF "\"$HERDR_PANE_ID\"" "$manifest" || grep -qF "\"$sid\"" "$manifest"; then
    echo "matched $manifest" >> "$HOME/python-hook.log"
    mkdir -p "$(dirname "$manifest")/reported"
    echo "reported by the python hook"
    exit 0
  fi
done
exit 0
"#;

#[test]
fn the_python_global_stop_hook_does_nothing_for_a_teamwire_member() {
    let hook_input =
        r#"{"session_id":"member-session","hook_event_name":"Stop","stop_hook_active":false}"#;
    let step = format!(
        "{}{}echo \"$(printf '%s' '{hook_input}' | sh \"$HOME/.claude/hooks/team-report.sh\")\" > \"$HOME/python-hook.out\"\n\
         echo python-hook-exit=$?\n",
        sh::report(false),
        sh::done("finished beside the python hook")
    );
    let w = World::new(&ClaudeScript {
        turns: vec![vec![Step::Run(step)]],
        ..ClaudeScript::default()
    });
    let home = w.herdr.root().join("home");
    let hooks = home.join(".claude/hooks");
    let team = home.join(".claude/team/pyteam");
    std::fs::create_dir_all(&hooks).unwrap();
    std::fs::create_dir_all(&team).unwrap();
    w.write(&hooks.join("team-report.sh"), PYTHON_HOOK);
    w.write(
        &team.join("manifest.json"),
        r#"{"team":"pyteam","children":[{"role":"x","pane_id":"w9:p9","session_id":"py-session"}]}"#,
    );

    let spec = json!({"team": "demo", "title": "Demo", "children": [
        {"role": "inv", "title": "Investigator", "kind": "investigator", "mode": "pane",
         "cwd": w.repo(), "task": "look around"}]});
    let launch = w.launch(&spec, &[]);
    assert_eq!(launch.code, 0, "{}", launch.all());
    w.wait_lead_screen(
        "[teamwire:demo] inv finished (done)",
        Duration::from_secs(30),
    );

    let log = std::fs::read_to_string(home.join("python-hook.log")).unwrap();
    assert_eq!(log.lines().count(), 1, "the python hook ran once: {log}");
    assert!(
        !log.contains("matched"),
        "it found no manifest for the member: {log}"
    );
    let printed = std::fs::read_to_string(home.join("python-hook.out")).unwrap();
    assert_eq!(printed.trim(), "", "and printed nothing");
    assert!(!team.join("reported").exists(), "and wrote nothing");
    assert!(
        w.member_screen("demo", "inv")
            .contains("python-hook-exit=0")
    );
    let lead = w.lead_screen();
    assert_eq!(lead.matches("inv finished (done)").count(), 1, "{lead}");
}
