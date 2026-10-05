//! Route A of ADR 0007: what the member's Stop hook does when a turn ends.

use std::time::Duration;

use serde_json::json;
use teamwire_core::{
    LeadRoute, Member, MemberState, ReportState, Role, Route, TurnEnd, TurnEndReply,
};
use teamwire_deliver::{DeliveryOutcome, deliver_line, deliver_line_within, result_line};
use teamwire_store::{CompletionRecord, Ctx, Party, ResultOutcome, StoreError};

use crate::final_message;
use crate::note::{gap_note, unread_note};
use crate::paths::{report_file, team_dir};
use crate::reminder::unread;
use crate::result_file::{MISSING_FROM_HARNESS, write_current};

/// The name a hook run gives its events (`docs/metrics.md` section 5).
const HOOK: &str = "claude-stop";

/// Route A of ADR 0007: hold once for gaps or unread messages, else write
/// the result and deliver it.
///
/// Never fails: every problem is logged and the turn ends normally.
pub fn on_turn_end(ctx: &mut Ctx<'_>, me: &Role, t: &TurnEnd) -> TurnEndReply {
    let started = ctx.clock.monotonic();
    match run(ctx, me, t, started) {
        Ok(reply) => reply,
        Err(e) => {
            tracing::warn!(error = %e, role = %me, "the turn-end hook could not finish");
            TurnEndReply::Continue
        }
    }
}

/// `started + budget`, or `None` when that is beyond what an `Instant` can
/// hold: adding would panic, and a hook must always exit 0.
fn deadline_after(started: std::time::Instant, budget: Duration) -> Option<std::time::Instant> {
    started.checked_add(budget)
}

fn hook_data(ctx: &Ctx<'_>, started: std::time::Instant) -> serde_json::Value {
    json!({ "hook": HOOK, "hook_ms": hook_ms(ctx, started) })
}

fn hook_ms(ctx: &Ctx<'_>, started: std::time::Instant) -> u64 {
    let ms = ctx
        .clock
        .monotonic()
        .saturating_duration_since(started)
        .as_millis();
    u64::try_from(ms).unwrap_or(u64::MAX)
}

fn run(
    ctx: &mut Ctx<'_>,
    me: &Role,
    t: &TurnEnd,
    started: std::time::Instant,
) -> Result<TurnEndReply, StoreError> {
    // A subagent, or a session that only paused for a background task, has
    // not finished its work (REP-10).
    if t.subagent {
        return Ok(TurnEndReply::Continue);
    }
    // The member is matched by session id and team identity together
    // (REP-11): the store is this team's, and the session must be the recorded one.
    let Some(member) = ctx
        .store
        .read(|tx| tx.member(me))?
        .filter(|m| m.state != MemberState::Removed)
        .filter(|m| m.session_id.as_ref() == Some(&t.session))
    else {
        return Ok(TurnEndReply::Continue);
    };
    let latest = ctx
        .store
        .read(|tx| tx.latest_completion(me))?
        .filter(|r| r.completion.generation == member.generation);

    let mut check_error = None;
    if !t.hold_already_active {
        let (note, error) = hold(ctx, &member, latest.as_ref(), started)?;
        if let Some(note) = note {
            return Ok(TurnEndReply::Hold(note));
        }
        check_error = error;
    }

    // Only a completion nobody delivered yet is delivered. `delivering` is a
    // crash left over: it is delivered once more (REP-2).
    if let Some(record) = latest {
        match record.completion.report_state {
            ReportState::Pending | ReportState::Delivering => {
                deliver(ctx, &member, record, t, started, check_error.as_deref())?;
            }
            ReportState::Delivered
                if record.completion.route == Some(Route::Watch)
                    && record.completion.final_message.is_none() =>
            {
                late_final(ctx, &member, &record, t)?;
            }
            ReportState::Delivered | ReportState::Undelivered | ReportState::Unconfirmed => {}
        }
    }
    Ok(TurnEndReply::Continue)
}

/// A completion the watch already reported (route B) has no final message
/// yet. When the member's turn ends, record it and rewrite the result file.
/// Nothing is typed: the watch printed its line.
fn late_final(
    ctx: &mut Ctx<'_>,
    member: &Member,
    record: &CompletionRecord,
    t: &TurnEnd,
) -> Result<(), StoreError> {
    let (text, source) = final_message::pick(t);
    if text.is_none() {
        return Ok(());
    }
    let ulid = record.completion.ulid;
    ctx.store.write(|tx| {
        tx.set_completion_final(&ulid, text.as_deref(), source)
            .map(|_| ())
    })?;
    if let Err(e) = write_current(ctx, member, &ulid, MISSING_FROM_HARNESS) {
        tracing::warn!(error = %e, role = %member.role, "could not rewrite the result file");
    }
    Ok(())
}

