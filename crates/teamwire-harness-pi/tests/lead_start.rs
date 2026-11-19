//! A fresh pi lead's command line (ADR 0051): the chosen session id first,
//! the old lead's allowed flags, then `--` and the first prompt. A secret
//! flag is named, never its value.

#![expect(
    clippy::unwrap_used,
    reason = "tests build fixtures from literals; a failure should fail the test"
)]

use std::sync::Arc;

use teamwire_core::{Harness, LeadStart, SessionId};
use teamwire_fakes::FakePlatform;
use teamwire_harness_pi::{Pi, PiConfig};

const OLD: &str = "01a1725a-5528-728a-b86b-775340828a01";
const NEW: &str = "01a1725a-5528-728a-b86b-775340828a02";
const PROMPT: &str = "@you are a teamwire lead. Load the teamwire skill.";

fn pi() -> Pi {
    Pi::new(
        Arc::new(FakePlatform::new()),
        PiConfig {
            agent_dir: "/nowhere/pi".into(),
            pi_bin: "pi".into(),
            exclude_providers: Vec::new(),
        },
    )
}

fn request(old_args: &str) -> LeadStart {
    LeadStart {
        old_session: SessionId::parse(OLD).unwrap(),
        old_args: old_args.split(' ').map(str::to_owned).collect(),
        new_session: SessionId::parse(NEW).unwrap(),
        prompt: PROMPT.to_owned(),
    }
}

#[test]
fn the_new_lead_keeps_its_model_thinking_and_session_folder_and_nothing_else() {
    let old = format!(
        "/opt/bin/pi --session {OLD} --api-key sk-s3cret --provider anthropic \
         --model claude-opus --thinking high --session-dir /work/sessions -c --no-tools"
    );
    let spec = pi().lead_start(&request(&old)).unwrap();
    assert_eq!(
        spec.argv,
        [
            "pi",
            "--session-id",
            NEW,
            "--provider",
            "anthropic",
            "--model",
            "claude-opus",
            "--thinking",
            "high",
            "--session-dir",
            "/work/sessions",
            "--",
            PROMPT,
        ]
    );
    // The session flags are replaced, so only the unknown flags are named.
    assert_eq!(spec.dropped, ["--api-key", "--no-tools"]);
    let all = format!("{:?}", (&spec.argv, &spec.dropped));
    assert!(!all.contains("s3cret"), "{all}");
    assert!(!all.contains(OLD), "{all}");
}
