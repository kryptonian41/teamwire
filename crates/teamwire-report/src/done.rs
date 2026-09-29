//! `done`: store a completion, and for a harness with no turn-end hook tell
//! the member about report gaps once first (route B of ADR 0007).

use serde_json::json;
use sha2::{Digest, Sha256};
use teamwire_core::{
    CompletionStatus, EventType, FinalSource, MemberState, ReportGaps, Role, TeamName, TeamState,
};
use teamwire_store::{Ctx, NewCompletion, Party, StoreError};

use crate::note::gap_note;
use crate::{DoneError, DoneReport, report_gaps};

const SUMMARY_MAX: usize = 2000;

/// Reads a status word. Anything but `done`, `blocked` or `failed` is
/// `invalid.status` and stores nothing (REP-1).
pub fn parse_status(text: &str) -> Result<CompletionStatus, DoneError> {
    CompletionStatus::parse(text).map_err(|_| DoneError::Invalid {
        field: "status".to_owned(),
        message: format!("{text:?} is not one of done, blocked, failed"),
    })
}

/// `done` for a caller that has the status as text: an unknown status is
/// refused, recorded as `member.completion_refused`, and nothing is stored.
pub fn done_with_status(
    ctx: &mut Ctx<'_>,
    me: &Role,
    status: &str,
    summary: &str,
) -> Result<DoneReport, DoneError> {
    match parse_status(status) {
        Ok(status) => done(ctx, me, status, summary),
        Err(e) => {
            record_refusal(ctx, me, &e, &json!({}));
            Err(e)
        }
    }
}

/// Writes `member.completion_refused` for a member that exists. The event is
/// a record for the metrics: a failure to write it changes nothing else.
fn record_refusal(ctx: &mut Ctx<'_>, me: &Role, why: &DoneError, extra: &serde_json::Value) {
    let DoneError::Invalid { field, message } = why else {
        return;
    };
    let live = ctx
        .store
        .read(|tx| tx.member(me))
        .ok()
        .flatten()
        .is_some_and(|m| m.state != MemberState::Removed);
    if !live {
        return;
    }
    let at = ctx.clock.now();
    let mut data = json!({ "reason": format!("invalid.{field}"), "message": message });
    if let (Some(data), Some(extra)) = (data.as_object_mut(), extra.as_object()) {
        data.extend(extra.clone());
    }
    let written = ctx.store.write(|tx| {
        tx.record_event(
            at,
            EventType::MemberCompletionRefused,
            Some(me),
            None,
            &data,
        )
        .map(|_| ())
    });
    if let Err(e) = written {
        tracing::warn!(error = %e, "could not record the refused completion");
    }
}

/// The SHA-256 the store keeps of a member's token (ADR 0008 point 1).
pub fn hash_token(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

/// True when `token` is the token launch gave `role` in team `team`.
///
/// Refused, with no reason given: a team that is not this store's, a member
/// that was removed, a member with no stored hash, a token whose hash
/// differs (ADR 0008 point 2). The hashes are compared without stopping at
/// the first difference.
#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
)]
pub fn token_matches(ctx: &Ctx<'_>, team: &TeamName, role: &Role, token: &str) -> bool {
    let Ok(Some(row)) = ctx.store.read(|tx| tx.team()) else {
        return false;
    };
    if row.name != *team {
        return false;
    }
    let live = matches!(
        ctx.store.read(|tx| tx.member(role)),
        Ok(Some(m)) if m.state != MemberState::Removed
    );
    let stored = ctx
        .store
        .read(|tx| tx.member_token_hash(role))
        .ok()
        .flatten();
    let given = hash_token(token);
    let same = stored.is_some_and(|stored| {
        stored
            .iter()
            .zip(given.iter())
            .fold(0_u8, |diff, (a, b)| diff | (a ^ b))
            == 0
    });
    live && same
}

/// What the write transaction decided.
enum Stored {
    Refused(DoneError),
    Done(DoneReport),
}

/// Records a completion for `me`.
pub fn done(
    ctx: &mut Ctx<'_>,
    me: &Role,
    status: CompletionStatus,
    summary: &str,
) -> Result<DoneReport, DoneError> {
    let summary = summary.trim();
    let member = ctx
        .store
        .read(|tx| tx.member(me))?
        .filter(|m| m.state != MemberState::Removed)
        .ok_or_else(|| DoneError::UnknownRole(me.clone()))?;
    if let Err(e) = check_summary(summary) {
        record_refusal(ctx, me, &e, &json!({}));
        return Err(e);
    }
    let summary = teamwire_core::one_line(summary, SUMMARY_MAX).into_string();

    // A harness with no turn-end hook has nothing to report at stop time, so
    // the gap check runs here, once per generation. A harness this binary
    // does not have is treated as having a hook: the store row is the record.
    let has_hook = ctx
        .adapters(&Party::Member(me.clone()))
        .map_or(true, |(harness, _)| harness.capabilities().turn_end_hook);
    let gaps = if has_hook {
        ReportGaps::default()
    } else {
        report_gaps(ctx, &member)
    };
    let report_file = crate::paths::report_file(ctx, me);

    let at = ctx.clock.now();
    let stored = ctx.store.write(|tx| {
        let team = tx
            .reader()
            .team()?
            .ok_or_else(|| StoreError::Constraint("the store has no team row".to_owned()))?;
        if team.state != TeamState::Open {
            return Ok(Stored::Refused(DoneError::TeamClosed));
        }
        let Some(current) = tx.reader().member(me)? else {
            return Ok(Stored::Refused(DoneError::UnknownRole(me.clone())));
        };
        if current.state == MemberState::Removed {
            return Ok(Stored::Refused(DoneError::UnknownRole(me.clone())));
        }
        if !has_hook && !gaps.is_empty() && current.gate_told_generation != Some(current.generation)
        {
            tx.set_gate_told_generation(me, current.generation)?;
            tx.record_event(
                at,
                EventType::MemberCompletionRefused,
                Some(me),
                None,
                &json!({ "reason": "report_gaps", "gaps": &gaps }),
            )?;
            return Ok(Stored::Done(DoneReport::NotStored {
                stored: false,
                gaps: gaps.clone(),
                report: format!(
                    "{} The second call is stored whatever the report says.",
                    gap_note(&gaps, &report_file)
                ),
            }));
        }
        let completion = tx.insert_completion(
            at,
            &NewCompletion {
                role: me.clone(),
                generation: current.generation,
                status,
                summary: summary.clone(),
                final_message: None,
                final_source: FinalSource::None,
            },
        )?;
        Ok(Stored::Done(DoneReport::Stored {
            completion: completion.ulid,
            n: completion.n,
            generation: completion.generation,
            status,
            report: if has_hook {
                "The result goes to the lead when this turn ends.".to_owned()
            } else {
                "The team watch tells the lead; this harness has no turn-end hook, so the final message is not part of the result.".to_owned()
            },
        }))
    })?;
    match stored {
        Stored::Refused(e) => Err(e),
        Stored::Done(report) => Ok(report),
    }
}

fn check_summary(summary: &str) -> Result<(), DoneError> {
    if summary.is_empty() {
        return Err(invalid("summary", "the summary is empty"));
    }
    if summary.chars().count() > SUMMARY_MAX {
        return Err(invalid(
            "summary",
            &format!("the summary is over {SUMMARY_MAX} characters"),
        ));
    }
    Ok(())
}

fn invalid(field: &str, message: &str) -> DoneError {
    DoneError::Invalid {
        field: field.to_owned(),
        message: message.to_owned(),
    }
}
