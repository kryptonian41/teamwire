//! `teamwire status`: the same reading of each member as the watch, without
//! any memory of earlier polls, and without a single write (WAT-11).

use std::path::PathBuf;
use std::time::Duration;

use teamwire_core::{
    DeliveryState, Liveness, Member, MemberState, Recipient, ReportState, Role, SessionId,
    Timestamp, one_line,
};
use teamwire_deliver::{AttemptHistory, watch_lease_is_dead};
use teamwire_store::{
    CompletionRecord, Ctx, Party, ReadTx, StoreError, WRITER_VERSION, database_path,
};

use crate::observe::{Activity, Listings, Seen, observe};
use crate::text::result_path;
use crate::{
    LeadView, MemberView, MessageView, StatusView, StuckDelivery, UNREAD_AFTER, UnansweredQuestion,
    UndeliveredResult, UnreadDelivery, WatchState, WatchView,
};

/// The longest message excerpt status shows.
const EXCERPT: usize = 80;

/// A member with its completion, run start, the version that started it, and
/// whether it was started without a first prompt and never reprompted.
type MemberRow = (
    Member,
    Option<CompletionRecord>,
    Timestamp,
    Option<String>,
    bool,
    Option<u64>,
);

/// The rows status shows, read in one transaction.
struct Rows {
    view: StatusView,
    members: Vec<MemberRow>,
    team_dir: PathBuf,
}

fn seconds(span: Duration) -> u64 {
    span.as_secs()
}

fn read_rows(tx: &ReadTx<'_>, ctx: &Ctx<'_>, now: Timestamp) -> Result<Rows, StoreError> {
    let mut view = StatusView::default();
    let Some(team) = tx.team()? else {
        view.errors.push("the team has no record".to_owned());
        return Ok(Rows {
            view,
            members: Vec::new(),
            team_dir: PathBuf::new(),
        });
    };
    let team_dir = database_path(ctx.home, &team.name)
        .parent()
        .map(PathBuf::from)
        .unwrap_or_default();
    view.team = Some(team.name.clone());
    view.title = Some(team.title.clone());
    view.state = Some(team.state);
    view.afk = team
        .afk_budget_ends_at
        .map(|ends| crate::afk::view(&team_dir, ends, now));
    view.lead = Some(LeadView {
        session_id: team.lead_session.clone(),
        liveness: Liveness::Unknown,
    });

    view.watch = match tx.watch_lease()? {
        None => WatchView::default(),
        Some(lease) => WatchView {
            state: if watch_lease_is_dead(&lease, ctx.platform, now) {
                WatchState::Stale
            } else {
                WatchState::Running
            },
            pid: Some(lease.pid),
            heartbeat_age_s: Some(seconds(now.saturating_since(lease.heartbeat_at))),
        },
    };

    let mut members = Vec::new();
    for member in tx.members()?.all_or_corrupt()? {
        if member.state == MemberState::Removed {
            continue;
        }
        let events = tx.events_of_role(&member.role)?.all_or_corrupt()?;
        let started = events
            .iter()
            .rev()
            .find(|e| e.kind == teamwire_core::EventType::MemberStarted);
        let unprompted = is_unprompted(started, &events);
        let run_start = crate::snapshot::run_start(&member, started.map(|e| e.time));
        let completion = tx
            .latest_completion(&member.role)?
            .filter(|c| c.completion.generation == member.generation);
        let configured = tx.context_handoff_tokens(&member.role)?;
        members.push((
            member,
            completion,
            run_start,
            started.map(|e| e.writer_version.clone()),
            unprompted,
            configured,
        ));
    }

    let recent = tx
        .messages(&teamwire_store::MessageFilter {
            limit: Some(5),
            newest: true,
            ..Default::default()
        })?
        .all_or_corrupt()?;
    for m in recent {
        let to = tx
            .deliveries_of(&m.ulid)?
            .all_or_corrupt()?
            .into_iter()
            .map(|d| d.delivery.recipient)
            .collect::<Vec<Recipient>>();
        view.messages.push(MessageView {
            display_id: m.display_id,
            time: m.created_at,
            kind: m.kind,
            from: m.sender,
            to,
            excerpt: one_line(&m.body, EXCERPT).into_string(),
        });
    }

    let stuck = tx
        .deliveries_in_states(&[
            DeliveryState::Pending,
            DeliveryState::Unconfirmed,
            DeliveryState::Undelivered,
        ])?
        .all_or_corrupt()?;
    for record in stuck {
        let d = record.delivery;
        let Some(message) = tx.message(&d.message)? else {
            continue;
        };
        let attempt = AttemptHistory::read(tx, &message, &d.recipient)?;
        if d.state == DeliveryState::Pending && !attempt.is_stale(now) {
            continue;
        }
        let age = now.saturating_since(attempt.started());
        view.stuck_deliveries.push(StuckDelivery {
            display_id: message.display_id,
            to: d.recipient,
            state: d.state,
            age_s: seconds(age),
        });
    }

    for record in tx
        .deliveries_in_states(&[DeliveryState::HandedOff])?
        .all_or_corrupt()?
    {
        let d = record.delivery;
        let Some(message) = tx.message(&d.message)? else {
            continue;
        };
        let Some(since) = AttemptHistory::read(tx, &message, &d.recipient)?.unread_since() else {
            continue;
        };
        let age = now.saturating_since(since);
        if age > UNREAD_AFTER {
            view.unread_deliveries.push(UnreadDelivery {
                display_id: message.display_id,
                to: d.recipient,
                age_s: seconds(age),
            });
        }
    }

    for m in tx.unanswered_questions()?.all_or_corrupt()? {
        let to = tx
            .deliveries_of(&m.ulid)?
            .all_or_corrupt()?
            .into_iter()
            .map(|d| d.delivery.recipient)
            .collect();
        view.unanswered_questions.push(UnansweredQuestion {
            display_id: m.display_id,
            from: m.sender,
            to,
            age_s: seconds(now.saturating_since(m.created_at)),
        });
    }

    for record in tx
        .completions_in_states(&[ReportState::Undelivered, ReportState::Unconfirmed])?
        .all_or_corrupt()?
    {
        let c = record.completion;
        // A newer completion replaces this one as the member's result (ADR
        // 0007 point 2), and its file overwrote this one's (ADR 0024).
        let newest = tx.latest_completion(&c.role)?.map(|r| r.completion.ulid);
        if newest.as_ref() != Some(&c.ulid) {
            continue;
        }
        view.undelivered_results.push(UndeliveredResult {
            path: PathBuf::from(result_path(
                &team_dir,
                &c.role,
                record.result_sha256.as_ref(),
            )),
            role: c.role,
            n: c.n,
            report_state: c.report_state,
        });
    }
    Ok(Rows {
        view,
        members,
        team_dir,
    })
}

