//! The planning rules of LCH-18. They warn; they never refuse.

use globset::Glob;
use teamwire_core::{MemberKind, MemberState};

use crate::PlannedMember;
use crate::plan::Cx;

/// More open implementer branches than this makes integration hard.
const MAX_OPEN_IMPLEMENTERS: usize = 5;

/// True when two `owns` patterns can name the same path (LCH-18).
fn owns_overlap(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim_start_matches("./"), b.trim_start_matches("./"));
    if a == b {
        return true;
    }
    let under = |dir: &str, other: &str| dir.ends_with('/') && other.starts_with(dir);
    if under(a, b) || under(b, a) {
        return true;
    }
    let hit = |pattern: &str, text: &str| {
        Glob::new(pattern).is_ok_and(|g| g.compile_matcher().is_match(text))
    };
    hit(a, b) || hit(b, a)
}

pub(crate) fn planning_warnings(
    cx: &Cx<'_>,
    members: &[PlannedMember],
    warnings: &mut Vec<String>,
) {
    let implementers: Vec<&PlannedMember> = members
        .iter()
        .filter(|m| m.kind == MemberKind::Implementer)
        .collect();
    for m in &implementers {
        let missing = match (m.spec.check.is_none(), m.spec.owns.is_empty()) {
            (true, true) => "a check and an owns list",
            (true, false) => "a check",
            (false, true) => "an owns list",
            (false, false) => continue,
        };
        warnings.push(format!(
            "role {}: an implementer without {missing} cannot be verified",
            m.spec.role
        ));
    }
    for (i, a) in implementers.iter().enumerate() {
        for b in implementers.iter().skip(i + 1) {
            let clash = a.spec.owns.iter().find_map(|x| {
                b.spec
                    .owns
                    .iter()
                    .find(|y| owns_overlap(x, y))
                    .map(|y| (x, y))
            });
            if let Some((x, y)) = clash {
                warnings.push(format!(
                    "roles {} and {} both own overlapping paths ({x} and {y})",
                    a.spec.role, b.spec.role
                ));
            }
        }
    }
    let open = cx.existing.map_or(0, |e| {
        e.members
            .iter()
            .filter(|m| {
                m.kind == MemberKind::Implementer
                    && matches!(m.state, MemberState::Creating | MemberState::Started)
            })
            .count()
    });
    if open + implementers.len() > MAX_OPEN_IMPLEMENTERS {
        warnings.push(format!(
            "{} implementer branches would be open; more than {MAX_OPEN_IMPLEMENTERS} makes integration hard",
            open + implementers.len()
        ));
    }
}
