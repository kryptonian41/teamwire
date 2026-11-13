//! The `scope-changed` flag. Verify reads the member's check and owns at its
//! start and stores its record after the check has run. `teamwire amend` can
//! change both in between (ADR 0046). A record stored after the amend note
//! counts for `close --role`, so without this flag a run that started with
//! the old check could vouch for the new one. The record step re-reads the
//! member in the transaction that stores the record and compares.

use teamwire_core::Member;

use crate::{Flag, FlagId};

/// The flag id. It is raised after the waiver step, so no waiver can name it.
pub(crate) const SCOPE_CHANGED: &str = "scope-changed";

/// `Some` flag when the check or owns verify ran with differ from the stored
/// ones.
pub(crate) fn changed(ran_with: &Member, stored: &Member) -> Option<Flag> {
    (ran_with.check != stored.check || ran_with.owns != stored.owns).then(|| Flag {
        id: FlagId::new(SCOPE_CHANGED),
        detail: "the lead amended this member's check or owns while verify ran; run verify again"
            .to_owned(),
        waived: false,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use teamwire_core::{HarnessId, HostId, MemberKind, MemberState, Mode, Role, Timestamp};

    use super::*;

    fn member() -> Member {
        Member {
            role: Role::parse("api").unwrap(),
            title: "the api".to_owned(),
            kind: MemberKind::Implementer,
            mode: Mode::Worktree,
            harness: HarnessId::from_static("claude-code"),
            host: HostId::from_static("herdr"),
            place: None,
            session_id: None,
            cwd: PathBuf::from("/work"),
            repo: None,
            branch: None,
            base: None,
            base_sha: None,
            check: Some("cargo test -p a".to_owned()),
            owns: vec!["a/".to_owned()],
            deadline_s: 1800,
            state: MemberState::Started,
            generation: 1,
            gen_started_at: Timestamp::from_unix_millis(0).unwrap(),
            gate_told_generation: None,
            removed_at: None,
            removed_note: None,
        }
    }

    #[test]
    fn scope_changed_flags_a_record_whose_member_was_amended_mid_run() {
        let ran_with = member();
        let want = Flag {
            id: FlagId::new(SCOPE_CHANGED),
            detail:
                "the lead amended this member's check or owns while verify ran; run verify again"
                    .to_owned(),
            waived: false,
        };

        let mut new_check = member();
        new_check.check = Some("cargo test -p b".to_owned());
        assert_eq!(changed(&ran_with, &new_check), Some(want.clone()));

        let mut new_owns = member();
        new_owns.owns = vec!["a/".to_owned(), "b/".to_owned()];
        assert_eq!(changed(&ran_with, &new_owns), Some(want));

        // Anything else may change while verify runs: only the scope counts.
        let mut other = member();
        other.generation = 2;
        assert_eq!(changed(&ran_with, &member()), None);
        assert_eq!(changed(&ran_with, &other), None);
    }
}