/// The one hold a stop may get: report gaps (once per generation) and unread
/// messages (once per message), in one note.
fn hold(
    ctx: &mut Ctx<'_>,
    member: &Member,
    latest: Option<&CompletionRecord>,
    started: std::time::Instant,
) -> Result<(Option<String>, Option<String>), StoreError> {
    // A `delivering` completion already passed the check before the crash.
    let check_report = latest.is_some_and(|r| r.completion.report_state == ReportState::Pending)
        && member.gate_told_generation != Some(member.generation);
    let (gaps, check_error) = if check_report {
        crate::check_report(ctx, member)
    } else {
        (teamwire_core::ReportGaps::default(), None)
    };
    let told = !gaps.is_empty();
    let unread = unread(ctx, &member.role)?;
    if !told && unread.is_empty() {
        return Ok((None, check_error));
    }
    let ids: Vec<String> = unread.iter().map(|u| u.display_id.clone()).collect();
    let mut parts = Vec::new();
    if told {
        parts.push(gap_note(&gaps, &report_file(ctx, &member.role)));
    }
    if !ids.is_empty() {
        parts.push(unread_note(&ids));
    }
    let at = ctx.clock.now();
    let mut data = json!({
        "hook": HOOK,
        "hook_ms": hook_ms(ctx, started),
        "gaps": gaps,
        "unread": ids,
    });
    if let (Some(error), Some(fields)) = (&check_error, data.as_object_mut()) {
        fields.insert("report_check_error".to_owned(), json!(error));
    }
    let recipient = teamwire_core::Recipient::Member(member.role.clone());
    ctx.store.write(|tx| {
        if told {
            tx.hold_member(at, &member.role, member.generation, &data)?;
        } else {
            tx.hold_member_unread(at, &member.role, &data)?;
        }
        for u in &unread {
            // A message read since the list was made is left alone.
            tx.mark_reminded(at, &u.message, &recipient).ok();
        }
        Ok(())
    })?;
    Ok((Some(parts.join(" ")), check_error))
}

fn deliver(
    ctx: &mut Ctx<'_>,
    member: &Member,
    record: CompletionRecord,
    t: &TurnEnd,
    started: std::time::Instant,
    check_error: Option<&str>,
) -> Result<(), StoreError> {
    let ulid = record.completion.ulid;
    // The final message is recorded at stop time, from the hook first. A
    // crash left-over keeps the message it already has when this stop has none.
    let (text, source) = final_message::pick(t);
    let (text, source) = if text.is_some() {
        (text, source)
    } else {
        (
            record.completion.final_message.clone(),
            record.completion.final_source,
        )
    };
    ctx.store.write(|tx| {
        tx.set_completion_final(&ulid, text.as_deref(), source)
            .map(|_| ())
    })?;
    let record = ctx.store.read(|tx| tx.completion(&ulid))?.unwrap_or(record);

    #[expect(
        clippy::redundant_closure_for_method_calls,
        reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
    )]
    let team = ctx.store.read(|tx| tx.team())?;
    let Some(team) = team else {
        return Ok(());
    };
    let Some(dir) = team_dir(ctx) else {
        return Ok(());
    };
    let written = match write_current(ctx, member, &ulid, MISSING_FROM_HARNESS) {
        Ok(w) => Some(w),
        Err(e) => {
            // The lead still hears about the result; the summary is in the line.
            tracing::warn!(error = %e, role = %member.role, "could not write the result file");
            None
        }
    };

    // No host for the lead: the watch prints the result (DEL-17).
    if team.lead_route == LeadRoute::Watch {
        return Ok(());
    }

    // Take the delivery. No write transaction is open while the host works.
    // A Stop hook blocks the next stop of the same session, so two hooks of
    // one member never overlap: a `delivering` row seen here is a crash left
    // over, at any age, and is delivered once more (REP-2).
    let stale = Duration::ZERO;
    let claimed = ctx
        .store
        .write(|tx| tx.claim_completion_for_delivery(ctx.clock.now(), &ulid, stale))?;
    if claimed.is_none() {
        return Ok(());
    }
    let path = written.as_ref().map_or_else(
        || crate::result_file::path_of(&dir, member),
        |w| w.path.clone(),
    );
    let c = &record.completion;
    let line = result_line(&team.name, &c.role, c.status, &c.summary, &path);
    let outcome = match deadline_after(started, Duration::from_secs(ctx.config.hook_budget_s)) {
        Some(deadline) => deliver_line_within(ctx, &Party::Lead, &line, deadline),
        // A budget too large for the clock is no deadline at all.
        None => deliver_line(ctx, &Party::Lead, &line),
    };
    let result = match outcome {
        DeliveryOutcome::HandedOff => ResultOutcome {
            state: ReportState::Delivered,
            route: Some(Route::HostTyping),
            error: None,
            result_sha256: written.as_ref().map(|w| w.sha256),
        },
        DeliveryOutcome::Unconfirmed(e) => ResultOutcome {
            state: ReportState::Unconfirmed,
            route: Some(Route::HostTyping),
            error: Some(e),
            result_sha256: written.as_ref().map(|w| w.sha256),
        },
        DeliveryOutcome::Undelivered(e) => ResultOutcome {
            state: ReportState::Undelivered,
            route: None,
            error: Some(e),
            result_sha256: written.as_ref().map(|w| w.sha256),
        },
    };
    let mut extra = hook_data(ctx, started);
    if let (Some(error), Some(data)) = (check_error, extra.as_object_mut()) {
        data.insert("report_check_error".to_owned(), json!(error));
    }
    let at = ctx.clock.now();
    ctx.store.write(|tx| {
        tx.finish_completion_delivery_with(at, &ulid, &result, &extra)
            .map(|_| ())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_budget_too_large_for_the_clock_is_no_deadline() {
        let now = std::time::Instant::now();
        assert_eq!(deadline_after(now, Duration::MAX), None);
        assert_eq!(
            deadline_after(now, Duration::from_secs(40)),
            Some(now + Duration::from_secs(40))
        );
    }
}
