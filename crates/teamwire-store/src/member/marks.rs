//! A member's two marks from its spec: `quiet` and the set label. They are
//! written by a call of their own, right after launch inserts or relaunches
//! the member in the same transaction, so `NewMember` keeps its fields.

use teamwire_core::Role;

use crate::error::StoreError;
use crate::tx::{Tx, exactly_one};

impl Tx<'_> {
    /// Records what the member's spec said: `quiet` (true makes a member of
    /// any kind quiet) and its set label. Both are written every time, so a
    /// relaunched member never keeps the failed row's marks. No event: launch
    /// puts both in the data of the `member.creating` event it writes in the
    /// same transaction.
    pub fn set_member_marks(
        &self,
        role: &Role,
        quiet: bool,
        set: Option<&str>,
    ) -> Result<(), StoreError> {
        let rows = self.connection().execute(
            "UPDATE members SET quiet = ?2, set_label = ?3 WHERE role = ?1",
            rusqlite::params![role.as_str(), i64::from(quiet), set],
        )?;
        exactly_one(rows, format!("member {role}"))
    }
}

#[cfg(test)]
mod tests {
    use crate::StoreError;
    use crate::testkit::{event_types, role, with_members};

    fn marks(f: &crate::testkit::Fixture, name: &str) -> (bool, Option<String>) {
        let m = f.store.read(|tx| tx.member(&role(name))).unwrap().unwrap();
        (m.quiet, m.set)
    }

    #[test]
    fn a_member_has_no_marks_until_launch_writes_them() {
        let f = with_members(&["api"]);
        assert_eq!(marks(&f, "api"), (false, None));
    }

    #[test]
    fn the_marks_are_stored_read_back_and_write_no_event() {
        let mut f = with_members(&["api", "web"]);
        let before = event_types(&f.store);
        f.store
            .write(|tx| tx.set_member_marks(&role("api"), true, Some("rev1")))
            .unwrap();
        assert_eq!(marks(&f, "api"), (true, Some("rev1".to_owned())));
        assert_eq!(marks(&f, "web"), (false, None));
        assert_eq!(event_types(&f.store), before);
    }

    /// A relaunch writes the new spec's marks over the failed row's, so an
    /// unset mark must clear an old one.
    #[test]
    fn each_write_replaces_both_marks() {
        let mut f = with_members(&["api"]);
        f.store
            .write(|tx| tx.set_member_marks(&role("api"), true, Some("rev1")))
            .unwrap();
        f.store
            .write(|tx| tx.set_member_marks(&role("api"), false, None))
            .unwrap();
        assert_eq!(marks(&f, "api"), (false, None));
    }

    #[test]
    fn marks_for_a_role_with_no_member_are_refused() {
        let mut f = with_members(&["api"]);
        let refused = f
            .store
            .write(|tx| tx.set_member_marks(&role("ghost"), true, None));
        assert!(
            matches!(refused, Err(StoreError::Constraint(_))),
            "{refused:?}"
        );
    }
}
