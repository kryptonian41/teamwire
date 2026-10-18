//! The per-member sequence (LCH-3). Each member is recorded `creating` before
//! its first irreversible step, then: worktree, setup, files, spawn, trust,
//! `started`, and the first message. A failure at any step undoes that
//! member only, without force, and records it `failed` (LCH-4).

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use teamwire_core::{
    IdentityChannel, MemberLaunch, MemberState, SessionId, StartupOutcome, TeamState, TrustPolicy,
    Ulid, one_line,
};
use teamwire_store::{Ctx, NewMember, NewTeam};

use crate::member_parts::{Dirs, Progress, Steps, undo};
use crate::member_steps::{
    ReasonCode, Stop, dispatch, escalated_trust_message, run_setup, spawn, write_contract,
    write_member_files,
};
use crate::worktree;
use crate::{
    LaunchOutcome, LaunchReport, MemberOutcome, PlannedMember, ValidPlan, hold, watch_command,
};

const HOST_TIMEOUT: Duration = Duration::from_secs(30);
/// The longest reason kept in an event.
const REASON_MAX: usize = 300;

pub(crate) fn launch_all(ctx: &mut Ctx<'_>, plan: &ValidPlan) -> LaunchReport {
    let mut warnings = plan.warnings.clone();
    let launch_id = new_launch_id(ctx);
    let requested = plan.members.len();
    let mut members = Vec::new();
    let created = if plan.add {
        Ok(())
    } else {
        create_team(ctx, plan)
    };
    {
        if let Err(e) = created {
            for m in &plan.members {
                members.push(MemberOutcome {
                    role: m.spec.role.clone(),
                    outcome: LaunchOutcome::Failed(format!("the team could not be recorded: {e}")),
                });
            }
            return LaunchReport {
                members,
                warnings,
                watch_command: watch_command(ctx.registry, &plan.lead.harness, &plan.spec.team),
            };
        }
    }
    let dirs = Dirs {
        team: ctx.home.join("teams").join(plan.spec.team.as_str()),
    };
    for m in &plan.members {
        let outcome = launch_member(ctx, plan, &dirs, m, &launch_id, requested, &mut warnings);
        members.push(MemberOutcome {
            role: m.spec.role.clone(),
            outcome,
        });
    }
    let started = members
        .iter()
        .any(|m| matches!(m.outcome, LaunchOutcome::Started));
    if started {
        let lead = (
            &plan.lead.harness,
            plan.lead.session.as_ref().map(|(s, _)| s),
        );
        hold::ensure_hold(ctx, &plan.spec.team, lead, &mut warnings);
    }
    LaunchReport {
        members,
        warnings,
        watch_command: watch_command(ctx.registry, &plan.lead.harness, &plan.spec.team),
    }
}

fn random_u128() -> Result<u128, getrandom::Error> {
    Ok((u128::from(getrandom::u64()?) << 64) | u128::from(getrandom::u64()?))
}

fn new_launch_id(ctx: &Ctx<'_>) -> String {
    let random = random_u128().unwrap_or_default();
    Ulid::from_parts(ctx.clock.now(), random).to_string()
}

fn create_team(ctx: &mut Ctx<'_>, plan: &ValidPlan) -> Result<(), String> {
    let at = ctx.clock.now();
    let team = NewTeam {
        name: plan.spec.team.clone(),
        title: crate::plan::clean(&plan.spec.title),
        lead_harness: plan.lead.harness.clone(),
        lead_host: plan.lead.host.clone(),
        lead_route: plan.lead.route,
        lead_session: plan.lead.session.clone(),
    };
    // The folder lets `reopen` resume the lead from where it runs. A lead
    // with no session, or one the harness cannot place, leaves it out.
    let cwd = plan
        .lead
        .session
        .as_ref()
        .and_then(|(session, _)| {
            ctx.registry
                .harness(&plan.lead.harness)
                .ok()?
                .session_cwd(session)
                .ok()?
        })
        .map(|p| p.display().to_string());
    let extra = cwd.map_or(json!({}), |cwd| json!({ "lead_cwd": cwd }));
    ctx.store
        .write(|tx| tx.create_team_with(at, &team, &extra).map(|_| ()))
        .map_err(|e| e.to_string())
}