/// The word `status` shows for a member, from its records and one look at it.
/// A member started without a first prompt has no startup to wait out, so it
/// reads `quiet` when idle.
fn state_word(
    ctx: &Ctx<'_>,
    member: &Member,
    completion: Option<&CompletionRecord>,
    seen: Seen,
    age: Duration,
    unprompted: bool,
) -> String {
    match member.state {
        MemberState::Creating => return "not started".to_owned(),
        MemberState::Failed => return "failed to start".to_owned(),
        MemberState::Started | MemberState::Removed => {}
    }
    if let Some(record) = completion {
        let c = &record.completion;
        return match c.report_state {
            // A finished member that works again was prompted by someone
            // other than teamwire, which would have started a generation.
            ReportState::Delivered if seen == Seen::Alive(Activity::Working) => {
                "prompted outside teamwire".to_owned()
            }
            ReportState::Delivered => format!("done({})", c.status),
            // Only the member's own hook reports a pending result, so a
            // member gone before it ran is dead, as the watch says.
            ReportState::Pending if seen == Seen::Absent => "died?".to_owned(),
            ReportState::Pending | ReportState::Delivering => "unreported".to_owned(),
            ReportState::Undelivered => "undelivered".to_owned(),
            ReportState::Unconfirmed => "unconfirmed".to_owned(),
        };
    }
    let grace = Duration::from_secs(ctx.config.startup_grace_s);
    match seen {
        Seen::Absent => "died?".to_owned(),
        _ if age > Duration::from_secs(member.deadline_s) => "late".to_owned(),
        Seen::Alive(Activity::Blocked) => "blocked".to_owned(),
        Seen::Alive(Activity::Idle) if age >= grace || unprompted => "quiet".to_owned(),
        Seen::Alive(_) | Seen::NoInfo => "working".to_owned(),
    }
}

/// The command that resumes a member's session with its per-run files. Its
/// settings, MCP configuration and contract are already on disk, so only
/// the path names matter here; the token is not part of the command.
fn resume_command(
    ctx: &Ctx<'_>,
    team_dir: &std::path::Path,
    team: &teamwire_core::TeamName,
    member: &Member,
    session: &SessionId,
) -> Result<String, String> {
    let (harness, _) = ctx
        .adapters(&Party::Member(member.role.clone()))
        .map_err(|e| e.to_string())?;
    let options = ctx
        .store
        .read(|tx| tx.run_options(&member.role))
        .map_err(|e| e.to_string())?;
    let launch = teamwire_core::MemberLaunch::for_resume(
        team_dir,
        team,
        member,
        session,
        (options.model, options.effort),
    );
    let spec = harness
        .resume_spec(&launch, session)
        .map_err(|e| e.to_string())?;
    Ok(teamwire_core::LaunchSpec::shell_line(&spec.argv))
}

