//! The steps of one member's launch that touch the host and the disk: spawn,
//! setup, the member's files, and the first message.

use std::path::Path;
use std::time::Duration;

use teamwire_core::{Command, HostError, HostErrorCode, MemberLaunch, Place, one_line};
use teamwire_store::Ctx;

use crate::contract::{ContractParts, render_contract};
use crate::member_parts::Dirs;
use crate::worktree;
use crate::{PlannedMember, ValidPlan};

const SETUP_TIMEOUT: Duration = Duration::from_secs(600);
const LIST_TIMEOUT: Duration = Duration::from_secs(30);
const SPAWN_TIMEOUT: Duration = Duration::from_secs(150);
const SUBMIT_TIMEOUT: Duration = Duration::from_secs(60);
/// The longest first message.
const FIRST_MESSAGE_MAX: usize = 600;

/// Why a member's launch stopped.
pub(crate) struct Stop {
    pub(crate) reason: String,
    /// The member was refused (by name), as opposed to failing.
    pub(crate) refused: bool,
    /// What to do next, said only when the undo left nothing behind.
    pub(crate) advice: Option<&'static str>,
}

impl Stop {
    pub(crate) fn fail(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            refused: false,
            advice: None,
        }
    }

    pub(crate) fn refuse(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            refused: true,
            advice: None,
        }
    }
}

/// Starts the agent with one call. The host adapter owns any retry: a second
/// `spawn` after "not ready" would start the same name in a new place.
pub(crate) fn spawn(
    host: &dyn teamwire_core::Host,
    request: &teamwire_core::SpawnRequest,
) -> Result<Place, Stop> {
    host.spawn(request, SPAWN_TIMEOUT).map_err(|e| {
        let mut stop = spawn_stop(&e);
        if e.code == HostErrorCode::Timeout {
            stop.reason.push_str(&orphan_note(host, request));
        }
        stop
    })
}

/// After a spawn timeout the host may have started the agent, and launch has
/// no place to close. Say whether the agent is there, and what to do.
fn orphan_note(host: &dyn teamwire_core::Host, request: &teamwire_core::SpawnRequest) -> String {
    let name = request.name.as_str();
    match host.live_agent_names(LIST_TIMEOUT) {
        Ok(names) if names.iter().any(|n| n == name) => format!(
            "; WARNING: the host started an agent named {name} anyway and it is still running. \
             Launch has no place to close, so close that agent by hand in {} before launching \
             this role again",
            host.id()
        ),
        Ok(_) => format!("; the host's agent list does not show {name}"),
        Err(e) => format!(
            "; the host's agent list could not be read ({e}), so an agent named {name} may still \
             be running: check {} and close it by hand",
            host.id()
        ),
    }
}

/// What the lead is told when the folder-trust dialog also grants what the
/// folder declares. The pane is closed by the undo, so the lead must trust
/// the repository itself first.
pub(crate) fn escalated_trust_message(repo: &Path) -> String {
    format!(
        "the repository's own settings ask for extra permissions, so the trust dialog was not \
         answered; open Claude Code yourself in {}, review and accept that dialog there, then \
         launch the member again with --add (the role is reusable because it never started)",
        repo.display()
    )
}

pub(crate) fn spawn_stop(e: &HostError) -> Stop {
    if e.code == HostErrorCode::BadArgument && e.detail.starts_with("agent_name_taken:") {
        return Stop {
            reason: format!("the agent name is already live in the host ({})", e.detail),
            refused: true,
            advice: None,
        };
    }
    let hint = if e.code == HostErrorCode::Timeout {
        " (the host may have started the agent anyway; check its agent list)"
    } else {
        ""
    };
    Stop::fail(format!("the host could not start the agent: {e}{hint}"))
}

pub(crate) fn run_setup(
    ctx: &Ctx<'_>,
    plan: &ValidPlan,
    dirs: &Dirs,
    m: &PlannedMember,
) -> Result<(), Stop> {
    let script = m.spec.setup.clone().or_else(|| {
        m.worktree
            .is_some()
            .then(|| plan.spec.setup.clone())
            .flatten()
    });
    let Some(script) = script.filter(|s| !s.trim().is_empty()) else {
        return Ok(());
    };
    let role = &m.spec.role;
    let cwd = m.cwd.display().to_string();
    let cmd = Command::new("sh")
        .args(["-c", script.as_str(), "teamwire-setup", cwd.as_str()])
        .cwd(&m.cwd)
        .env("TEAMWIRE_TEAM", plan.spec.team.as_str())
        .env("TEAMWIRE_ROLE", role.as_str())
        .env("TEAMWIRE_HOME", ctx.home.display().to_string())
        .env("TEAMWIRE_CHILD_CWD", cwd.as_str());
    let out = ctx.platform.run(&cmd, SETUP_TIMEOUT);
    let log = dirs.setup_log(role);
    let (ok, text, why) = match &out {
        Ok(o) => (
            o.success(),
            format!(
                "{}{}\n[exit {}{}]\n",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr),
                o.exit_code,
                if o.timed_out { ", timed out" } else { "" }
            ),
            if o.timed_out {
                format!("setup ran past {} s", SETUP_TIMEOUT.as_secs())
            } else {
                format!("setup exited {}", o.exit_code)
            },
        ),
        Err(e) => (
            false,
            format!("[could not run: {e}]\n"),
            format!("setup could not run: {e}"),
        ),
    };
    if let Err(e) = write_file(ctx.platform, &log, text.as_bytes(), 0o644) {
        tracing::warn!(error = %e, "could not write the setup log");
    }
    if ok {
        Ok(())
    } else {
        Err(Stop {
            reason: format!("{why} (log: {})", log.display()),
            refused: false,
            advice: Some("fix the setup command, then add the member again with --add"),
        })
    }
}