fn launch_member(
    ctx: &mut Ctx<'_>,
    plan: &ValidPlan,
    dirs: &Dirs,
    m: &PlannedMember,
    launch_id: &str,
    requested: usize,
    warnings: &mut Vec<String>,
) -> LaunchOutcome {
    let mut made = Progress::default();
    let mut steps = Steps {
        marks: Vec::new(),
        last: ctx.clock.monotonic(),
    };
    let extra = json!({ "launch": launch_id, "requested": requested });
    let result = run_member(ctx, plan, dirs, m, &extra, &mut made, &mut steps, warnings);
    match result {
        Ok(()) => LaunchOutcome::Started,
        Err(stop) => {
            let stop_code = stop.code;
            let (undo, left) = undo(ctx, m, &made);
            let mut reason = if undo.is_empty() {
                stop.reason
            } else {
                format!("{}; undone: {undo}", stop.reason)
            };
            if let (Some(advice), false) = (stop.advice, left) {
                reason = format!("{reason}; {advice}");
            }
            if made.recorded {
                record_failed(ctx, m, stop_code, &reason, &extra, &steps);
            }
            if stop.refused {
                LaunchOutcome::Refused(reason)
            } else {
                LaunchOutcome::Failed(reason)
            }
        }
    }
}

fn record_failed(
    ctx: &mut Ctx<'_>,
    m: &PlannedMember,
    code: &str,
    reason: &str,
    extra: &Value,
    steps: &Steps,
) {
    let at = ctx.clock.now();
    let mut data = extra.clone();
    put(&mut data, "reason_code", json!(code));
    put(
        &mut data,
        "reason",
        json!(one_line(reason, REASON_MAX).as_str()),
    );
    put(&mut data, "steps", steps.json());
    let role = m.spec.role.clone();
    if let Err(e) = ctx
        .store
        .write(|tx| tx.set_member_state(at, &role, MemberState::Failed, &data))
    {
        tracing::warn!(role = %m.spec.role, error = %e, "could not record the failed launch");
    }
}

/// Sets `key` on a JSON object.
fn put(data: &mut Value, key: &str, value: Value) {
    if let Some(map) = data.as_object_mut() {
        map.insert(key.to_owned(), value);
    }
}

fn new_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| format!("no random bytes for the token: {e}"))?;
    Ok(hex::encode(bytes))
}

fn sha256(text: &str) -> [u8; 32] {
    Sha256::digest(text.as_bytes()).into()
}

