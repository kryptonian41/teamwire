//! The warning a whole-team close gives while the team's code has had no
//! closer. Leads closed teams whose implementers' work no closer had
//! checked. The warning is advice: the close still goes on.

use teamwire_core::{Member, MemberKind, TeamName};
use teamwire_store::Ctx;

use crate::git;

/// The warning, or `None` once any closer reported (a removed one too) or
/// when no implementer branch has a commit of its own.
pub(crate) fn warning(ctx: &Ctx<'_>, team: &TeamName, members: &[Member]) -> Option<String> {
    let reported = members.iter().any(|m| {
        m.kind == MemberKind::Closer
            && ctx
                .store
                .read(|tx| tx.latest_completion(&m.role))
                .ok()
                .flatten()
                .is_some()
    });
    if reported {
        return None;
    }
    let n = members
        .iter()
        .filter(|m| m.kind == MemberKind::Implementer && has_commits(ctx, m))
        .count();
    (n > 0).then(|| {
        format!(
            "no closer reported on team {team}, and {n} implementer branch(es) have commits; \
             add one from teamwire preset closer, or close anyway if the user said so."
        )
    })
}

/// The member's branch has a commit its base does not. A branch git cannot
/// read counts as none, because the warning never stops a close.
fn has_commits(ctx: &Ctx<'_>, member: &Member) -> bool {
    let (Some(repo), Some(branch), Some(base)) = (
        &member.repo,
        &member.branch,
        member.base_sha.as_ref().or(member.base.as_ref()),
    ) else {
        return false;
    };
    let range = format!("{base}..{branch}");
    git::stdout_of(ctx.platform, repo, &["rev-list", "--count", &range, "--"])
        .ok()
        .and_then(|n| n.parse::<u64>().ok())
        .is_some_and(|n| n > 0)
}
