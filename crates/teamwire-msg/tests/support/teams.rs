//! A second team beside the rig's own, in the same home and on the same fake
//! harness and host: the cross-team message tests.
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

use serde_json::json;
use teamwire_core::{
    Clock as _, HarnessId, HostId, LeadRoute, MemberKind, MemberState, Mode, Place, SessionKind,
    TeamName,
};
use teamwire_store::{Ctx, NewMember, NewTeam, Store};

use crate::rig::{Rig, role, sid};

impl Rig {
    /// Another team `name` in the same home, led from session `lead_session`
    /// on the same fake harness and host, with the named members started.
    /// Returns each party's place, the lead first.
    pub(crate) fn add_team(
        &mut self,
        name: &str,
        lead_session: &str,
        members: &[(&str, MemberKind)],
    ) -> Vec<(String, Place)> {
        let team = TeamName::parse(name).unwrap();
        let at = self.clock.now();
        let mut store = Store::open(self.home.path(), &team).unwrap();
        store
            .write(|tx| {
                tx.create_team(
                    at,
                    &NewTeam {
                        name: team.clone(),
                        title: name.to_owned(),
                        lead_harness: HarnessId::from_static("fake"),
                        lead_host: HostId::from_static("fake"),
                        lead_route: LeadRoute::Sink,
                        lead_session: Some((sid(lead_session), SessionKind::Interactive)),
                    },
                )?;
                Ok(())
            })
            .unwrap();
        let mut places = vec![("lead".to_owned(), self.healthy(lead_session))];
        for (role_name, kind) in members {
            let r = role(role_name);
            let session = format!("session-{name}-{role_name}");
            let cwd = self.home.path().join("work").join(name).join(role_name);
            store
                .write(|tx| {
                    tx.insert_member(
                        at,
                        &NewMember {
                            role: r.clone(),
                            title: (*role_name).to_owned(),
                            kind: *kind,
                            mode: Mode::Pane,
                            harness: HarnessId::from_static("fake"),
                            host: HostId::from_static("fake"),
                            cwd,
                            repo: None,
                            branch: None,
                            base: None,
                            base_sha: None,
                            check: None,
                            owns: vec![],
                            deadline_s: 1800,
                        },
                    )?;
                    tx.set_member_session(&r, &sid(&session))?;
                    tx.set_member_state(at, &r, MemberState::Started, &json!({}))
                })
                .unwrap();
            places.push(((*role_name).to_owned(), self.healthy(&session)));
        }
        places
    }

    /// A context on `store`, another team's, with this rig's adapters.
    pub(crate) fn ctx_on<'a>(&'a self, store: &'a mut Store) -> Ctx<'a> {
        Ctx {
            home: self.home.path(),
            store,
            registry: &self.registry,
            platform: &self.platform,
            clock: &*self.clock,
            config: &self.config,
        }
    }

    /// Opens team `name`'s store read-write.
    pub(crate) fn store_of(&self, name: &str) -> Store {
        Store::open_existing(self.home.path(), &TeamName::parse(name).unwrap()).unwrap()
    }
}
