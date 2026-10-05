//! What the per-member sequence carries between its steps: the team's
//! directories, what was made so far, the step timings, and the undo.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};
use teamwire_core::{HostErrorCode, Place, Role};
use teamwire_store::Ctx;

use crate::PlannedMember;
use crate::worktree::{self, Made};

const HOST_TIMEOUT: Duration = Duration::from_secs(30);

/// What has been made for a member so far, for the undo.
#[derive(Default)]
pub(crate) struct Progress {
    pub(crate) recorded: bool,
    pub(crate) place: Option<Place>,
    pub(crate) worktree: Option<Made>,
}

/// The paths of one team's directory.
pub(crate) struct Dirs {
    pub(crate) team: PathBuf,
}

impl Dirs {
    pub(crate) fn member(&self, role: &Role) -> PathBuf {
        self.team.join("members").join(role.as_str())
    }
    pub(crate) fn contract(&self, role: &Role) -> PathBuf {
        self.team
            .join("contract")
            .join(format!("{}.md", role.as_str()))
    }
    pub(crate) fn brief(&self, role: &Role) -> PathBuf {
        self.team
            .join("briefs")
            .join(format!("{}.md", role.as_str()))
    }
    pub(crate) fn report(&self, role: &Role) -> PathBuf {
        self.team
            .join("reports")
            .join(format!("{}.md", role.as_str()))
    }
    pub(crate) fn setup_log(&self, role: &Role) -> PathBuf {
        self.team
            .join("setup")
            .join(format!("{}.log", role.as_str()))
    }
    pub(crate) fn outbox(&self) -> PathBuf {
        self.team.join("outbox")
    }
}

/// Step timings, in milliseconds, for `data.steps`.
pub(crate) struct Steps {
    pub(crate) marks: Vec<(&'static str, u128)>,
    pub(crate) last: std::time::Instant,
}

impl Steps {
    pub(crate) fn mark(&mut self, ctx: &Ctx<'_>, name: &'static str) {
        let now = ctx.clock.monotonic();
        let ms = now.saturating_duration_since(self.last).as_millis();
        self.marks.push((name, ms));
        self.last = now;
    }

    pub(crate) fn json(&self) -> Value {
        Value::Object(
            self.marks
                .iter()
                .map(|(k, ms)| ((*k).to_owned(), json!(ms)))
                .collect(),
        )
    }
}

/// Takes back what was made: the place, then the worktree (no force).
/// The second value is true when the worktree could not be removed.
pub(crate) fn undo(ctx: &Ctx<'_>, m: &PlannedMember, made: &Progress) -> (String, bool) {
    let mut left = false;
    let mut notes = Vec::new();
    if let (Some(place), Ok(host)) = (&made.place, ctx.registry.host(&m.host)) {
        match host.close(place, HOST_TIMEOUT) {
            Ok(()) => notes.push(format!("place {} closed", place.id)),
            Err(e) if e.code == HostErrorCode::NotFound => {
                notes.push(format!("place {} was already gone", place.id));
            }
            Err(e) => notes.push(format!("could not close place {}: {e}", place.id)),
        }
    }
    if let Some(w) = &made.worktree {
        let (note, still_there) = worktree::remove(ctx.platform, w);
        left = still_there;
        notes.push(note);
    }
    (notes.join("; "), left)
}
