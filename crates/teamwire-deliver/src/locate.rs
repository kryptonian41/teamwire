//! The locator: which place holds a party's session, from two views.

use std::time::Duration;

use teamwire_core::{
    DeliveryError, DeliveryErrorCode, Harness, Host, HostErrorCode, Pid, Place, SessionId,
    SessionKind, one_line,
};
use teamwire_store::{Ctx, Party, StoreError};

use crate::budget::Budget;

/// The most characters of a host's error text kept in a delivery error.
const FAILED_DETAIL_CHARS: usize = 200;

/// The place two views agreed on, with the evidence the second check needs.
pub(crate) struct Located {
    pub(crate) place: Place,
    pub(crate) session: SessionId,
    pub(crate) pids: Vec<Pid>,
}

/// What the host says about one place.
enum HostView {
    Confirms,
    OtherSession(String),
    Nothing(String),
    Failed(String),
}

/// The place where `party`'s session runs now, agreed by two views.
///
/// The harness view lists the live processes of the session and reads the
/// place variable of each. The host view says who holds each of those
/// places. The place is returned only when exactly one place is named and
/// confirmed. The pid and place recorded at launch are never evidence.
/// No store transaction is open while the adapters are called.
pub fn locate(ctx: &Ctx<'_>, party: &Party) -> Result<Place, DeliveryError> {
    let (harness, host) = ctx.adapters(party)?;
    let budget = Budget::new(ctx.clock, None);
    locate_with(ctx, party, harness, host, &budget).map(|found| found.place)
}

pub(crate) fn locate_with(
    ctx: &Ctx<'_>,
    party: &Party,
    harness: &dyn Harness,
    host: &dyn Host,
    budget: &Budget<'_>,
) -> Result<Located, DeliveryError> {
    let session = recorded_session(ctx, party)?;
    let Some(place_var) = host.place_env_var() else {
        return Err(DeliveryError::new(
            DeliveryErrorCode::SingleSource,
            format!(
                "host {} gives no place variable, so only the host view exists",
                host.id()
            ),
        ));
    };
    let pids = harness
        .processes_for_session(&session, budget.call()?)
        .map_err(|e| {
            DeliveryError::new(
                DeliveryErrorCode::NoLiveProcess,
                format!("the harness could not list session {session}: {e}"),
            )
        })?;
    if pids.is_empty() {
        return Err(DeliveryError::new(
            DeliveryErrorCode::NoLiveProcess,
            format!("no live process runs session {session}"),
        ));
    }
    let candidates = candidate_places(ctx, host, &pids, place_var)?;

    let mut confirmed = Vec::new();
    let mut other = None;
    let mut nothing = None;
    let mut failed = None;
    for place in candidates {
        match host_view(host, &place, &session, &pids, budget.call()?) {
            HostView::Confirms => confirmed.push(place),
            HostView::OtherSession(d) => other = other.or(Some(d)),
            HostView::Nothing(d) => nothing = nothing.or(Some(d)),
            HostView::Failed(d) => failed = failed.or(Some(d)),
        }
    }
    // A place the host could not answer for may hold the session too, so a
    // failed listing refuses even when another place is confirmed.
    if let Some(detail) = failed {
        return Err(DeliveryError::new(
            DeliveryErrorCode::HostListingFailed,
            detail,
        ));
    }
    match confirmed.len() {
        1 => Ok(Located {
            place: confirmed.remove(0),
            session,
            pids,
        }),
        0 => Err(other.map_or_else(
            || {
                DeliveryError::new(
                    DeliveryErrorCode::HostNoSession,
                    nothing.unwrap_or_else(|| "the host names no place".to_owned()),
                )
            },
            |detail| DeliveryError::new(DeliveryErrorCode::HostOtherSession, detail),
        )),
        n => Err(DeliveryError::new(
            DeliveryErrorCode::TwoPlaces,
            format!("the host confirms session {session} in {n} places"),
        )),
    }
}

/// The second host check, right before the send (ADR 0006 point 3).
pub(crate) fn recheck(
    host: &dyn Host,
    found: &Located,
    budget: &Budget<'_>,
) -> Result<(), DeliveryError> {
    match host_view(
        host,
        &found.place,
        &found.session,
        &found.pids,
        budget.call()?,
    ) {
        HostView::Confirms => Ok(()),
        HostView::OtherSession(d) => Err(DeliveryError::new(
            DeliveryErrorCode::HostOtherSession,
            format!("the place changed hands before the send: {d}"),
        )),
        HostView::Nothing(d) => Err(DeliveryError::new(
            DeliveryErrorCode::HostNoSession,
            format!("the place emptied before the send: {d}"),
        )),
        HostView::Failed(d) => Err(DeliveryError::new(DeliveryErrorCode::HostListingFailed, d)),
    }
}