#[expect(
    clippy::too_many_arguments,
    reason = "one sequence, each argument a distinct input"
)]
fn run_member(
    ctx: &mut Ctx<'_>,
    plan: &ValidPlan,
    dirs: &Dirs,
    m: &PlannedMember,
    extra: &Value,
    made: &mut Progress,
    steps: &mut Steps,
    warnings: &mut Vec<String>,
) -> Result<(), Stop> {
    let role = &m.spec.role;
    let harness = ctx
        .registry
        .harness(&m.harness)
        .map_err(|e| Stop::fail(ReasonCode::Registry, e.to_string()))?;
    let host = ctx
        .registry
        .host(&m.host)
        .map_err(|e| Stop::fail(ReasonCode::Registry, e.to_string()))?;
    let token = new_token().map_err(|e| Stop::fail(ReasonCode::Prepare, e))?;
    let base_sha = match &m.worktree {
        Some(w) => Some(
            worktree::start_commit(ctx.platform, w)
                .map_err(|e| Stop::fail(ReasonCode::Prepare, e))?,
        ),
        None => None,
    };

    // 1. Record the member before anything irreversible.
    let new = NewMember {
        role: role.clone(),
        title: m.spec.title.clone(),
        kind: m.kind,
        mode: m.spec.mode,
        harness: m.harness.clone(),
        host: m.host.clone(),
        cwd: m.cwd.clone(),
        repo: m.worktree.as_ref().map(|w| w.repo.clone()),
        branch: m.worktree.as_ref().map(|w| w.branch.clone()),
        base: m.worktree.as_ref().and_then(|w| w.base.clone()),
        base_sha: base_sha.clone(),
        check: m.spec.check.clone(),
        owns: m.spec.owns.clone(),
        deadline_s: m.deadline_s,
    };
    let hash = sha256(&token);
    let at = ctx.clock.now();
    // The close lock is read inside the write that records the member, so a
    // close that took it after validation cannot be missed (CLS-3).
    let mut closed = None;
    let recorded = ctx.store.write(|tx| {
        if let Some(team) = tx.reader().team()?
            && team.state != TeamState::Open
        {
            closed = Some(team.state);
            return Ok(());
        }
        if m.relaunch {
            if !tx.relaunch_failed_member_with(at, &new, extra)? {
                return Err(teamwire_store::StoreError::Constraint(format!(
                    "member {role} is not a failed member"
                )));
            }
        } else {
            tx.insert_member_with(at, &new, extra)?;
        }
        tx.set_member_token_hash(role, &hash)
    });
    if let Some(state) = closed {
        return Err(Stop::refuse(
            ReasonCode::Record,
            format!(
                "team_closed: team {} is {state}; the member was not started",
                plan.spec.team
            ),
        ));
    }
    recorded.map_err(|e| {
        Stop::fail(
            ReasonCode::Record,
            format!("the member could not be recorded: {e}"),
        )
    })?;
    made.recorded = true;
    steps.mark(ctx, "record");

    // 2. The worktree.
    if let (Some(w), Some(sha)) = (&m.worktree, &base_sha) {
        made.worktree = Some(
            worktree::add(ctx.platform, w, sha).map_err(|e| Stop::fail(ReasonCode::Worktree, e))?,
        );
        steps.mark(ctx, "worktree");
    }

    // 3. Setup, before the agent exists.
    run_setup(ctx, plan, dirs, m, warnings)?;
    steps.mark(ctx, "setup");

    // 4. Contract, brief, and the harness's launch files.
    let member_dir = dirs.member(role);
    let session = SessionId::parse(&uuid::Uuid::new_v4().to_string())
        .map_err(|e| Stop::fail(ReasonCode::Prepare, e.to_string()))?;
    let identity = vec![
        ("TEAMWIRE_TEAM".to_owned(), plan.spec.team.to_string()),
        ("TEAMWIRE_ROLE".to_owned(), role.to_string()),
        ("TEAMWIRE_HOME".to_owned(), ctx.home.display().to_string()),
        ("TEAMWIRE_TOKEN".to_owned(), token),
    ];
    let launch = MemberLaunch {
        team: plan.spec.team.clone(),
        role: role.clone(),
        address: m.address.clone(),
        kind: m.kind,
        cwd: m.cwd.clone(),
        session_id: session.clone(),
        model: m.spec.model.clone(),
        effort: m.spec.effort.clone(),
        member_dir: member_dir.clone(),
        contract_path: dirs.contract(role),
        identity,
        otel: ctx.config.otel.clone(),
        teamwire_bin: plan.teamwire_bin.clone(),
        plugins: merged_plugins(&plan.spec.plugins, &m.spec.plugins),
        accept_trust: m.spec.accept_trust,
    };
    write_contract(ctx, dirs, m, &launch)?;
    let spec = harness.launch_spec(&launch).map_err(|e| {
        Stop::fail(
            ReasonCode::Harness,
            format!("the harness could not build the launch: {e}"),
        )
    })?;
    if spec.identity_channel != m.channel {
        return Err(Stop::fail(
            ReasonCode::Harness,
            format!(
                "the harness used identity channel {} but validation chose {}",
                spec.identity_channel, m.channel
            ),
        ));
    }
    write_member_files(ctx, dirs, m, &launch, &spec.files)?;
    steps.mark(ctx, "files");

    // 5. Spawn through the host.
    let env = if spec.identity_channel == IdentityChannel::HostEnv {
        spec.env
    } else {
        Vec::new()
    };
    let request = teamwire_core::SpawnRequest {
        name: m.address.clone(),
        title: m.spec.title.clone(),
        cwd: m.cwd.clone(),
        argv: spec.argv,
        env,
    };
    let place = spawn(host, &request)?;
    made.place = Some(place.clone());
    let chosen = harness.capabilities().choose_session_id;
    ctx.store
        .write(|tx| {
            tx.set_member_place(role, &place)?;
            if chosen {
                tx.set_member_session(role, &session)?;
            }
            Ok(())
        })
        .map_err(|e| {
            Stop::fail(
                ReasonCode::Place,
                format!("the place could not be recorded: {e}"),
            )
        })?;
    steps.mark(ctx, "spawn");

    // 6. Trust, through the harness.
    let policy = if m.spec.accept_trust {
        TrustPolicy::Accept
    } else {
        TrustPolicy::Refuse
    };
    match harness.answer_startup(host, &place, policy) {
        Ok(StartupOutcome::NoDialog | StartupOutcome::AnsweredPlainTrust) => {}
        Ok(StartupOutcome::RefusedEscalated) => {
            let repo = m.worktree.as_ref().map_or(&m.cwd, |w| &w.repo);
            return Err(Stop::fail(ReasonCode::Trust, escalated_trust_message(repo)));
        }
        Err(e) => {
            return Err(Stop::fail(
                ReasonCode::Trust,
                format!("the trust step failed: {e}"),
            ));
        }
    }
    steps.mark(ctx, "trust");

    // 7. Started.
    let session_known = if chosen {
        Some(session)
    } else {
        host.occupant(&place, HOST_TIMEOUT)
            .ok()
            .and_then(|o| o.session)
    };
    if let (false, Some(s)) = (chosen, &session_known) {
        let s = s.clone();
        // The session-start hook may have recorded the session already; it
        // knows the real one, so a value on the row stays.
        let kept = ctx.store.write(|tx| match tx.reader().member(role)? {
            Some(row) if row.session_id.is_some() => Ok(()),
            _ => tx.set_member_session(role, &s),
        });
        if let Err(e) = kept {
            tracing::warn!(%role, error = %e, "could not record the session id");
        }
    }
    steps.mark(ctx, "trust_done");
    let mut data = extra.clone();
    put(&mut data, "steps", steps.json());
    // `reopen` resumes with these; a resume without them runs on the default.
    put(&mut data, "model", json!(m.spec.model));
    put(&mut data, "effort", json!(m.spec.effort));
    put(&mut data, "plugins", json!(launch.plugins));
    put(
        &mut data,
        "session_id",
        json!(session_known.as_ref().map(SessionId::as_str)),
    );
    let at = ctx.clock.now();
    ctx.store
        .write(|tx| tx.set_member_state(at, role, MemberState::Started, &data))
        .map_err(|e| {
            Stop::fail(
                ReasonCode::Started,
                format!("the member could not be marked started: {e}"),
            )
        })?;
    made.recorded = false;

    // 8. The first message, once (LCH-14, LCH-19).
    if m.spec.dispatch {
        dispatch(host, &place, plan, dirs, m, warnings);
    }
    Ok(())
}

/// The team's plugin map overlaid by the member's: the member wins per key.
pub(crate) fn merged_plugins(
    team: &BTreeMap<String, bool>,
    member: &BTreeMap<String, bool>,
) -> BTreeMap<String, bool> {
    let mut merged = team.clone();
    merged.extend(member.iter().map(|(k, v)| (k.clone(), *v)));
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn member_value_wins_and_other_keys_are_kept() {
        let map = |kv: &[(&str, bool)]| kv.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect();
        let team = map(&[("a@x", true), ("b@x", false)]);
        let member = map(&[("a@x", false), ("c@x", true)]);
        assert_eq!(
            merged_plugins(&team, &member),
            map(&[("a@x", false), ("b@x", false), ("c@x", true)])
        );
    }
}
