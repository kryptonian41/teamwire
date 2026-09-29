//! Members for the rig: only the tests that address a member include this.

use teamwire_core::{HarnessId, HostId, MemberKind, Mode, Role};
use teamwire_store::{NewMember, Party};

use crate::rig::{Rig, at, sid};

pub(crate) fn role(text: &str) -> Role {
    Role::parse(text).unwrap()
}

impl Rig {
    /// Adds a member on the given harness id, with session `session`.
    pub(crate) fn add_member(&mut self, name: &str, harness: &'static str, session: &str) -> Party {
        let r = role(name);
        let session = sid(session);
        self.store
            .write(|tx| {
                tx.insert_member(
                    at(1),
                    &NewMember {
                        role: r.clone(),
                        title: name.to_owned(),
                        kind: MemberKind::Implementer,
                        mode: Mode::Pane,
                        harness: HarnessId::from_static(harness),
                        host: HostId::from_static("fake"),
                        cwd: "/work".into(),
                        repo: None,
                        branch: None,
                        base: None,
                        base_sha: None,
                        check: None,
                        owns: vec![],
                        deadline_s: 1800,
                    },
                )?;
                tx.set_member_session(&r, &session)
            })
            .unwrap();
        Party::Member(r)
    }
}
