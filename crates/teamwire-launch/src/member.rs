//! The per-member sequence (LCH-3). Each member is recorded `creating` before
//! its first irreversible step, then: worktree, setup, files, spawn, trust,
//! `started`, and the first message. A failure at any step undoes that
//! member only, without force, and records it `failed` (LCH-4). A refusal
//! (the AFK budget has ended, checked at the start and again right before
//! spawn, or the team has started closing) is undone the same way and
//! writes `member.refused`.

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::{Value, json};
use teamwire_core::{
    EventType, IdentityChannel, MemberLaunch, MemberState, SessionId, StartupOutcome, TeamState,
    TrustPolicy, Ulid, one_line,
};
use teamwire_store::{Ctx, NewMember, NewTeam, StoreError, Tx, token_hash};

use crate::member_parts::{Dirs, Progress, Steps, undo};
use crate::member_steps::{
    ReasonCode, Stop, dispatch, escalated_trust_message, not_open, run_setup, spawn,
    write_contract, write_member_files,
};
use crate::worktree;
use crate::{
    LaunchOutcome, LaunchReport, MemberOutcome, PlannedMember, ValidPlan, hold, watch_command,
};

const HOST_TIMEOUT: Duration = Duration::from_secs(30);
/// The longest reason kept in an event.
const REASON_MAX: usize = 300;
/// The refusals that write `member.refused` instead of `member.failed`, by
/// their `reason_code`. Each one's reason starts with its code and a colon.
const REFUSALS: [&str; 2] = ["afk_budget_spent", "team_closed"];

