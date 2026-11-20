//! A member's two marks from its spec: `quiet` and the set label. They are
//! written by a call of their own, right after launch inserts or relaunches
//! the member in the same transaction, so `NewMember` keeps its fields.
//! `ReadTx::hears` reads the quiet mark back for every read that shows a
//! member a message.

use teamwire_core::{Actor, Recipient, Role};

use crate::error::StoreError;
use crate::tx::{ReadTx, Tx, exactly_one};

impl ReadTx<'_> {
    /// Whether `recipient` may be shown a message from `sender`, by the
    /// recipient's row as it stands now (`Member::hears`, MSG-3). A member
    /// can become quiet after a message to it was stored, through a relaunch
    /// after a failed start, so a read that shows a member a message asks
    /// this whatever the time the message was stored. The lead and another
    /// team's agents have no row here, and a role with no row has no rule to
    /// apply, so only a member that is quiet in effect gets false.
    pub fn hears(&self, recipient: &Recipient, sender: &Actor) -> Result<bool, StoreError> {
        let Recipient::Member(role) = recipient else {
            return Ok(true);
        };
        Ok(self.member(role)?.is_none_or(|m| m.hears(sender)))
    }
}

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

    /// The quiet mark decides at read time: the same stored message from
    /// a peer is heard before the mark is set and not after it.
    #[test]
    fn a_member_hears_a_peer_until_its_quiet_mark_is_set() {
        use teamwire_core::{Actor, Recipient};
        let mut f = with_members(&["api", "web"]);
        let to_api = Recipient::Member(role("api"));
        let hears = |f: &crate::testkit::Fixture, from: &Actor| {
            f.store.read(|tx| tx.hears(&to_api, from)).unwrap()
        };
        let peer = Actor::Member(role("web"));
        assert!(hears(&f, &peer));
        f.store
            .write(|tx| tx.set_member_marks(&role("api"), true, None))
            .unwrap();
        assert!(!hears(&f, &peer));
        assert!(!hears(&f, &Actor::User));
        assert!(hears(&f, &Actor::Lead));
        let lead = f
            .store
            .read(|tx| tx.hears(&Recipient::Lead, &peer))
            .unwrap();
        assert!(lead, "the lead has no quiet mark");
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
