//! `Harness::lead_start`: `pi --session-id <new> <carried flags> --
//! <prompt>`. `teamwire install` registers the lead package with
//! `pi install`, so the new session loads the lead extension, which records
//! the session at `session_start` (`docs/design/2026-10-30-pi-lead.md` 4.1).
//! pi finds a session id in the current folder, so the new pane starts in
//! the old lead's folder.

use teamwire_core::{CarryRules, HarnessError, HarnessErrorCode, LeadStart, LeadStartSpec, carry};

/// The flags a new lead keeps, from `pi --help` (pi 1.4, 2026-11-17): the
/// trust switches, the provider, model and thinking level, the models to
/// cycle, the session folder (the new session must live beside the old
/// one), and three switches that change nothing but the session's surface.
/// `--api-key` is dropped, and only its name is shown.
const RULES: CarryRules<'static> = CarryRules {
    switches: &[
        "--approve",
        "-a",
        "--no-approve",
        "-na",
        "--verbose",
        "--offline",
    ],
    with_value: &[
        "--provider",
        "--model",
        "--thinking",
        "--models",
        "--session-dir",
        "--tui-mode",
    ],
    keyed: &[],
    replaced: &[
        ("--session-id", true),
        ("--session", true),
        ("--fork", true),
        ("--continue", false),
        ("-c", false),
        ("--resume", false),
        ("-r", false),
    ],
};

pub(crate) fn lead_start(pi_bin: &str, req: &LeadStart) -> Result<LeadStartSpec, HarnessError> {
    let new = req.new_session.as_ref().ok_or_else(|| {
        HarnessError::new(
            HarnessErrorCode::BadInput,
            "a pi lead start needs the new session's id",
        )
    })?;
    let (carried, dropped) = carry(&req.old_args, &RULES);
    let mut argv = vec![
        pi_bin.to_owned(),
        "--session-id".to_owned(),
        new.as_str().to_owned(),
    ];
    argv.extend(carried);
    // pi reads a message that starts with `@` as a file to attach, and `--`
    // ends its options, so the prompt is always a message.
    argv.extend(["--".to_owned(), req.prompt.clone()]);
    Ok(LeadStartSpec { argv, dropped })
}
