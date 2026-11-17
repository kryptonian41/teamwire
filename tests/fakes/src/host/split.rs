//! `FakeHost::split_run` and `FakeHost::close_pane`. A split pane is a place
//! with no name, so `list` and `live_agent_names` leave it out, as herdr and
//! tmux leave out a pane that holds no teamwire agent.

use teamwire_core::{HostError, Occupant, Place, PlaceState, SplitRequest};

use super::FakeHost;
use crate::calls::{CallKind, lock};

/// What a test runs after each successful split, such as starting a fake
/// session in the new pane.
type SplitFn = Box<dyn Fn(&FakeHost, &Place, &SplitRequest) + Send + Sync>;

pub(super) struct SplitHook(SplitFn);

impl std::fmt::Debug for SplitHook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SplitHook")
    }
}

impl FakeHost {
    pub(super) fn split_beside(
        &self,
        beside: &Place,
        req: &SplitRequest,
    ) -> Result<Place, HostError> {
        self.log.record(
            "split_run",
            CallKind::Write,
            format!("{} {}", beside.id, req.label),
        );
        let late_error = self.before("split_run")?;
        if !lock(&self.state).caps.splits_pane {
            return Err(HostError::unsupported());
        }
        self.check_place(beside)?;
        let place = self.add_place(
            "",
            PlaceState::Idle,
            Occupant {
                session: None,
                pids: Vec::new(),
            },
        );
        self.with_rec(&place, |r| {
            r.title.clone_from(&req.label);
            r.cwd.clone_from(&req.cwd);
            r.argv.clone_from(&req.argv);
            r.env.clone_from(&req.env);
            r.split = Some(req.clone());
        });
        if late_error.is_none()
            && let Some(SplitHook(hook)) = &*lock(&self.on_split)
        {
            hook(self, &place, req);
        }
        Self::finish(place, late_error)
    }

    /// Runs `hook` after each split that succeeds, with the new place and
    /// its request, once the host's own state is unlocked. A test starts the
    /// new pane's fake session here, since it learns the place only now.
    pub fn on_split(&self, hook: impl Fn(&Self, &Place, &SplitRequest) + Send + Sync + 'static) {
        *lock(&self.on_split) = Some(SplitHook(Box::new(hook)));
    }

    pub(super) fn close_split(&self, place: &Place) -> Result<(), HostError> {
        self.log
            .record("close_pane", CallKind::Write, place.id.as_str());
        let late_error = self.before("close_pane")?;
        if !lock(&self.state).caps.splits_pane {
            return Err(HostError::unsupported());
        }
        // A place of another host, or one already closed, is gone: that
        // counts as closed.
        if place.host == self.id {
            let mut st = lock(&self.state);
            st.places.remove(&place.id);
            st.order.retain(|id| id != &place.id);
        }
        Self::finish((), late_error)
    }

    /// The request `split_run` got for the place; `None` for a place that
    /// `split_run` did not make.
    pub fn split_request_of(&self, place: &Place) -> Option<SplitRequest> {
        self.read_rec(place, |r| r.split.clone()).flatten()
    }
}
