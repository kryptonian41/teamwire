//! What `reopen` reads back from event data: the lead's folder, and the
//! model and effort a member was launched with. Nothing here has its own
//! column; launch and `lead --adopt` put these values in event data.

use std::path::PathBuf;

use teamwire_core::{Event, EventType, Role};

use crate::error::StoreError;
use crate::tx::ReadTx;

/// The model and effort a member's newest `member.started` event recorded.
/// `None` means the member ran on the default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunOptions {
    pub model: Option<String>,
    pub effort: Option<String>,
    /// The event has the `model` or `effort` key, even as `null`. False for a
    /// member started by a build that did not record them: whether it ran on
    /// the default is then unknown.
    pub recorded: bool,
}

fn text(data: &serde_json::Value, key: &str) -> Option<String> {
    data.get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

impl ReadTx<'_> {
    /// The `seq` of every event of one of `kinds`, oldest first, read from
    /// the `type` and `role` columns only. A role of `None` matches any role.
    fn seqs_of(&self, role: Option<&Role>, kinds: &[EventType]) -> Result<Vec<u64>, StoreError> {
        let marks = vec!["?"; kinds.len()].join(", ");
        let mut stmt = self.connection().prepare(&format!(
            "SELECT seq FROM events WHERE (?1 IS NULL OR role = ?1) AND type IN ({marks}) ORDER BY seq"
        ))?;
        let params = std::iter::once(role.map(|r| r.as_str().to_owned()))
            .chain(kinds.iter().map(|k| Some(k.as_str().to_owned())));
        let seqs = stmt
            .query_map(rusqlite::params_from_iter(params), |row| {
                row.get::<_, i64>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(seqs
            .into_iter()
            .filter_map(|n| u64::try_from(n).ok())
            .collect())
    }

    /// The events at `seqs` (sorted), or the first of them that is damaged
    /// as an error. A damaged event between them that is none of them is
    /// not read at all.
    fn events_at(&self, seqs: &[u64]) -> Result<Vec<Event>, StoreError> {
        let (Some(first), Some(last)) = (seqs.first(), seqs.last()) else {
            return Ok(Vec::new());
        };
        let range = self.events_after(first.saturating_sub(1), Some(*last), None)?;
        if let Some(bad) = range
            .bad
            .iter()
            .find(|b| b.key.parse().is_ok_and(|k: u64| seqs.contains(&k)))
        {
            return Err(StoreError::bad_value(bad.table, &bad.key, &bad.reason));
        }
        Ok(range
            .items
            .into_iter()
            .filter(|e| seqs.contains(&e.seq))
            .collect())
    }

    /// The newest event of `kind` (of `role`, when given). Fails only when
    /// that newest event is damaged: an older or unrelated damaged event
    /// does not matter to a caller that reads only the newest.
    pub fn latest_event(
        &self,
        role: Option<&Role>,
        kind: EventType,
    ) -> Result<Option<Event>, StoreError> {
        self.latest_of(role, &[kind])
    }

    /// The newest event of any of `kinds` (of `role`, when given), such as
    /// the event that started a member's current generation. Fails only when
    /// that newest event is damaged.
    pub fn latest_of_kinds(
        &self,
        role: Option<&Role>,
        kinds: &[EventType],
    ) -> Result<Option<Event>, StoreError> {
        self.latest_of(role, kinds)
    }

    /// The newest event of any of `kinds`: only that row is read.
    fn latest_of(
        &self,
        role: Option<&Role>,
        kinds: &[EventType],
    ) -> Result<Option<Event>, StoreError> {
        let newest = self.seqs_of(role, kinds)?.pop();
        Ok(self.events_at(newest.as_slice())?.pop())
    }

    /// Every event of `role` whose type is one of `kinds`, oldest first.
    /// Fails only when one of those events is damaged; a damaged event of
    /// another type is not read.
    pub fn events_of_kinds(
        &self,
        role: &Role,
        kinds: &[EventType],
    ) -> Result<Vec<Event>, StoreError> {
        self.events_at(&self.seqs_of(Some(role), kinds)?)
    }

    /// The lead's folder as the newest `team.created` or `team.lead_adopted`
    /// event recorded it. The newest event decides even when it holds no
    /// folder, because an older folder belongs to an older lead session.
    pub fn lead_cwd(&self) -> Result<Option<PathBuf>, StoreError> {
        let newest = self.latest_of(None, &[EventType::TeamCreated, EventType::TeamLeadAdopted])?;
        Ok(newest
            .and_then(|e| text(&e.data, "lead_cwd"))
            .map(PathBuf::from))
    }

    /// The context size, in tokens, at which `role` hands off, as its newest
    /// `member.started` event recorded it. `None` when the key is null or
    /// missing, or the member has no such event.
    pub fn context_handoff_tokens(&self, role: &Role) -> Result<Option<u64>, StoreError> {
        let started = self.latest_event(Some(role), EventType::MemberStarted)?;
        Ok(started.and_then(|e| e.data.get("context_handoff_tokens")?.as_u64()))
    }

    /// The model and effort of `role`'s newest `member.started` event.
    pub fn run_options(&self, role: &Role) -> Result<RunOptions, StoreError> {
        let started = self.latest_event(Some(role), EventType::MemberStarted)?;
        Ok(started.map_or_else(RunOptions::default, |e| RunOptions {
            model: text(&e.data, "model"),
            effort: text(&e.data, "effort"),
            recorded: e.data.get("model").is_some() || e.data.get("effort").is_some(),
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
                effort: Some("high".to_owned()),
                recorded: true,
            }
        );
        assert_eq!(
            f.store.read(|tx| tx.run_options(&role("other"))).unwrap(),
            RunOptions::default()
        );
    }

    fn damage_events(f: &mut crate::testkit::Fixture, kind: EventType) {
        f.store
            .write(|tx| {
                tx.connection()
                    .execute(
                        "UPDATE events SET time = 'yesterday' WHERE type = ?1",
                        [kind.as_str()],
                    )
                    .map(|_| ())
                    .map_err(Into::into)
            })
            .unwrap();
    }

    fn started_member() -> crate::testkit::Fixture {
        let mut f = fixture();
        f.store
            .write(|tx| {
                tx.create_team_with(at(0), &new_team(), &json!({ "lead_cwd": "/lead" }))?;
                tx.insert_member_with(at(1), &new_member("api"), &json!({}))?;
                tx.set_member_state(
                    at(2),
                    &role("api"),
                    MemberState::Started,
                    &json!({ "model": "opus", "effort": "high" }),
                )
            })
            .unwrap();
        f
    }

    #[test]
    fn an_unrelated_damaged_event_blocks_neither_reader() {
        let mut f = started_member();
        damage_events(&mut f, EventType::MemberCreating);
        let got = f.store.read(|tx| tx.run_options(&role("api"))).unwrap();
        assert_eq!(got.model.as_deref(), Some("opus"));
        assert_eq!(
            f.store.read(|tx| tx.lead_cwd()).unwrap(),
            Some(PathBuf::from("/lead"))
        );
    }

    #[test]
    fn a_damaged_older_lead_event_of_the_other_type_does_not_block_lead_cwd() {
        let mut f = fixture();
        let session = SessionId::parse("s2").unwrap();
        f.store
            .write(|tx| {
                tx.create_team_with(at(0), &new_team(), &json!({ "lead_cwd": "/first" }))?;
                tx.adopt_lead_in(
                    at(1),
                    &session,
                    SessionKind::Interactive,
                    Some(std::path::Path::new("/second")),
                )
            })
            .unwrap();
        damage_events(&mut f, EventType::TeamCreated);
        assert_eq!(
            f.store.read(|tx| tx.lead_cwd()).unwrap(),
            Some(PathBuf::from("/second"))
        );
    }

    #[test]
    fn a_damaged_started_event_of_another_member_is_passed_over() {
        let mut f = fixture();
        f.store
            .write(|tx| {
                tx.create_team(at(0), &new_team())?;
                tx.insert_member(at(1), &new_member("api"))?;
                tx.set_member_state(
                    at(2),
                    &role("api"),
                    MemberState::Started,
                    &json!({ "model": "sonnet" }),
                )?;
                tx.insert_member(at(3), &new_member("web"))?;
                tx.set_member_state(at(4), &role("web"), MemberState::Started, &json!({}))
            })
            .unwrap();
        f.store
            .write(|tx| {
                tx.connection()
                    .execute(
                        "UPDATE events SET time = 'yesterday' WHERE type = 'member.started' AND role = 'web'",
                        [],
                    )
                    .map(|_| ())
                    .map_err(Into::into)
            })
            .unwrap();
        let got = f.store.read(|tx| tx.run_options(&role("api"))).unwrap();
        assert_eq!(got.model.as_deref(), Some("sonnet"));
    }

    #[test]
    fn a_damaged_newest_started_event_is_corrupt() {
        let mut f = started_member();
        damage_events(&mut f, EventType::MemberStarted);
        let err = f.store.read(|tx| tx.run_options(&role("api"))).unwrap_err();
        assert!(matches!(err, crate::StoreError::Corrupt(_)), "{err:?}");
    }

    #[test]
    fn context_handoff_tokens_is_the_newest_started_events_number() {
        let mut f = fixture();
        let r = role("api");
        let tokens = |f: &crate::testkit::Fixture| f.store.read(|tx| tx.context_handoff_tokens(&r));
        assert_eq!(tokens(&f).unwrap(), None, "no event");
        f.store
            .write(|tx| {
                tx.create_team(at(0), &new_team())?;
                tx.insert_member(at(1), &new_member("api"))
            })
            .unwrap();
        assert_eq!(tokens(&f).unwrap(), None, "no member.started yet");
        let start = |f: &mut crate::testkit::Fixture, data: serde_json::Value| {
            f.store
                .write(|tx| tx.set_member_state(at(2), &role("api"), MemberState::Started, &data))
                .unwrap();
        };
        start(&mut f, json!({ "context_handoff_tokens": 120_000 }));
        assert_eq!(tokens(&f).unwrap(), Some(120_000));
    }

    #[test]
    fn context_handoff_tokens_is_none_for_null_and_for_a_missing_key() {
        for data in [json!({ "context_handoff_tokens": null }), json!({})] {
            let mut f = fixture();
            f.store
                .write(|tx| {
                    tx.create_team(at(0), &new_team())?;
                    tx.insert_member(at(1), &new_member("api"))?;
                    tx.set_member_state(at(2), &role("api"), MemberState::Started, &data)
                })
                .unwrap();
            let got = f
                .store
                .read(|tx| tx.context_handoff_tokens(&role("api")))
                .unwrap();
            assert_eq!(got, None, "{data}");
        }
    }
}