#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`ReadTx::team` as a path does not infer the transaction's lifetime for `Store::read`"
)]
fn recorded_session(ctx: &Ctx<'_>, party: &Party) -> Result<SessionId, DeliveryError> {
    let unreadable = |e: StoreError| {
        DeliveryError::new(
            DeliveryErrorCode::NoLiveProcess,
            format!("the recorded session could not be read: {e}"),
        )
    };
    match party {
        Party::Lead => {
            let team = ctx
                .store
                .read(|tx| tx.team())
                .map_err(unreadable)?
                .ok_or_else(|| {
                    DeliveryError::new(DeliveryErrorCode::NoLiveProcess, "the team has no row")
                })?;
            if team.lead_kind == Some(SessionKind::Background) {
                return Err(DeliveryError::new(
                    DeliveryErrorCode::BackgroundSession,
                    "the lead was recorded as a background session",
                ));
            }
            team.lead_session.ok_or_else(|| {
                DeliveryError::new(
                    DeliveryErrorCode::NoLiveProcess,
                    "no lead session is recorded",
                )
            })
        }
        Party::Member(role) => {
            let member = ctx
                .store
                .read(|tx| tx.member(role))
                .map_err(unreadable)?
                .ok_or_else(|| {
                    DeliveryError::new(
                        DeliveryErrorCode::RecipientRemoved,
                        format!("no member {role}"),
                    )
                })?;
            member.session_id.ok_or_else(|| {
                DeliveryError::new(
                    DeliveryErrorCode::NoLiveProcess,
                    format!("member {role} has no recorded session"),
                )
            })
        }
    }
}

/// The distinct places the session's processes name for themselves.
fn candidate_places(
    ctx: &Ctx<'_>,
    host: &dyn Host,
    pids: &[Pid],
    place_var: &str,
) -> Result<Vec<Place>, DeliveryError> {
    let mut places: Vec<Place> = Vec::new();
    let mut unreadable = Vec::new();
    for pid in pids {
        let failure = std::cell::RefCell::new(None);
        let id = host.place_from_env(&|var| match ctx.platform.env_of(*pid, var) {
            Ok(value) => value,
            Err(e) => {
                failure.borrow_mut().get_or_insert(e);
                None
            }
        });
        match (id, failure.into_inner()) {
            (Some(id), _) if !id.is_empty() => {
                let place = Place {
                    host: host.id(),
                    id,
                };
                if !places.contains(&place) {
                    places.push(place);
                }
            }
            (_, Some(e)) => unreadable.push(format!("pid {pid}: {e}")),
            _ => {}
        }
    }
    if places.is_empty() {
        let detail = if unreadable.is_empty() {
            format!("no process of the session has {place_var} set")
        } else {
            format!("no place variable could be read: {}", unreadable.join("; "))
        };
        return Err(DeliveryError::new(DeliveryErrorCode::NoPlace, detail));
    }
    Ok(places)
}

fn host_view(
    host: &dyn Host,
    place: &Place,
    session: &SessionId,
    pids: &[Pid],
    timeout: Duration,
) -> HostView {
    match host.occupant(place, timeout) {
        Ok(occupant) => match occupant.session {
            Some(held) if held == *session => HostView::Confirms,
            Some(held) => {
                HostView::OtherSession(format!("place {} holds session {held}", place.id))
            }
            None if occupant.pids.is_empty() => {
                HostView::Nothing(format!("place {} holds nothing", place.id))
            }
            None if occupant.pids.iter().any(|p| pids.contains(p)) => HostView::Confirms,
            None => HostView::OtherSession(format!(
                "place {} holds other processes than session {session}",
                place.id
            )),
        },
        Err(e) if e.code == HostErrorCode::NotFound => {
            HostView::Nothing(format!("the host does not list place {}", place.id))
        }
        // The detail is stored and shown to the lead, and a host error can
        // hold several lines of command output.
        Err(e) => HostView::Failed(
            one_line(
                &format!("the host could not say who holds {}: {e}", place.id),
                FAILED_DETAIL_CHARS,
            )
            .into_string(),
        ),
    }
}
