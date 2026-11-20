//! The pi lead package's pull-request watch tool (ADR 0039). pi has no tool
//! like Claude's Monitor, so the extension runs `teamwire pr-watch` itself
//! and steers each line into the session, as it does for `teamwire watch`
//! (ADR 0036), and refuses `teamwire pr-watch` through bash.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use teamwire_fakes::FakePlatform;
use teamwire_harness_pi::lead_setup::{LeadCommand, LeadSetupError};
use teamwire_harness_pi::{Pi, PiConfig};

/// The extension text `install` writes, as pi reads it.
fn extension() -> Result<String, LeadSetupError> {
    let pi = Pi::new(
        Arc::new(FakePlatform::new()) as Arc<dyn teamwire_core::Platform>,
        PiConfig {
            agent_dir: PathBuf::from("/pi/agent"),
            pi_bin: "pi".into(),
            exclude_providers: Vec::new(),
        },
    );
    let want = LeadCommand {
        bin: PathBuf::from("/opt/bin/teamwire"),
        home: None,
    };
    let files = pi.package_files(
        &want,
        "---\nname: teamwire\ndescription: d\n---\n\n# Body\n",
    )?;
    Ok(files
        .into_iter()
        .find(|(p, _)| p == Path::new("extensions/teamwire-lead.ts"))
        .map(|(_, text)| text)
        .unwrap_or_default())
}

#[test]
fn the_tool_takes_the_repo_the_pull_requests_and_an_optional_interval() {
    let text = extension().unwrap();
    assert!(
        text.contains(r#""prWatchTool":"teamwire_pr_watch""#),
        "{text}"
    );
    assert!(text.contains("name: TW.prWatchTool,"), "{text}");
    for param in [
        "repo: Type.String(",
        "prs: Type.Array(Type.Integer(",
        "interval: Type.Optional(",
    ] {
        assert!(text.contains(param), "{param}: {text}");
    }
}

#[test]
fn the_tool_runs_pr_watch_without_deliver_and_steers_its_lines_like_the_watch() {
    let text = extension().unwrap();
    // pi reads each line itself. `pr-watch` refuses `--deliver` since ADR
    // 0055, so a tool that passed it would never start the watch.
    assert!(
        text.contains(r#"["pr-watch", "-R", repo, ...every, ...prs]"#),
        "{text}"
    );
    assert!(!text.contains("--deliver"), "{text}");
    // Both tools start their child through the one function whose lines go
    // to the one steered `sendMessage` call.
    assert_eq!(text.matches("await startWatch(").count(), 2, "{text}");
    assert_eq!(text.matches("pi.sendMessage(").count(), 1, "{text}");
    assert!(
        text.contains(r#"{ triggerTurn: true, deliverAs: "steer" }"#),
        "{text}"
    );
    // A failed check ends pr-watch with exit 1: an ordinary end, not a stop.
    assert!(
        text.contains("(code) => code === 0 || code === 1"),
        "{text}"
    );
}

#[test]
fn bash_refuses_pr_watch_and_names_the_tool() {
    let text = extension().unwrap();
    assert!(text.contains(r"|pr-watch\b)/;"), "{text}");
    assert!(text.contains("if (found && !found[1]) {"), "{text}");
    assert!(
        text.contains("In pi, watch pull requests with the ${TW.prWatchTool} tool"),
        "{text}"
    );
}
