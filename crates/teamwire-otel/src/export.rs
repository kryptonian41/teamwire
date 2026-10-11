//! `export_once`: claim a batch, send it, commit the mark (ADR 0005
//! point 3). No store transaction is open while the HTTP calls run.

use std::time::Duration;

use teamwire_core::Pid;
use teamwire_core::platform::ProcessRef;
use teamwire_store::{Ctx, ExportClaim, ExportHolder};

use crate::history::History;
use crate::observe::{Observation, gauges, observe};
use crate::otlp::{self, BUCKET_MS};
use crate::spans::{spans_ending, trace_id};
use crate::{ExportError, ExportReport, OtlpConfig, defs};

/// A claim this old, whose exporter is gone, may be taken over.
const CLAIM_STALE_AFTER: Duration = Duration::from_secs(5 * 60);

#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "a method path does not satisfy `Store::read`'s closure bound: it takes a reference to a type with its own lifetime"
)]
/// Sends the events after the export mark, then moves the mark. Nothing
/// new to send is `Ok` with an empty report; a live claim held by another
/// exporter is `Claimed` and nothing is sent.
pub fn export_once(ctx: &mut Ctx<'_>, exporter: &OtlpConfig) -> Result<ExportReport, ExportError> {
    let now = ctx.clock.now();
    let platform = ctx.platform;
    let own_pid = Pid::new(std::process::id());
    let claimer = ExportHolder {
        id: own_pid.to_string(),
        started: platform.process_started(own_pid),
    };
    let me = claimer.id.clone();
    let claim = ctx.store.write(|tx| {
        // Checked in the claiming transaction, so no event can arrive between
        // the check and the claim. A batch that ends inside a bucket would send
        // that bucket's point again when the next batch adds to it (ADR 0005
        // point 4), so the batch waits until the last bucket in it is over.
        let reader = tx.reader();
        let mark = reader.export_mark()?;
        let open_bucket = reader
            .events_after(mark.last_event_seq, None, None)?
            .items
            .iter()
            .any(|e| {
                e.time.unix_millis().div_euclid(BUCKET_MS) * BUCKET_MS + BUCKET_MS
                    > now.unix_millis()
            });
        if open_bucket && mark.claimed_to.is_none() {
            return Ok(None);
        }
        // A holder whose pid now belongs to a process that started at
        // another time is gone: its pid was reused.
        tx.claim_metric_export(now, claimer.clone(), CLAIM_STALE_AFTER, |holder| {
            holder.id.parse::<u32>().map_or(true, |pid| {
                !platform.process_alive(ProcessRef {
                    pid: Pid::new(pid),
                    started: holder.started,
                })
            })
        })
    })?;
    let Some(claim) = claim else {
        let mark = ctx.store.read(|tx| tx.export_mark())?;
        return match mark.claimed_by {
            Some(holder) if mark.claimed_to.is_some() => Err(ExportError::Claimed(format!(
                "pid {holder} holds the batch up to event {}",
                mark.claimed_to.unwrap_or(0)
            ))),
            _ => Ok(ExportReport {
                from_seq: mark.last_event_seq,
                to_seq: mark.last_event_seq,
                metric_points: 0,
                spans: 0,
            }),
        };
    };
    match send_batch(ctx, exporter, &claim) {
        Ok(report) => {
            match ctx
                .store
                .write(|tx| tx.commit_metric_export(&me, claim.to_seq))
            {
                Ok(true) => Ok(report),
                Ok(false) => Err(ExportError::Claimed(
                    "the claim was taken over while the batch was sent".to_owned(),
                )),
                Err(e) => {
                    // Without this the claim would stay with this live pid
                    // until it exits, and every later export would be refused.
                    release(ctx, &me);
                    Err(e.into())
                }
            }
        }
        Err(e) => {
            // The mark stays. Dropping the claim lets the next run retry at once.
            release(ctx, &me);
            Err(e)
        }
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "a batch of events is far below 2^52"
)]
fn send_batch(
    ctx: &Ctx<'_>,
    exporter: &OtlpConfig,
    claim: &ExportClaim,
) -> Result<ExportReport, ExportError> {
    let (team, listing) = ctx
        .store
        .read(|tx| Ok((tx.team()?, tx.events_after(0, None, None)?)))?;
    if !listing.bad.is_empty() {
        tracing::warn!(rows = listing.bad.len(), "export skipped unreadable events");
    }
    let (team_name, team_ulid) = team.map_or_else(
        || ("unknown".to_owned(), String::new()),
        |t| (t.name.as_str().to_owned(), t.ulid.to_string()),
    );
    let history = History::new(listing.items);
    let batch: Vec<_> = history
        .events()
        .iter()
        .filter(|e| e.seq > claim.from_seq && e.seq <= claim.to_seq)
        .collect();

    let mut observations: Vec<Observation> = Vec::new();
    let mut spans = Vec::new();
    for e in &batch {
        observations.extend(observe(&history, e, &team_name));
        spans.extend(spans_ending(&history, e, &team_name, &team_ulid));
    }
    if let Some(last) = batch.last() {
        observations.extend(gauges(&history, claim.to_seq, last.time, &team_name));
        observations.push(Observation {
            def: &defs::EXPORT_LAG,
            labels: Vec::new(),
            value: batch.len() as f64,
            time: last.time,
        });
    }

    // Spans go first. A resent span has the same ids, so the backend keeps
    // one copy; a resent metric point would count twice.
    if !spans.is_empty() {
        let request = otlp::traces_request(&team_name, trace_id(&team_ulid), &spans);
        otlp::post(exporter, "v1/traces", &request)?;
    }
    let (request, points) = otlp::metrics_request(&team_name, &observations);
    if points > 0 {
        otlp::post(exporter, "v1/metrics", &request)?;
    }
    Ok(ExportReport {
        from_seq: claim.from_seq,
        to_seq: claim.to_seq,
        metric_points: points,
        spans: spans.len() as u64,
    })
}

fn release(ctx: &mut Ctx<'_>, me: &str) {
    if let Err(e) = ctx.store.write(|tx| tx.release_metric_claim(me)) {
        tracing::warn!(error = %e, "could not release the export claim");
    }
}
