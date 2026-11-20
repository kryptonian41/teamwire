//! Member rows: insert, launch state, place, session, token hash,
//! generations, the report gate and removal.

use std::path::PathBuf;

use rusqlite::Row;
use serde_json::{Value, json};
use teamwire_core::{
    EventType, HarnessId, HostId, Member, MemberKind, MemberState, Mode, Place, Role, SessionId,
    Timestamp,
};

use crate::error::StoreError;
use crate::tx::{
    Listing, ReadTx, Tx, bad, col, exactly_one, hash32, listing, one, parsed, parsed_opt,
    same_hash, token_hash, ts, ts_opt,
};

/// What launch knows about a member before it starts anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMember {
    pub role: Role,
    pub title: String,
    pub kind: MemberKind,
    pub mode: Mode,
    pub harness: HarnessId,
    pub host: HostId,
    pub cwd: PathBuf,
    pub repo: Option<PathBuf>,
    pub branch: Option<String>,
    pub base: Option<String>,
    pub base_sha: Option<String>,
    pub check: Option<String>,
    pub owns: Vec<String>,
    pub deadline_s: u64,
}

const MEMBER_COLS: &str = "role, title, kind, mode, harness, host, place, session_id, cwd, repo, \
     branch, base, base_sha, check_cmd, owns, deadline_s, state, generation, gen_started_at, \
     gate_told_generation, removed_at, removed_note";

fn member_from_row(row: &Row<'_>) -> Result<Member, String> {
    let host: HostId = parsed(row, 5, "host", HostId::parse)?;
    let place: Option<String> = col(row, 6)?;
    let path_opt = |i| -> Result<Option<PathBuf>, String> {
        let text: Option<String> = col(row, i)?;
        Ok(text.map(PathBuf::from))
    };
    let owns: String = col(row, 14)?;
    let owns: Vec<String> = serde_json::from_str(&owns).map_err(|e| bad("owns", e))?;
    let deadline: i64 = col(row, 15)?;
    let generation: i64 = col(row, 17)?;
    let told: Option<i64> = col(row, 19)?;
    Ok(Member {
        role: parsed(row, 0, "role", Role::parse)?,
        title: col(row, 1)?,
        kind: parsed(row, 2, "kind", MemberKind::parse)?,
        mode: parsed(row, 3, "mode", Mode::parse)?,
        harness: parsed(row, 4, "harness", HarnessId::parse)?,
        place: place.map(|id| Place {
            host: host.clone(),
            id,
        }),
        host,
        session_id: parsed_opt(row, 7, "session", SessionId::parse)?,
        cwd: PathBuf::from(col::<String>(row, 8)?),
        repo: path_opt(9)?,
        branch: col(row, 10)?,
        base: col(row, 11)?,
        base_sha: col(row, 12)?,
        check: col(row, 13)?,
        owns,
        deadline_s: u64::try_from(deadline).map_err(|e| bad("deadline", e))?,
        state: parsed(row, 16, "state", MemberState::parse)?,
        generation: u32::try_from(generation).map_err(|e| bad("generation", e))?,
        gen_started_at: ts(row, 18)?,
        gate_told_generation: told
            .map(|g| u32::try_from(g).map_err(|e| bad("gate_told_generation", e)))
            .transpose()?,
        removed_at: ts_opt(row, 20)?,
        removed_note: col(row, 21)?,
    })
}

fn state_event(state: MemberState) -> EventType {
    match state {
        MemberState::Creating => EventType::MemberCreating,
        MemberState::Started => EventType::MemberStarted,
        MemberState::Failed => EventType::MemberFailed,
        MemberState::Removed => EventType::MemberRemoved,
    }
}

/// The launch and removal moves a member's state may make. `failed` goes
/// back to `creating` only through `relaunch_failed_member`; `removed` is
/// reached only through `remove_member` and never left.
fn may_move(from: MemberState, to: MemberState) -> bool {
    matches!(
        (from, to),
        (
            MemberState::Creating,
            MemberState::Started | MemberState::Failed
        ) | (MemberState::Started, MemberState::Failed)
    )
}

