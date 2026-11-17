//! `Harness::lead_start`: `codex --no-daemon <carried flags> <prompt>`. Codex
//! picks its own session id, so the new lead runs `teamwire lead --adopt`
//! itself (ADR 0051).

use teamwire_core::{CarryRules, HarnessError, HarnessErrorCode, LeadStart, LeadStartSpec, carry};

/// The flags a new lead keeps: the approval and sandbox flags, the model and
/// profile, the working root, and the effort, which Codex takes only as a
/// `-c model_reasoning_effort=` override. Any other `-c` value is dropped,
/// because it can set anything in Codex's configuration. Codex is not
/// installed where this was written, so the list holds only flags teamwire
/// already passes (`launch.rs`) and Codex's documented approval flags.
const RULES: CarryRules<'static> = CarryRules {
    switches: &[
        "--dangerously-bypass-approvals-and-sandbox",
        "--dangerously-bypass-hook-trust",
        "--full-auto",
    ],
    with_value: &[
        "-m",
        "--model",
        "-s",
        "--sandbox",
        "-a",
        "--ask-for-approval",
        "-p",
        "--profile",
        "-C",
        "--cd",
    ],
    keyed: &[
        ("-c", "model_reasoning_effort="),
        ("--config", "model_reasoning_effort="),
    ],
    // Every lead start names it first, so the new lead stays locatable.
    replaced: &[("--no-daemon", false)],
};

pub(crate) fn lead_start(codex_bin: &str, req: &LeadStart) -> Result<LeadStartSpec, HarnessError> {
    if req.new_session.is_some() {
        return Err(HarnessError::new(
            HarnessErrorCode::BadInput,
            "Codex picks its own session id, so a Codex lead start takes none",
        ));
    }
    let (carried, dropped) = carry(&req.old_args, &RULES);
    let mut argv = vec![codex_bin.to_owned(), "--no-daemon".to_owned()];
    argv.extend(carried);
    argv.push(req.prompt.clone());
    Ok(LeadStartSpec {
        argv,
        dropped,
        warnings: Vec::new(),
    })
}
