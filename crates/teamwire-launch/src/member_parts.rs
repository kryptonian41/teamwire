//! What the per-member sequence carries between its steps: the team's
//! directories, what was made so far, the step timings, and the undo.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};
use teamwire_core::{HostErrorCode, Place, Role};
use teamwire_store::Ctx;

use crate::PlannedMember;
use crate::worktree::{self, Excluded, Made};

const HOST_TIMEOUT: Duration = Duration::from_secs(30);

/// What has been made for a member so far, for the undo.
#[derive(Default)]
pub(crate) struct Progress {
    pub(crate) recorded: bool,
    pub(crate) place: Option<Place>,
    pub(crate) worktree: Option<Made>,
    pub(crate) excluded: Vec<Excluded>,
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

/// Takes back what was made: the place, then the worktree (no force), then
/// the exclude lines setup added.
/// The second value is true when the worktree could not be removed.
pub(crate) fn undo(ctx: &mut Ctx<'_>, m: &PlannedMember, made: &Progress) -> (String, bool) {
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
        let (note, still_there, branch_deleted) = worktree::remove(ctx.platform, w);
        left = still_there;
        notes.push(note);
        // A deleted branch must not stay in the member row: `status` would
        // show it and `close` would list it as kept.
        if branch_deleted && made.recorded {
            let cleared = ctx.store.write(|tx| {
                tx.connection()
                    .execute(
                        "UPDATE members SET branch = NULL WHERE role = ?1",
                        [m.spec.role.as_str()],
                    )
                    .map(|_| ())
                    .map_err(Into::into)
            });
            if let Err(e) = cleared {
                notes.push(format!("could not clear the stored branch: {e}"));
            }
        }
    }
    for e in &made.excluded {
        notes.push(worktree::unexclude(ctx.platform, e));
    }
    (notes.join("; "), left)
}