impl Tx<'_> {
    /// Inserts a member as `creating`, generation 1, and writes
    /// `member.creating`. A second insert of the same role is a constraint
    /// error; a `failed` role is relaunched with `relaunch_failed_member`.
    pub fn insert_member(&self, at: Timestamp, m: &NewMember) -> Result<(), StoreError> {
        self.insert_member_with(at, m, &Value::Null)
    }

    /// `insert_member`, with `extra` (a JSON object) merged into the
    /// `member.creating` event's data, such as the launch id and requested
    /// count of `docs/metrics.md` section 5. `kind` and `mode` always win.
    pub fn insert_member_with(
        &self,
        at: Timestamp,
        m: &NewMember,
        extra: &Value,
    ) -> Result<(), StoreError> {
        self.connection().execute(
            "INSERT INTO members (role, title, kind, mode, harness, host, cwd, repo, branch, base,
                                  base_sha, check_cmd, owns, deadline_s, state, generation,
                                  gen_started_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 'creating', 1, ?15)",
            rusqlite::params![
                m.role.as_str(),
                m.title,
                m.kind.as_str(),
                m.mode.as_str(),
                m.harness.as_str(),
                m.host.as_str(),
                m.cwd.to_string_lossy(),
                m.repo.as_ref().map(|p| p.to_string_lossy().into_owned()),
                m.branch,
                m.base,
                m.base_sha,
                m.check,
                json!(m.owns).to_string(),
                i64::try_from(m.deadline_s).unwrap_or(i64::MAX),
                at.to_string(),
            ],
        )?;
        self.creating_event(at, m, extra)
    }

    fn creating_event(
        &self,
        at: Timestamp,
        m: &NewMember,
        extra: &Value,
    ) -> Result<(), StoreError> {
        let mut data = extra.as_object().cloned().unwrap_or_default();
        data.insert("kind".to_owned(), json!(m.kind.as_str()));
        data.insert("mode".to_owned(), json!(m.mode.as_str()));
        self.record_event(
            at,
            EventType::MemberCreating,
            Some(&m.role),
            None,
            &Value::Object(data),
        )?;
        Ok(())
    }

    /// Launches a role again whose row is `failed` and never reached
    /// `started` (LCH-5; a role that started is refused with a constraint error): the row takes the
    /// new fields, loses its place, session and token, and is `creating`
    /// again. Writes `member.replaced` (with the failed attempt's times)
    /// and `member.creating`. False, and nothing changes, when the role's row
    /// is not `failed`.
    pub fn relaunch_failed_member(&self, at: Timestamp, m: &NewMember) -> Result<bool, StoreError> {
        self.relaunch_failed_member_with(at, m, &Value::Null)
    }

    /// `relaunch_failed_member`, with `extra` merged into the new
    /// `member.creating` event's data, as `insert_member_with` does.
    pub fn relaunch_failed_member_with(
        &self,
        at: Timestamp,
        m: &NewMember,
        extra: &Value,
    ) -> Result<bool, StoreError> {
        let Some(old) = self.reader().member(&m.role)? else {
            return Ok(false);
        };
        if old.state != MemberState::Failed {
            return Ok(false);
        }
        // LCH-5: only a role that failed before it started may be launched again.
        let started: i64 = self.connection().query_row(
            "SELECT count(*) FROM events WHERE type = 'member.started' AND role = ?1",
            [m.role.as_str()],
            |r| r.get(0),
        )?;
        if started > 0 {
            return Err(StoreError::Constraint(format!(
                "member {} reached started and cannot be launched again",
                m.role
            )));
        }
        let rows = self.connection().execute(
            "UPDATE members SET title = ?2, kind = ?3, mode = ?4, harness = ?5, host = ?6,
                    place = NULL, session_id = NULL, token_sha256 = NULL, cwd = ?7, repo = ?8,
                    branch = ?9, base = ?10, base_sha = ?11, check_cmd = ?12, owns = ?13,
                    deadline_s = ?14, state = 'creating', gen_started_at = ?15
             WHERE role = ?1 AND state = 'failed'",
            rusqlite::params![
                m.role.as_str(),
                m.title,
                m.kind.as_str(),
                m.mode.as_str(),
                m.harness.as_str(),
                m.host.as_str(),
                m.cwd.to_string_lossy(),
                m.repo.as_ref().map(|p| p.to_string_lossy().into_owned()),
                m.branch,
                m.base,
                m.base_sha,
                m.check,
                json!(m.owns).to_string(),
                i64::try_from(m.deadline_s).unwrap_or(i64::MAX),
                at.to_string(),
            ],
        )?;
        exactly_one(rows, format!("failed member {}", m.role))?;
        self.record_event(
            at,
            EventType::MemberReplaced,
            Some(&m.role),
            None,
            &json!({
                "previous_state": "failed",
                "previous_kind": old.kind.as_str(),
                "previous_started_at": old.gen_started_at.to_string(),
            }),
        )?;
        self.creating_event(at, m, extra)?;
        Ok(true)
    }

    /// Records where the member's process runs. No event: the launch step
    /// that follows (`member.started`) is the one metrics count.
    pub fn set_member_place(&self, role: &Role, place: &Place) -> Result<(), StoreError> {
        let rows = self.connection().execute(
            "UPDATE members SET place = ?2 WHERE role = ?1",
            [role.as_str(), place.id.as_str()],
        )?;
        exactly_one(rows, format!("member {role}"))
    }

    /// Replaces the member's check and its whole owns list, for
    /// `teamwire amend` (ADR 0046). No event: the amend's `CHECK AMENDED:`
    /// note, stored in the same transaction, is the record of the change,
    /// and the schema stays as it is.
    pub fn set_member_scope(
        &self,
        role: &Role,
        check: Option<&str>,
        owns: &[String],
    ) -> Result<(), StoreError> {
        let rows = self.connection().execute(
            "UPDATE members SET check_cmd = ?2, owns = ?3 WHERE role = ?1",
            rusqlite::params![role.as_str(), check, json!(owns).to_string()],
        )?;
        exactly_one(rows, format!("member {role}"))
    }

    /// Clears the stored branch after launch deleted it, so `status` does not
    /// show it and `close` does not list it as kept. No event (see
    /// `set_member_place`).
    pub fn clear_member_branch(&self, role: &Role) -> Result<(), StoreError> {
        let rows = self.connection().execute(
            "UPDATE members SET branch = NULL WHERE role = ?1",
            [role.as_str()],
        )?;
        exactly_one(rows, format!("member {role}"))
    }

    /// Records the member's harness session id. No event (see `set_member_place`).
    pub fn set_member_session(&self, role: &Role, session: &SessionId) -> Result<(), StoreError> {
        let rows = self.connection().execute(
            "UPDATE members SET session_id = ?2 WHERE role = ?1",
            [role.as_str(), session.as_str()],
        )?;
        exactly_one(rows, format!("member {role}"))
    }

    /// Records the SHA-256 of the member's token. The token itself is never
    /// stored. No event (see `set_member_place`).
    pub fn set_member_token_hash(&self, role: &Role, hash: &[u8; 32]) -> Result<(), StoreError> {
        let rows = self.connection().execute(
            "UPDATE members SET token_sha256 = ?2 WHERE role = ?1",
            rusqlite::params![role.as_str(), hash.as_slice()],
        )?;
        exactly_one(rows, format!("member {role}"))
    }

    /// Moves a member to `started` or `failed` and writes `member.started`
    /// or `member.failed`, with `data` (for example `reason`, `seconds`) as the
    /// event's data. Any other move is refused: `removed` has its own method,
    /// and nothing goes back.
    pub fn set_member_state(
        &self,
        at: Timestamp,
        role: &Role,
        to: MemberState,
        data: &Value,
    ) -> Result<(), StoreError> {
        let Some(current) = self.reader().member(role)? else {
            return Err(StoreError::Constraint(format!("no such member {role}")));
        };
        if !may_move(current.state, to) {
            return Err(StoreError::Constraint(format!(
                "member {role} cannot move from {} to {to}",
                current.state
            )));
        }
        let rows = self.connection().execute(
            "UPDATE members SET state = ?2 WHERE role = ?1 AND state = ?3",
            [role.as_str(), to.as_str(), current.state.as_str()],
        )?;
        exactly_one(rows, format!("member {role}"))?;
        self.record_event(at, state_event(to), Some(role), None, data)?;
        Ok(())
    }

    /// Moves a member from `creating` to `failed` because launch refused it
    /// after recording it, and writes `member.refused` with `data`. No
    /// `member.failed` is written: the member counts once, as refused. The row
    /// stays, so close still finds what the undo left and `--add` can start it
    /// again. A member that is missing or not `creating` is a constraint error.
    pub fn refuse_member(
        &self,
        at: Timestamp,
        role: &Role,
        data: &Value,
    ) -> Result<(), StoreError> {
        let rows = self.connection().execute(
            "UPDATE members SET state = 'failed' WHERE role = ?1 AND state = 'creating'",
            [role.as_str()],
        )?;
        exactly_one(rows, format!("creating member {role}"))?;
        self.record_event(at, EventType::MemberRefused, Some(role), None, data)?;
        Ok(())
    }

    /// Marks a member `removed`, keeping its row, and writes `member.removed`
    /// with `data` (`unverified`, `stopped`). False when it was already removed.
    pub fn remove_member(
        &self,
        at: Timestamp,
        role: &Role,
        note: Option<&str>,
        data: &Value,
    ) -> Result<bool, StoreError> {
        if self.reader().member(role)?.is_none() {
            return Err(StoreError::Constraint(format!("no such member {role}")));
        }
        let rows = self.connection().execute(
            "UPDATE members SET state = 'removed', removed_at = ?2, removed_note = ?3
             WHERE role = ?1 AND state != 'removed'",
            rusqlite::params![role.as_str(), at.to_string(), note],
        )?;
        if rows == 0 {
            return Ok(false);
        }
        self.record_event(at, EventType::MemberRemoved, Some(role), None, data)?;
        Ok(true)
    }

    /// Starts the next generation of a member (a reprompt): the generation
    /// goes up by one, `gen_started_at` becomes `at`, and `member.reprompted`
    /// is written. Returns the new generation. A removed or failed member
    /// cannot be reprompted.
    pub fn bump_generation(
        &self,
        at: Timestamp,
        role: &Role,
        message: &str,
    ) -> Result<u32, StoreError> {
        self.next_generation(at, role, message, EventType::MemberReprompted)
    }

    /// Starts the next generation of a finished member that a message other
    /// than a reprompt woke (ADR 0043): as `bump_generation`, but the event
    /// is `member.woken`, which no reprompt count includes.
    pub fn wake_generation(
        &self,
        at: Timestamp,
        role: &Role,
        message: &str,
    ) -> Result<u32, StoreError> {
        self.next_generation(at, role, message, EventType::MemberWoken)
    }

    fn next_generation(
        &self,
        at: Timestamp,
        role: &Role,
        message: &str,
        kind: EventType,
    ) -> Result<u32, StoreError> {
        let rows = self.connection().execute(
            "UPDATE members SET generation = generation + 1, gen_started_at = ?2
             WHERE role = ?1 AND state IN ('creating', 'started')",
            [role.as_str(), &at.to_string()],
        )?;
        exactly_one(rows, format!("live member {role}"))?;
        let generation = self
            .reader()
            .member(role)?
            .map(|m| m.generation)
            .ok_or_else(|| StoreError::Constraint(format!("no such member {role}")))?;
        self.record_event(
            at,
            kind,
            Some(role),
            None,
            &json!({ "generation": generation, "message": message }),
        )?;
        Ok(generation)
    }

    /// Moves a started member to a new session in a new place: the generation
    /// goes up by one, `gen_started_at` becomes `at`, and `member.handoff` is
    /// written. Deliveries the member has not read lose their `reminded_at`,
    /// so the new session is reminded of them and the Stop hook counts them
    /// again. Returns the new generation.
    /// Only a `started` member can hand off.
    pub fn record_handoff(
        &self,
        at: Timestamp,
        role: &Role,
        session: &SessionId,
        place: &Place,
    ) -> Result<u32, StoreError> {
        let old = self
            .reader()
            .member(role)?
            .ok_or_else(|| StoreError::Constraint(format!("no such member {role}")))?;
        let rows = self.connection().execute(
            "UPDATE members SET session_id = ?2, place = ?3,
                    generation = generation + 1, gen_started_at = ?4
             WHERE role = ?1 AND state = 'started'",
            rusqlite::params![role.as_str(), session.as_str(), place.id, at.to_string()],
        )?;
        exactly_one(rows, format!("started member {role}"))?;
        self.connection().execute(
            "UPDATE deliveries SET reminded_at = NULL
             WHERE recipient = ?1 AND state != 'acked'",
            [role.as_str()],
        )?;
        let generation = old.generation + 1;
        self.record_event(
            at,
            EventType::MemberHandoff,
            Some(role),
            None,
            &json!({
                "generation": generation,
                "old_session": old.session_id.as_ref().map(SessionId::as_str),
                "new_session": session.as_str(),
                "place": place.id,
            }),
        )?;
        Ok(generation)
    }

    /// Undoes a `bump_generation` or `wake_generation` after a known failure
    /// to deliver its message: puts back the earlier generation and its start
    /// time. The earlier event stays; a second one of the same `kind`
    /// (`member.reprompted` or `member.woken`) with `"undone": true` says the
    /// bump was taken back. Refused once the new generation has a
    /// completion, which the undo would hide (ADR 0007 point 10).
    pub fn restore_generation(
        &self,
        at: Timestamp,
        role: &Role,
        generation: u32,
        gen_started_at: Timestamp,
        kind: EventType,
    ) -> Result<(), StoreError> {
        let rows = self.connection().execute(
            "UPDATE members SET generation = ?2, gen_started_at = ?3
             WHERE role = ?1 AND generation = ?2 + 1
               AND NOT EXISTS (SELECT 1 FROM completions
                               WHERE role = ?1 AND generation = ?2 + 1)",
            rusqlite::params![role.as_str(), generation, gen_started_at.to_string()],
        )?;
        exactly_one(
            rows,
            format!("member {role} one generation ahead of {generation}, with no completion yet"),
        )?;
        self.record_event(
            at,
            kind,
            Some(role),
            None,
            &json!({ "generation": generation, "undone": true }),
        )?;
        Ok(())
    }

    /// Records the generation the report check last told the member about.
    pub fn set_gate_told_generation(&self, role: &Role, generation: u32) -> Result<(), StoreError> {
        let rows = self.connection().execute(
            "UPDATE members SET gate_told_generation = ?2 WHERE role = ?1",
            rusqlite::params![role.as_str(), generation],
        )?;
        exactly_one(rows, format!("member {role}"))
    }

    /// Unread messages held the member: writes `member.held` with `data`
    /// and leaves `gate_told_generation` alone, so the once-per-generation
    /// report-gap hold is not used up (ADR 0003 point 8).
    pub fn hold_member_unread(
        &self,
        at: Timestamp,
        role: &Role,
        data: &Value,
    ) -> Result<(), StoreError> {
        if self.reader().member(role)?.is_none() {
            return Err(StoreError::Constraint(format!("no such member {role}")));
        }
        self.record_event(at, EventType::MemberHeld, Some(role), None, data)?;
        Ok(())
    }

    /// The report check held the member: records `told_generation` and writes
    /// `member.held` with `data` (`gaps`, `unread`).
    pub fn hold_member(
        &self,
        at: Timestamp,
        role: &Role,
        told_generation: u32,
        data: &Value,
    ) -> Result<(), StoreError> {
        self.set_gate_told_generation(role, told_generation)?;
        self.record_event(at, EventType::MemberHeld, Some(role), None, data)?;
        Ok(())
    }
}

