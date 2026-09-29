//! `done`: store a completion, and for a harness with no turn-end hook tell
//! the member about report gaps once first (route B of ADR 0007).

use serde_json::json;
use teamwire_core::{CompletionStatus, FinalSource, MemberState, ReportGaps, Role, TeamState};
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
    if summary.is_empty() {
        return Err(invalid("summary", "the summary is empty"));
    }
    if summary.chars().count() > SUMMARY_MAX {
        return Err(invalid(
            "summary",
            &format!("the summary is over {SUMMARY_MAX} characters"),
        ));
    }
    let summary = teamwire_core::one_line(summary, SUMMARY_MAX).into_string();

    let member = ctx
        .store
        .read(|tx| tx.member(me))?
        .filter(|m| m.state != MemberState::Removed)
        .ok_or_else(|| DoneError::UnknownRole(me.clone()))?;

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
            tx.hold_member(
                at,
                me,
                current.generation,
                &json!({ "gaps": &gaps, "unread": [], "via": "done" }),
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

fn invalid(field: &str, message: &str) -> DoneError {
    DoneError::Invalid {
        field: field.to_owned(),
        message: message.to_owned(),
    }
}
