//! A fresh Codex lead's command line (ADR 0051): `--no-daemon` first, the old
//! lead's allowed flags, the effort override and no other `-c`, and the
//! first prompt last. Codex picks its own session id.

#![expect(
    clippy::unwrap_used,
    reason = "tests build fixtures from literals; a failure should fail the test"
)]

use std::sync::Arc;

use teamwire_core::{Harness, HarnessErrorCode, LeadStart, SessionId};
use teamwire_fakes::FakePlatform;
use teamwire_harness_codex::{CodexCli, CodexConfig};

const OLD: &str = "019e0000-0000-7000-8000-000000000001";
const PROMPT: &str = "First run teamwire lead --adopt demo. You are a teamwire lead.";

fn codex() -> CodexCli {
    CodexCli::new(
        Arc::new(FakePlatform::new()),
        CodexConfig {
            codex_home: "/nowhere/codex".into(),
            codex_bin: "codex".into(),
        },
    )
}

fn request(old_args: &str) -> LeadStart {
    LeadStart {
        old_session: SessionId::parse(OLD).unwrap(),
        old_args: old_args.split(' ').map(str::to_owned).collect(),
        new_session: None,
        prompt: PROMPT.to_owned(),
    }
}

#[test]
fn the_new_lead_keeps_its_approvals_model_and_effort_and_nothing_else() {
    let old = format!(
        "/opt/bin/codex --no-daemon resume {OLD} --dangerously-bypass-approvals-and-sandbox \
         -m gpt-5.5 -c model_reasoning_effort=\"high\" -c mcp_servers.x.env.TOKEN=\"s3cret\" \
         --config=model_reasoning_effort=low --search -C /work/lead"
    );
    let spec = codex().lead_start(&request(&old)).unwrap();
    assert_eq!(
        spec.argv,
        [
            "codex",
            "--no-daemon",
            "--dangerously-bypass-approvals-and-sandbox",
            "-m",
            "gpt-5.5",
            "-c",
            "model_reasoning_effort=\"high\"",
            "--config=model_reasoning_effort=low",
            "-C",
            "/work/lead",
            PROMPT,
        ]
    );
    // The other `-c` is named, never its value.
    assert_eq!(spec.dropped, ["-c", "--search"]);
    let all = format!("{:?}", (&spec.argv, &spec.dropped));
    assert!(!all.contains("s3cret"), "{all}");
    assert!(!all.contains(OLD), "{all}");
}

#[test]
fn a_codex_lead_start_with_a_chosen_session_id_is_refused() {
    let mut req = request("codex");
    req.new_session = Some(SessionId::parse(OLD).unwrap());
    let err = codex().lead_start(&req).unwrap_err();
    assert_eq!(err.code, HarnessErrorCode::BadInput);
}
