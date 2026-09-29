//! What the watch can see of one member from outside: only read calls, to the
//! member's own harness and host (INV-8).

use std::collections::HashMap;
use std::time::Duration;

use teamwire_core::{Liveness, Member, PlaceState};
use teamwire_store::{Ctx, Party};

/// Every call the watch makes to a harness or host is bounded.
const CALL_TIMEOUT: Duration = Duration::from_secs(5);

/// What a member is doing right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Activity {
    Working,
    Idle,
    Blocked,
}

/// One look at one member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Seen {
    Alive(Activity),
    /// The harness says the session is gone, and the host's own listing is
    /// non-empty, so the listing worked (INV-21).
    Absent,
    /// A failed call, an unknown answer, or a listing that could not be
    /// trusted. Never counts for or against the member (WAT-4).
    NoInfo,
}

/// Which hosts returned a usable listing during this poll. One listing per
/// host per poll, however many members it serves.
#[derive(Debug, Default)]
pub(crate) struct Listings {
    non_empty: HashMap<String, bool>,
}

impl Listings {
    fn usable(&mut self, host: &dyn teamwire_core::Host) -> bool {
        let id = host.id().as_str().to_owned();
        *self.non_empty.entry(id).or_insert_with(|| {
            host.list(CALL_TIMEOUT)
                .is_ok_and(|places| !places.is_empty())
        })
    }
}

fn activity(state: PlaceState) -> Option<Activity> {
    match state {
        PlaceState::Working => Some(Activity::Working),
        PlaceState::Idle => Some(Activity::Idle),
        PlaceState::Blocked => Some(Activity::Blocked),
        PlaceState::Gone | PlaceState::Unknown => None,
    }
}

pub(crate) fn observe(ctx: &Ctx<'_>, member: &Member, listings: &mut Listings) -> Seen {
    let Some(session) = &member.session_id else {
        return Seen::NoInfo;
    };
    let Ok((harness, host)) = ctx.adapters(&Party::Member(member.role.clone())) else {
        return Seen::NoInfo;
    };
    let liveness = match harness.is_alive(session, CALL_TIMEOUT) {
        Ok(Liveness::Alive(PlaceState::Gone) | Liveness::Dead) => {
            return if listings.usable(host) {
                Seen::Absent
            } else {
                Seen::NoInfo
            };
        }
        Ok(Liveness::Alive(state)) => activity(state),
        Ok(Liveness::Unknown) | Err(_) => return Seen::NoInfo,
    };
    // A dialog is only visible to the host on some harnesses, and a host
    // that says the place is working is news when the harness says idle.
    let hosted = member
        .place
        .as_ref()
        .filter(|_| host.capabilities().reports_state)
        .and_then(|place| host.state(place, CALL_TIMEOUT).ok())
        .and_then(activity);
    match (liveness, hosted) {
        (_, Some(Activity::Blocked)) => Seen::Alive(Activity::Blocked),
        (Some(Activity::Idle) | None, Some(Activity::Working)) => Seen::Alive(Activity::Working),
        (Some(seen), _) => Seen::Alive(seen),
        (None, _) => Seen::NoInfo,
    }
}