/// Writes one file under the team directory: parents made, written whole,
/// then set to `mode` (a token file is 0600).
pub(crate) fn write_file(
    platform: &dyn teamwire_core::Platform,
    path: &Path,
    bytes: &[u8],
    mode: u32,
) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot make {}: {e}", parent.display()))?;
    }
    platform
        .write_atomic(path, bytes)
        .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|e| format!("cannot set the mode of {}: {e}", path.display()))
}

pub(crate) fn write_member_files(
    ctx: &Ctx<'_>,
    dirs: &Dirs,
    m: &PlannedMember,
    launch: &MemberLaunch,
    files: &[teamwire_core::LaunchFile],
) -> Result<(), Stop> {
    use std::os::unix::fs::PermissionsExt;
    let role = &m.spec.role;
    let fail = |e: String| Stop::fail(e);
    // The member directory holds the token, so it is private too.
    std::fs::create_dir_all(&launch.member_dir)
        .and_then(|()| {
            std::fs::set_permissions(&launch.member_dir, std::fs::Permissions::from_mode(0o700))
        })
        .map_err(|e| fail(format!("cannot make {}: {e}", launch.member_dir.display())))?;
    let report = dirs.report(role);
    let contract = render_contract(&ContractParts {
        team: &launch.team,
        role,
        kind: m.kind,
        report_file: &report,
        outbox_dir: &dirs.outbox(),
        teamwire_bin: &launch.teamwire_bin,
        check: m.spec.check.as_deref(),
        owns: &m.spec.owns,
    });
    write_file(
        ctx.platform,
        &launch.contract_path,
        contract.as_bytes(),
        0o644,
    )
    .map_err(fail)?;
    write_file(ctx.platform, &dirs.brief(role), m.brief.as_bytes(), 0o644).map_err(fail)?;
    if m.worktree.is_some() {
        write_file(
            ctx.platform,
            &m.cwd.join("TASK.md"),
            m.brief.as_bytes(),
            0o644,
        )
        .map_err(fail)?;
        worktree::exclude_task_md(ctx.platform, &m.cwd).map_err(fail)?;
    }
    for file in files {
        if !file.path.starts_with(&launch.member_dir) {
            return Err(fail(format!(
                "the harness wants a file outside the member directory: {}",
                file.path.display()
            )));
        }
        write_file(ctx.platform, &file.path, &file.bytes, file.mode).map_err(fail)?;
    }
    Ok(())
}

/// The first message: a pointer to the brief, sent once (LCH-14).
pub(crate) fn dispatch(
    host: &dyn teamwire_core::Host,
    place: &Place,
    plan: &ValidPlan,
    dirs: &Dirs,
    m: &PlannedMember,
    warnings: &mut Vec<String>,
) {
    let where_ = if m.worktree.is_some() {
        format!(
            "TASK.md in your working directory ({})",
            m.cwd.join("TASK.md").display()
        )
    } else {
        dirs.brief(&m.spec.role).display().to_string()
    };
    let text = format!(
        "Your brief is at {where_}. Read it in full now, then carry it out. It is your complete \
         task: what to build, which files you own, which you must not touch, and how to finish."
    );
    let line = one_line(&text, FIRST_MESSAGE_MAX);
    if let Err(e) = host.submit(place, &line, SUBMIT_TIMEOUT) {
        warnings.push(format!(
            "{}: started, but the first message may not have arrived ({e}); it is not resent. \
             Send it yourself: teamwire send {} --to {} --type note --text \"{}\"",
            m.spec.role, plan.spec.team, m.spec.role, line
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escalated_message_names_the_main_checkout_and_add() {
        let text = escalated_trust_message(Path::new("/work/proj"));
        assert!(text.contains("open Claude Code yourself in /work/proj"));
        assert!(text.contains("--add") && text.contains("never started"));
        assert!(!text.contains("answer it yourself"));
    }
}
