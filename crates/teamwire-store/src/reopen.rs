//! What `reopen` reads back from event data: the lead's folder, and the
//! model and effort a member was launched with. Nothing here has its own
//! column; launch and `lead --adopt` put these values in event data.

use std::path::PathBuf;

use teamwire_core::{EventType, Role};

use crate::error::StoreError;
use crate::tx::ReadTx;

/// The model and effort a member's newest `member.started` event recorded.
/// `None` means the member was launched without one, or by a build that did
/// not record it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunOptions {
    pub model: Option<String>,
    pub effort: Option<String>,
}

fn text(data: &serde_json::Value, key: &str) -> Option<String> {
    data.get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

impl ReadTx<'_> {
    /// The lead's folder as the newest `team.created` or `team.lead_adopted`
    /// event recorded it. The newest event decides even when it holds no
    /// folder, because an older folder belongs to an older lead session.
    pub fn lead_cwd(&self) -> Result<Option<PathBuf>, StoreError> {
        let mut events = self
            .events_of_type(EventType::TeamCreated)?
            .all_or_corrupt()?;
        events.extend(
            self.events_of_type(EventType::TeamLeadAdopted)?
                .all_or_corrupt()?,
        );
        Ok(events
            .into_iter()
            .max_by_key(|e| e.seq)
            .and_then(|e| text(&e.data, "lead_cwd"))
            .map(PathBuf::from))
    }

    /// The model and effort of `role`'s newest `member.started` event.
    pub fn run_options(&self, role: &Role) -> Result<RunOptions, StoreError> {
        let started = self
            .events_of_role(role)?
            .all_or_corrupt()?
            .into_iter()
            .rev()
            .find(|e| e.kind == EventType::MemberStarted);
        Ok(started.map_or_else(RunOptions::default, |e| RunOptions {
            model: text(&e.data, "model"),
            effort: text(&e.data, "effort"),
        }))
    }
}

#[cfg(test)]
#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "a method path does not satisfy `Store::read`'s closure bound: it takes a reference to a type with its own lifetime"
)]
mod tests {
    use serde_json::json;
    use teamwire_core::{MemberState, SessionId, SessionKind};

    use crate::testkit::{at, fixture, new_member, new_team, role};

    use super::*;

    #[test]
    fn lead_cwd_is_the_newest_events_even_when_it_has_none() {
        let mut f = fixture();
        f.store
            .write(|tx| {
                tx.create_team_with(at(0), &new_team(), &json!({ "lead_cwd": "/first" }))
                    .map(|_| ())
            })
            .unwrap();
        assert_eq!(
            f.store.read(|tx| tx.lead_cwd()).unwrap(),
            Some(PathBuf::from("/first"))
        );
        let session = SessionId::parse("s2").unwrap();
        f.store
            .write(|tx| {
                tx.adopt_lead_in(
                    at(1),
                    &session,
                    SessionKind::Interactive,
                    Some(std::path::Path::new("/second")),
                )
            })
            .unwrap();
        assert_eq!(
            f.store.read(|tx| tx.lead_cwd()).unwrap(),
            Some(PathBuf::from("/second"))
        );
        f.store
            .write(|tx| tx.adopt_lead(at(2), &session, SessionKind::Interactive))
            .unwrap();
        assert_eq!(f.store.read(|tx| tx.lead_cwd()).unwrap(), None);
    }

    #[test]
    fn run_options_come_from_the_started_event() {
        let mut f = fixture();
        f.store
            .write(|tx| {
                tx.insert_member_with(at(0), &new_member("api"), &json!({}))?;
                tx.set_member_state(
                    at(1),
                    &role("api"),
                    MemberState::Started,
                    &json!({ "model": "opus", "effort": "high" }),
                )
            })
            .unwrap();
        let got = f.store.read(|tx| tx.run_options(&role("api"))).unwrap();
        assert_eq!(
            got,
            RunOptions {
                model: Some("opus".to_owned()),
                effort: Some("high".to_owned())
            }
        );
        assert_eq!(
            f.store.read(|tx| tx.run_options(&role("other"))).unwrap(),
            RunOptions::default()
        );
    }
}
