//! The single `team` row: creation, the close lock, lead adoption and the
//! sleep-hold record.

use std::path::Path;

use rusqlite::{OptionalExtension, Row};
use serde_json::{Value, json};
use teamwire_core::platform::ProcessRef;
use teamwire_core::{
    EventType, HarnessId, HoldInfo, HostId, LeadRoute, Pid, REFS_MAX, SessionId, SessionKind,
    TeamName, TeamState, TicketRef, Timestamp, Ulid,
};

use crate::error::StoreError;
use crate::tx::{ReadTx, Tx, bad, col, exactly_one, listing, one, parsed, parsed_opt, ts, ts_opt};

/// What launch records when it creates the team.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTeam {
    pub name: TeamName,
    pub title: String,
    pub lead_harness: HarnessId,
    pub lead_host: HostId,
    pub lead_route: LeadRoute,
    /// The lead's own session, when the lead has one to record.
    pub lead_session: Option<(SessionId, SessionKind)>,
}

/// The `team` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamRow {
    pub ulid: Ulid,
    pub name: TeamName,
    pub title: String,
    pub state: TeamState,
    /// The process that holds the close lock, while `state` is `Closing`.
    pub closing_pid: Option<Pid>,
    /// When `closing_pid` started, if the platform could tell. Together they
    /// name the owner: the pid alone may since belong to another process.
    pub closing_started: Option<Timestamp>,
    pub lead_session: Option<SessionId>,
    pub lead_kind: Option<SessionKind>,
    pub lead_harness: HarnessId,
    pub lead_host: HostId,
    pub lead_route: LeadRoute,
    pub sleep_hold: Option<HoldInfo>,
    pub created_at: Timestamp,
    /// The tickets the team works on, in the order they were recorded. Empty
    /// for a team launched without refs and for a database older than
    /// schema version 5.
    pub refs: Vec<TicketRef>,
}

/// How a close ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseEnd {
    /// The team is now `closed`.
    Completed,
    /// The close stopped at `step`; the lock is released and the team is open
    /// again so the close can run once more.
    Failed { step: String },
}

const TEAM_COLS: &str = "ulid, name, title, state, closing_pid, lead_session_id, lead_kind, \
     lead_harness, lead_host, lead_route, sleep_hold_pid, sleep_hold_owned, created_at, \
     closing_pid_started_at, refs";

/// A read-only view of a version-4 database shows `refs` as NULL.
fn refs_from_column(text: Option<String>) -> Result<Vec<TicketRef>, String> {
    text.map_or(Ok(Vec::new()), |t| {
        serde_json::from_str(&t).map_err(|e| bad("refs", e))
    })
}

fn team_from_row(row: &Row<'_>) -> Result<TeamRow, String> {
    let pid = |i| -> Result<Option<Pid>, String> {
        let raw: Option<i64> = col(row, i)?;
        raw.map(|p| u32::try_from(p).map(Pid::new).map_err(|e| bad("pid", e)))
            .transpose()
    };
    let hold_owned: Option<bool> = col(row, 11)?;
    let sleep_hold = match (pid(10)?, hold_owned) {
        (Some(pid), Some(owned)) => Some(HoldInfo { pid, owned }),
        (None, None) => None,
        _ => return Err("sleep hold pid and owned flag disagree".to_owned()),
    };
    Ok(TeamRow {
        ulid: parsed(row, 0, "ulid", Ulid::parse)?,
        name: parsed(row, 1, "name", TeamName::parse)?,
        title: col(row, 2)?,
        state: parsed(row, 3, "state", TeamState::parse)?,
        closing_pid: pid(4)?,
        closing_started: ts_opt(row, 13)?,
        lead_session: parsed_opt(row, 5, "lead session", SessionId::parse)?,
        lead_kind: parsed_opt(row, 6, "lead kind", SessionKind::parse)?,
        lead_harness: parsed(row, 7, "lead harness", HarnessId::parse)?,
        lead_host: parsed(row, 8, "lead host", HostId::parse)?,
        lead_route: parsed(row, 9, "lead route", LeadRoute::parse)?,
        sleep_hold,
        created_at: ts(row, 12)?,
        refs: refs_from_column(col(row, 14)?)?,
    })
}