pub(crate) fn launch_all(ctx: &mut Ctx<'_>, plan: &ValidPlan) -> LaunchReport {
    let mut warnings = plan.warnings.clone();
    let launch_id = new_launch_id(ctx);
    let requested = plan.members.len();
    let mut members = Vec::new();
    let created = if plan.add {
        add_refs(ctx, plan)
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
    let mut extra = json!({ "refs": plan.new_refs });
    if let Some(cwd) = cwd {
        put(&mut extra, "lead_cwd", json!(cwd));
    }
    ctx.store
        .write(|tx| {
            tx.create_team_with(at, &team, &extra)?;
            tx.set_refs(&plan.new_refs)
        })
        .map_err(|e| e.to_string())
}

/// `--add`: records the refs the team does not hold yet, with their event, in
/// one transaction. A spec with no new ref writes nothing.
fn add_refs(ctx: &mut Ctx<'_>, plan: &ValidPlan) -> Result<(), String> {
    if plan.new_refs.is_empty() {
        return Ok(());
    }
    let at = ctx.clock.now();
    ctx.store
        .write(|tx| tx.add_refs(at, &plan.new_refs).map(|_| ()))
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
    // `task_class` is `null` when the spec sets none, so a reader of
    // `member.creating` can tell an unclassed member from a missing key.
    let extra = json!({
        "launch": launch_id,
        "requested": requested,
        "dispatch": m.spec.dispatch,
        "task_class": m.spec.task_class,
    });
    let result = run_member(ctx, plan, dirs, m, &extra, &mut made, &mut steps, warnings);
    match result {
        Ok(()) => LaunchOutcome::Started,
        Err(stop) => {
            let refusal = REFUSALS
                .into_iter()
                .find(|code| stop.refused && stop.reason.starts_with(&format!("{code}:")));
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
            match refusal {
                Some(code) => record_refused(ctx, m, code, &reason, &extra, &steps, made.recorded),
                None if made.recorded => record_failed(ctx, m, stop_code, &reason, &extra, &steps),
                None => {}
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

/// Writes `member.refused`. A member launch had already recorded is left
/// `failed`, as a failed launch leaves it, so close still finds whatever the
/// undo could not remove and `--add` can start it again. When the store
/// refuses the write (the team is being closed), nothing is recorded.
fn record_refused(
    ctx: &mut Ctx<'_>,
    m: &PlannedMember,
    code: &str,
    reason: &str,
    extra: &Value,
    steps: &Steps,
    recorded: bool,
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
    let written = ctx.store.write(|tx| {
        if recorded {
            tx.refuse_member(at, &role, &data)
        } else {
            tx.record_event(at, EventType::MemberRefused, Some(&role), None, &data)
                .map(|_| ())
        }
    });
    if let Err(e) = written {
        tracing::warn!(role = %m.spec.role, error = %e, "could not record the refused launch");
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
    // The code of a refusal is never written: `REFUSALS` gives its own.
    if let Some(why) = crate::afk::budget_refusal(ctx.store, ctx.clock, m) {
        return Err(Stop::refuse(ReasonCode::Prepare, why));
    }
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
    let hash = token_hash(&token);
    let at = ctx.clock.now();
    // The close lock is read inside the write that records the member, so a
    // close that took it after validation cannot be missed (CLS-3).
    write_if_open(
        ctx,
        plan,
        ReasonCode::Record,
        "the member could not be recorded",
        |tx| {
            if m.relaunch {
                if !tx.relaunch_failed_member_with(at, &new, extra)? {
                    return Err(StoreError::Constraint(format!(
                        "member {role} is not a failed member"
                    )));
                }
            } else {
                tx.insert_member_with(at, &new, extra)?;
            }
            tx.set_member_token_hash(role, &hash)
        },
    )?;
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
    run_setup(ctx, plan, dirs, m, made, warnings)?;
    steps.mark(ctx, "setup");

    // 4. Contract, brief, and the harness's launch files. A close may have
    // taken the lock while setup ran; files written now would re-create a
    // folder that close removes.
    let state = ctx.store.read(not_open).map_err(|e| {
        Stop::fail(
            ReasonCode::Files,
            format!("the team could not be read: {e}"),
        )
    })?;
    if let Some(state) = state {
        return Err(team_closed(plan, ReasonCode::Files, state));
    }
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
        context_handoff_tokens: m
            .spec
            .context_handoff_tokens
            .or(plan.spec.context_handoff_tokens),
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
    write_member_files(ctx, dirs, m, &launch, &spec.files, made)?;
    steps.mark(ctx, "files");

    // 5. Spawn through the host.
    let env = if spec.identity_channel == IdentityChannel::HostEnv {
        spec.env
    } else {
        Vec::new()
    };
    // The team row's title: an `--add` spec may name another one.
    let team_title =
        hold::team_of(ctx.store).map_or_else(|| crate::plan::clean(&plan.spec.title), |t| t.title);
    let request = teamwire_core::SpawnRequest {
        name: m.address.clone(),
        title: m.spec.title.clone(),
        team_title,
        // `insert_member` starts every member at generation 1.
        generation: 1,
        cwd: m.cwd.clone(),
        argv: spec.argv,
        env,
    };
    // Setup can outlast the AFK budget; no member starts after it ends.
    if let Some(why) = crate::afk::budget_refusal(ctx.store, ctx.clock, m) {
        return Err(Stop::refuse(ReasonCode::Spawn, why));
    }
    let place = spawn(host, &request)?;
    made.place = Some(place.clone());
    let chosen = harness.capabilities().choose_session_id;
    write_if_open(
        ctx,
        plan,
        ReasonCode::Place,
        "the place could not be recorded",
        |tx| {
            tx.set_member_place(role, &place)?;
            if chosen {
                tx.set_member_session(role, &session)?;
            }
            Ok(())
        },
    )?;
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
        "context_handoff_tokens",
        json!(launch.context_handoff_tokens),
    );
    // Close takes these lines back when the team ends; a line that was
    // already in the file is not launch's, so it is not listed.
    let excluded: Vec<Value> = made
        .excluded
        .iter()
        .map(|e| json!({ "file": e.file.display().to_string(), "lines": e.lines }))
        .collect();
    put(&mut data, "excluded", json!(excluded));
    put(
        &mut data,
        "session_id",
        json!(session_known.as_ref().map(SessionId::as_str)),
    );
    let at = ctx.clock.now();
    write_if_open(
        ctx,
        plan,
        ReasonCode::Started,
        "the member could not be marked started",
        |tx| tx.set_member_state(at, role, MemberState::Started, &data),
    )?;
    made.recorded = false;

    // 8. The first message, once (LCH-14, LCH-19).
    if m.spec.dispatch {
        dispatch(host, &place, plan, dirs, m, warnings);
    }
    Ok(())
}

/// Runs `f` in one write that first reads the team state. A team a close has
/// locked gets no write, and the member is refused with `team_closed`, so its
/// undo runs (CLS-3). The check is in launch, not in the store's setters,
/// because the undo must still record a closing team's member `failed`.
fn write_if_open(
    ctx: &mut Ctx<'_>,
    plan: &ValidPlan,
    code: ReasonCode,
    what: &str,
    f: impl FnOnce(&Tx<'_>) -> Result<(), StoreError>,
) -> Result<(), Stop> {
    let mut closed = None;
    ctx.store
        .write(|tx| {
            closed = not_open(&tx.reader())?;
            match closed {
                Some(_) => Ok(()),
                None => f(tx),
            }
        })
        .map_err(|e| Stop::fail(code, format!("{what}: {e}")))?;
    match closed {
        Some(state) => Err(team_closed(plan, code, state)),
        None => Ok(()),
    }
}

fn team_closed(plan: &ValidPlan, code: ReasonCode, state: TeamState) -> Stop {
    Stop::refuse(
        code,
        format!(
            "team_closed: team {} is {state}; the member was not started",
            plan.spec.team
        ),
    )
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
