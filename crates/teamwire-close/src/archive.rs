//! The archive of a closed team: `team.db` from `SQLite`'s backup API, every
//! other file of the team directory, and `SHA256SUMS`. It is checked by
//! content before anything is deleted, and a path is never overwritten (CLS-2).

use std::path::{Path, PathBuf};

use teamwire_core::Timestamp;
use teamwire_store::Ctx;

use crate::CloseOpts;

/// `<parent>/<team>-<YYYY-MM-DD>`, then `-HHMMSS`, then `-HHMMSS-<n>` when the
/// earlier names are taken. The parent is `--archive-to`, else
/// `$TEAMWIRE_HOME/archive`. Nothing is created here.
pub(crate) fn destination(
    ctx: &Ctx<'_>,
    team: &teamwire_core::TeamName,
    opts: &CloseOpts,
) -> PathBuf {
    let parent = opts
        .archive_to
        .clone()
        .unwrap_or_else(|| ctx.home.join("archive"));
    first_free(&parent, team.as_str(), ctx.clock.now())
}

fn first_free(parent: &Path, team: &str, now: Timestamp) -> PathBuf {
    let at = now.as_datetime();
    let day = format!(
        "{team}-{:04}-{:02}-{:02}",
        at.year(),
        u8::from(at.month()),
        at.day()
    );
    let stamped = format!("{day}-{:02}{:02}{:02}", at.hour(), at.minute(), at.second());
    let mut candidates = vec![day, stamped.clone()];
    candidates.extend((2..1000).map(|n| format!("{stamped}-{n}")));
    let taken = |name: &String| parent.join(name).symlink_metadata().is_ok();
    let name = candidates
        .iter()
        .find(|n| !taken(n))
        .cloned()
        .unwrap_or(stamped);
    parent.join(name)
}