impl Tx<'_> {
    /// Inserts the team row, `open`, and writes `team.created`.
    pub fn create_team(&self, at: Timestamp, team: &NewTeam) -> Result<Ulid, StoreError> {
        self.create_team_with(at, team, &Value::Null)
    }

    /// `create_team`, with `extra` merged into the `team.created` event's
    /// data (launch adds `lead_cwd`).
    pub fn create_team_with(
        &self,
        at: Timestamp,
        team: &NewTeam,
        extra: &Value,
    ) -> Result<Ulid, StoreError> {
        let mut data = extra.as_object().cloned().unwrap_or_default();
        data.extend([
            ("title".to_owned(), json!(team.title)),
            ("harness".to_owned(), json!(team.lead_harness.as_str())),
            ("host".to_owned(), json!(team.lead_host.as_str())),
            ("lead_route".to_owned(), json!(team.lead_route.as_str())),
        ]);
        let ulid = self.new_ulid(at);
        let (session, kind) = match &team.lead_session {
            Some((id, kind)) => (Some(id.as_str()), Some(kind.as_str())),
            None => (None, None),
        };
        self.connection().execute(
            "INSERT INTO team (id, ulid, name, title, state, lead_session_id, lead_kind,
                               lead_harness, lead_host, lead_route, created_at)
             VALUES (1, ?1, ?2, ?3, 'open', ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                ulid.to_string(),
                team.name.as_str(),
                team.title,
                session,
                kind,
                team.lead_harness.as_str(),
                team.lead_host.as_str(),
                team.lead_route.as_str(),
                at.to_string(),
            ],
        )?;
        self.record_event(
            at,
            EventType::TeamCreated,
            None,
            Some(&ulid),
            &Value::Object(data),
        )?;
        Ok(ulid)
    }

    /// Takes the close lock: `open` to `closing` with the owner's pid and
    /// start time, in one conditional update. A bare `Pid` records no start
    /// time. False when the team is not open, and then nothing changes and
    /// no event is written (CLS-3).
    pub fn begin_close(
        &self,
        at: Timestamp,
        owner: impl Into<ProcessRef>,
    ) -> Result<bool, StoreError> {
        let owner = owner.into();
        let rows = self.connection().execute(
            "UPDATE team SET state = 'closing', closing_pid = ?1, closing_pid_started_at = ?2
             WHERE id = 1 AND state = 'open'",
            rusqlite::params![
                i64::from(owner.pid.get()),
                owner.started.map(|t| t.to_string())
            ],
        )?;
        if rows == 0 {
            return Ok(false);
        }
        self.record_event(
            at,
            EventType::TeamClosing,
            None,
            None,
            &json!({ "pid": owner.pid.get() }),
        )?;
        Ok(true)
    }

    /// Ends a close that holds the lock. False when the team is not `closing`.
    pub fn finish_close(&self, at: Timestamp, end: &CloseEnd) -> Result<bool, StoreError> {
        let (state, data) = match end {
            CloseEnd::Completed => ("closed", json!({ "outcome": "completed" })),
            CloseEnd::Failed { step } => {
                ("open", json!({ "outcome": "failed", "failed_step": step }))
            }
        };
        let rows = self.connection().execute(
            "UPDATE team SET state = ?1, closing_pid = NULL, closing_pid_started_at = NULL
             WHERE id = 1 AND state = 'closing'",
            [state],
        )?;
        if rows == 0 {
            return Ok(false);
        }
        self.record_event(at, EventType::TeamClosed, None, None, &data)?;
        Ok(true)
    }

    /// Clears a close lock whose owner is dead (`close --clear-stale-lock`).
    /// False when no lock is held, or when it is now held by another owner
    /// than `checked` (pid and start time), the owner the caller saw and
    /// found dead: a close that took the lock since then keeps it.
    pub fn clear_stale_close_lock(
        &self,
        at: Timestamp,
        checked: Option<ProcessRef>,
    ) -> Result<bool, StoreError> {
        let rows = self.connection().execute(
            "UPDATE team SET state = 'open', closing_pid = NULL, closing_pid_started_at = NULL
             WHERE id = 1 AND state = 'closing'
               AND closing_pid IS ?1 AND closing_pid_started_at IS ?2",
            rusqlite::params![
                checked.map(|owner| i64::from(owner.pid.get())),
                checked
                    .and_then(|owner| owner.started)
                    .map(|t| t.to_string()),
            ],
        )?;
        if rows == 0 {
            return Ok(false);
        }
        self.record_event(
            at,
            EventType::CloseStep,
            None,
            None,
            &json!({ "step": "clear_stale_lock", "outcome": "cleared" }),
        )?;
        Ok(true)
    }

    /// Sets the refs column of the team row. The first launch calls it in the
    /// transaction that creates the row; the `team.created` event carries the
    /// same list, so no event is written here.
    pub fn set_refs(&self, refs: &[TicketRef]) -> Result<(), StoreError> {
        let rows = self.connection().execute(
            "UPDATE team SET refs = ?1 WHERE id = 1",
            [json!(refs).to_string()],
        )?;
        exactly_one(rows, "team")
    }

    /// Appends the refs the team does not hold yet, in the given order, and
    /// writes `team.refs_added` with the ones it appended. Returns them. A
    /// call that adds nothing writes no event. A ref is never removed.
    pub fn add_refs(
        &self,
        at: Timestamp,
        refs: &[TicketRef],
    ) -> Result<Vec<TicketRef>, StoreError> {
        let Some(team) = self.reader().team()? else {
            return Err(StoreError::Constraint("no such team".to_owned()));
        };
        let mut all = team.refs;
        let mut added: Vec<TicketRef> = Vec::new();
        for r in refs {
            if !all.contains(r) {
                all.push(r.clone());
                added.push(r.clone());
            }
        }
        if added.is_empty() {
            return Ok(added);
        }
        // `validate` read the team before this lock, so a second `--add` may
        // have used the room since.
        if all.len() > REFS_MAX {
            return Err(StoreError::Constraint(format!(
                "a team holds at most {REFS_MAX} refs, and this would make {}",
                all.len()
            )));
        }
        self.set_refs(&all)?;
        self.record_event(
            at,
            EventType::TeamRefsAdded,
            None,
            Some(&team.ulid),
            &json!({ "added": added }),
        )?;
        Ok(added)
    }

    /// Records a new lead session (`teamwire lead --adopt`) and returns the
    /// old one.
    pub fn adopt_lead(
        &self,
        at: Timestamp,
        session: &SessionId,
        kind: SessionKind,
    ) -> Result<Option<SessionId>, StoreError> {
        self.adopt_lead_in(at, session, kind, None)
    }

    /// `adopt_lead`, recording the new lead's folder in the event's data as
    /// `lead_cwd`, when it is known.
    pub fn adopt_lead_in(
        &self,
        at: Timestamp,
        session: &SessionId,
        kind: SessionKind,
        cwd: Option<&Path>,
    ) -> Result<Option<SessionId>, StoreError> {
        let old: Option<Option<String>> = self
            .connection()
            .query_row("SELECT lead_session_id FROM team WHERE id = 1", [], |r| {
                r.get(0)
            })
            .optional()?;
        let Some(old) = old else {
            return Err(StoreError::Constraint("no such team".to_owned()));
        };
        let rows = self.connection().execute(
            "UPDATE team SET lead_session_id = ?1, lead_kind = ?2 WHERE id = 1",
            [session.as_str(), kind.as_str()],
        )?;
        exactly_one(rows, "team")?;
        let mut data = json!({ "old_session": old, "new_session": session.as_str() });
        if let (Some(cwd), Some(map)) = (cwd, data.as_object_mut()) {
            map.insert("lead_cwd".to_owned(), json!(cwd.to_string_lossy()));
        }
        self.record_event(at, EventType::TeamLeadAdopted, None, None, &data)?;
        old.map(|s| SessionId::parse(&s).map_err(|e| StoreError::bad_value("team", "1", e)))
            .transpose()
    }

    /// Records the sleep hold the team owns or reuses, or clears the record.
    /// No event: the closed list of event types has none for it.
    pub fn set_sleep_hold(&self, hold: Option<&HoldInfo>) -> Result<(), StoreError> {
        let rows = self.connection().execute(
            "UPDATE team SET sleep_hold_pid = ?1, sleep_hold_owned = ?2 WHERE id = 1",
            rusqlite::params![hold.map(|h| i64::from(h.pid.get())), hold.map(|h| h.owned)],
        )?;
        exactly_one(rows, "team")
    }
}

impl ReadTx<'_> {
    /// The team row, if launch created one.
    pub fn team(&self) -> Result<Option<TeamRow>, StoreError> {
        let mut stmt = self
            .connection()
            .prepare(&format!("SELECT {TEAM_COLS} FROM team WHERE id = 1"))?;
        one(listing("team", &mut stmt, [], team_from_row)?)
    }
}

#[cfg(test)]
mod tests;