impl ReadTx<'_> {
    pub fn member(&self, role: &Role) -> Result<Option<Member>, StoreError> {
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {MEMBER_COLS} FROM members WHERE role = ?1"
        ))?;
        one(listing(
            "members",
            &mut stmt,
            [role.as_str()],
            member_from_row,
        )?)
    }

    /// Every member, removed ones included, in the order they were inserted.
    /// A damaged row is reported in `bad` and left out.
    pub fn members(&self) -> Result<Listing<Member>, StoreError> {
        let mut stmt = self
            .connection()
            .prepare(&format!("SELECT {MEMBER_COLS} FROM members ORDER BY rowid"))?;
        listing("members", &mut stmt, [], member_from_row)
    }

    /// The stored SHA-256 of the member's token, if one was recorded.
    pub fn member_token_hash(&self, role: &Role) -> Result<Option<[u8; 32]>, StoreError> {
        let mut stmt = self
            .connection()
            .prepare("SELECT role, token_sha256 FROM members WHERE role = ?1")?;
        let found = listing("members", &mut stmt, [role.as_str()], |r| hash32(r, 1))?;
        Ok(one(found)?.flatten())
    }

    /// Whether `token` is the one launch gave `role` (ADR 0008 point 2).
    /// `None` when `role` has no member that is not removed, or no stored
    /// hash; otherwise whether the token's hash is the stored one, compared
    /// without stopping at the first difference.
    pub fn token_matches(&self, role: &Role, token: &str) -> Result<Option<bool>, StoreError> {
        let live = self
            .member(role)?
            .is_some_and(|m| m.state != MemberState::Removed);
        let stored = self.member_token_hash(role)?;
        Ok(stored
            .filter(|_| live)
            .map(|stored| same_hash(&stored, &token_hash(token))))
    }
}

#[cfg(test)]
mod tests;