/// The status view. `role` adds the resume command of that member. Reads only.
pub fn status_with(ctx: &Ctx<'_>, role: Option<&Role>) -> StatusView {
    let now = ctx.clock.now();
    let rows = match ctx.store.read(|tx| read_rows(tx, ctx, now)) {
        Ok(rows) => rows,
        Err(e) => {
            return StatusView {
                errors: vec![e.to_string()],
                ..StatusView::default()
            };
        }
    };
    let Rows {
        mut view,
        members,
        team_dir,
    } = rows;
    let mut listings = Listings::default();
    for (member, completion, run_start, started_by, unprompted, configured) in &members {
        let seen = observe(ctx, member, &mut listings);
        let age = now.saturating_since(*run_start);
        let resume = match (role, &view.team, &member.session_id) {
            (Some(wanted), Some(team), Some(session)) if *wanted == member.role => {
                match resume_command(ctx, &team_dir, team, member, session) {
                    Ok(command) => Some(command),
                    Err(why) => {
                        view.errors
                            .push(format!("no resume command for {wanted}: {why}"));
                        None
                    }
                }
            }
            (Some(wanted), _, None) if *wanted == member.role => {
                view.errors
                    .push(format!("{wanted} has no session recorded to resume"));
                None
            }
            _ => None,
        };
        view.members.push(MemberView {
            role: member.role.clone(),
            title: member.title.clone(),
            state: state_word(ctx, member, completion.as_ref(), seen, age, *unprompted),
            generation: member.generation,
            age_s: seconds(age),
            deadline_s: member.deadline_s,
            older_build: started_by.as_deref().is_some_and(|v| v != WRITER_VERSION),
            started_by: started_by.clone(),
            result: completion.as_ref().map(|c| c.completion.report_state),
            branch: member.branch.clone(),
            resume_command: resume,
            context: crate::context::read(ctx, member, *configured),
        });
    }
    if let (Some(lead), Ok((harness, _))) = (view.lead.as_mut(), ctx.adapters(&Party::Lead))
        && let Some(session) = &lead.session_id
    {
        lead.liveness = harness
            .is_alive(session, Duration::from_secs(5))
            .unwrap_or(Liveness::Unknown);
    }
    if let Some(wanted) = role
        && !view.members.iter().any(|m| &m.role == wanted)
    {
        view.errors.push(format!("no member {wanted} in this team"));
    }
    view
}

/// Whether the member got no first prompt and nothing since. An older build's
/// `member.started` has no `dispatch` key and keeps the grace period. A
/// handoff counts as a prompt: its new session is about to be given work.
fn is_unprompted(started: Option<&teamwire_core::Event>, events: &[teamwire_core::Event]) -> bool {
    use teamwire_core::EventType;
    started.is_some_and(|s| {
        s.data.get("dispatch") == Some(&serde_json::Value::Bool(false))
            && !events.iter().any(|e| {
                matches!(
                    e.kind,
                    EventType::MemberReprompted | EventType::MemberHandoff
                ) && e.seq > s.seq
            })
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use teamwire_core::{Event, EventType, Timestamp, Ulid};

    use super::is_unprompted;

    fn event(seq: u64, kind: EventType, data: serde_json::Value) -> Event {
        let time = Timestamp::from_unix_millis(1_790_000_000_000).unwrap();
        Event {
            seq,
            ulid: Ulid::from_parts(time, u128::from(seq)),
            time,
            kind,
            role: None,
            subject: None,
            data,
            writer_version: "test".to_owned(),
        }
    }

    #[test]
    fn a_member_without_a_first_prompt_is_unprompted_until_a_reprompt_or_a_handoff() {
        let started = event(1, EventType::MemberStarted, json!({ "dispatch": false }));
        let both = |later: EventType| {
            let events = [started.clone(), event(2, later, json!({}))];
            is_unprompted(Some(&started), &events)
        };
        assert!(is_unprompted(
            Some(&started),
            std::slice::from_ref(&started)
        ));
        assert!(!both(EventType::MemberReprompted));
        assert!(!both(EventType::MemberHandoff));
        assert!(both(EventType::MessageSent));
    }

    #[test]
    fn a_dispatched_member_and_an_older_build_are_not_unprompted() {
        let dispatched = event(1, EventType::MemberStarted, json!({ "dispatch": true }));
        let older = event(1, EventType::MemberStarted, json!({}));
        assert!(!is_unprompted(Some(&dispatched), &[]));
        assert!(!is_unprompted(Some(&older), &[]));
        assert!(!is_unprompted(None, &[]));
    }
}
