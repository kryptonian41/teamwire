//! Reopen of the team's lead: a plain resume of its session in a new tab
//! named `tw_<team>_lead`, from the folder the session ran in.

use std::path::PathBuf;
use std::time::Duration;

use teamwire_core::{Address, LaunchSpec, Liveness, SessionKind, SpawnRequest};
use teamwire_store::{Ctx, TeamRow};

use crate::reopen::{Ready, ReopenOpts, ReopenOutcome};

const ALIVE_TIMEOUT: Duration = Duration::from_secs(5);

#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
)]
/// Checks the lead. A lead that passes is `Reopened` (on a dry run, "would
/// be") and, unless this is a dry run, leaves its tab request in `ready`.
pub(crate) fn reopen_lead<'r>(
    ctx: &Ctx<'r>,
    team: &TeamRow,
    opts: &ReopenOpts,
    ready: &mut Option<Ready<'r>>,
) -> ReopenOutcome {
    let Some(session) = &team.lead_session else {
        return ReopenOutcome::skipped("no lead session is recorded");
    };
    if team.lead_kind == Some(SessionKind::Background) {
        return ReopenOutcome::skipped("the lead is a background session");
    }
    let registry = ctx.registry;
    let harness = match registry.harness(&team.lead_harness) {
        Ok(harness) => harness,
        Err(e) => return ReopenOutcome::refused(e.to_string()),
    };
    // The caller is the lead: it runs, whatever the session registry says.
    if harness
        .current_session()
        .is_some_and(|caller| caller.id == *session)
    {
        return ReopenOutcome::skipped("already open (this session)");
    }
    match harness.is_alive(session, ALIVE_TIMEOUT) {
        Ok(Liveness::Dead) => {}
        Ok(Liveness::Alive(_)) => return ReopenOutcome::skipped("already open"),
        Ok(Liveness::Unknown) => {
            return ReopenOutcome::skipped("it is not known whether the session runs");
        }
        Err(e) => {
            return ReopenOutcome::skipped(format!(
                "it is not known whether the session runs ({e})"
            ));
        }
    }
    let argv = match harness.lead_resume_argv(session) {
        Ok(argv) => argv,
        Err(e) => return ReopenOutcome::refused(format!("the harness cannot resume a lead: {e}")),
    };
    // The folder recorded at launch or adoption; else what the harness can
    // still read of the session. A resume only finds a session from there.
    let recorded = ctx.store.read(|tx| tx.lead_cwd()).ok().flatten();
    let cwd: Option<PathBuf> = recorded.or_else(|| harness.session_cwd(session).ok().flatten());
    let manual = |cwd: Option<&PathBuf>| {
        let command = LaunchSpec::shell_line(&argv);
        match cwd {
            Some(dir) => format!(
                "cd {} && {command}",
                LaunchSpec::shell_line(&[dir.display().to_string()])
            ),
            None => format!("{command} (from the folder the session ran in)"),
        }
    };
    let Some(cwd) = cwd else {
        return ReopenOutcome::refused(format!(
            "the lead's folder is not recorded and the session does not name it; to resume it \
             by hand run: {}",
            manual(None)
        ));
    };
    if team.lead_host.as_str() == teamwire_core::NoHost::ID {
        return ReopenOutcome::refused(format!(
            "the lead's host is none, so there is no tab to open; to resume it by hand run: {}",
            manual(Some(&cwd))
        ));
    }
    let host = match registry.host(&team.lead_host) {
        Ok(host) => host,
        Err(e) => return ReopenOutcome::refused(e.to_string()),
    };
    if crate::plan::is_home_dir(opts.user_home.as_deref(), &cwd) {
        return ReopenOutcome::refused(format!(
            "the lead's folder {} is your home directory, and reopen will not answer Claude's \
             folder-trust dialog for it; to resume it by hand run: {}",
            cwd.display(),
            manual(Some(&cwd))
        ));
    }
    if !cwd.is_dir() {
        return ReopenOutcome::refused(format!(
            "the lead's folder {} is gone; to resume it by hand run: {}",
            cwd.display(),
            manual(Some(&cwd))
        ));
    }
    if opts.dry_run {
        return ReopenOutcome::Reopened;
    }
    let request = SpawnRequest {
        name: Address::lead(&team.name),
        title: "lead".to_owned(),
        cwd: cwd.clone(),
        argv: argv.clone(),
        env: Vec::new(),
    };
    *ready = Some(Ready {
        harness,
        host,
        request,
        folder: cwd,
    });
    ReopenOutcome::Reopened
}
