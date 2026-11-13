//! Which raised flags the lead has waived for a member at HEAD (ADR 0045).
//!
//! A waiver recorded at HEAD counts, as ADR 0009 point 2 set. A waiver for a
//! flag that names one file also counts at a later HEAD while that file's
//! content is the content at the waived HEAD, compared by blob id. Any
//! change to the file, a merge or rebase included, ends it. Every other
//! waiver ends at the next commit.
//!
//! Never: count a waiver when git cannot answer for either HEAD; hold a
//! store transaction open across a git call.

use std::collections::{HashMap, HashSet};

use teamwire_core::Role;
use teamwire_store::{Ctx, Waiver};

use crate::VerifyError;
use crate::git::Git;
use crate::inspect::RawFlag;

/// The ids of `raised` that a waiver of `role` covers at `head`.
pub(crate) fn waived_ids(
    ctx: &Ctx<'_>,
    git: &Git<'_>,
    role: &Role,
    head: &str,
    raised: &[RawFlag],
) -> Result<HashSet<String>, VerifyError> {
    let rows: Vec<Waiver> = ctx
        .store
        .read(|tx| tx.all_waivers()?.all_or_corrupt())?
        .into_iter()
        .filter(|w| &w.role == role)
        .collect();
    let mut at_head: HashMap<String, Option<Option<String>>> = HashMap::new();
    let mut waived = HashSet::new();
    for flag in raised {
        let mut earlier = Vec::new();
        for row in rows.iter().filter(|w| w.flag_id == flag.id) {
            if row.head_sha == head {
                waived.insert(flag.id.clone());
            } else {
                earlier.push(row.head_sha.as_str());
            }
        }
        let Some(path) = flag.path.as_deref() else {
            continue;
        };
        if waived.contains(&flag.id) || earlier.is_empty() {
            continue;
        }
        let now = at_head
            .entry(path.to_owned())
            .or_insert_with(|| git.blob(head, path).ok())
            .clone();
        let Some(now) = now else {
            continue;
        };
        if earlier
            .iter()
            .any(|w| git.blob(w, path).is_ok_and(|then| then == now))
        {
            waived.insert(flag.id.clone());
        }
    }
    Ok(waived)
}
