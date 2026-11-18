//! A member's permission dialogs and the lead's answers (ADR 0052), kept as
//! three event types and no table. The ULID of a `dialog.opened` row is the
//! dialog id. `dialog.answered` and `dialog.closed` name it as `subject`.
//!
//! The state of a dialog is read from its rows, never stored: a
//! `dialog.closed` row gives its outcome; otherwise a dialog past
//! `expires_at` is expired, so a hook that died never leaves it pending;
//! otherwise a `dialog.answered` row makes it answered; otherwise it is
//! pending. `answer_dialog` checks and writes under the one write lock
//! `Store::write` holds, so two answers can never both be recorded.

use serde_json::{Value, json};
use teamwire_core::{
    DialogState, DialogVerdict, Event, EventType, PermissionAnswer, PermissionRequest, Role,
    SessionId, Timestamp, Ulid,
};

use crate::error::StoreError;
use crate::tx::{ReadTx, Tx};

/// One dialog, read from its events at a given time.
#[derive(Debug, Clone, PartialEq)]
pub struct DialogRecord {
    /// The `dialog.opened` event's ULID.
    pub id: Ulid,
    pub role: Role,
    pub generation: u32,
    pub opened_at: Timestamp,
    pub expires_at: Timestamp,
    pub request: PermissionRequest,
    pub state: DialogState,
    pub answer: Option<PermissionAnswer>,
    pub answered_at: Option<Timestamp>,
    pub closed_at: Option<Timestamp>,
}

/// What `Tx::answer_dialog` did.
#[derive(Debug, Clone, PartialEq)]
pub enum AnswerOutcome {
    /// Boxed, because a record is far bigger than the other outcomes.
    Recorded(Box<DialogRecord>),
    /// No `dialog.opened` row with this id for this role.
    NoDialog,
    /// The dialog belongs to an earlier session or generation of the member.
    Changed,
    /// Already answered, taken, expired or unanswerable.
    NotPending(DialogState),
}

impl Tx<'_> {
    /// Writes `dialog.opened`. For a request that is not answerable it also
    /// writes `dialog.closed` with outcome `unanswerable`, in the same
    /// transaction, so no answer can ever be recorded for it.
    pub fn open_dialog(
        &self,
        at: Timestamp,
        role: &Role,
        generation: u32,
        request: &PermissionRequest,
        expires_at: Timestamp,
    ) -> Result<Ulid, StoreError> {
        let data = json!({
            "session": request.session.as_str(),
            "generation": generation,
            "subagent": request.subagent,
            "tool": request.tool,
            "summary": request.summary,
            "input": request.input,
            "why_not": request.why_not,
            "expires_at": expires_at.to_string(),
        });
        let id = self.record_event(at, EventType::DialogOpened, Some(role), None, &data)?;
        if !request.answerable() {
            let closed = json!({ "outcome": DialogState::Unanswerable.as_str(), "verdict": null });
            self.record_event(at, EventType::DialogClosed, Some(role), Some(&id), &closed)?;
        }
        Ok(id)
    }

    /// Records `answer` to the dialog `id` of `role`, only while the dialog
    /// is pending at `at`, of the member's current session and generation.
    /// Anything else writes nothing and says why.
    pub fn answer_dialog(
        &self,
        at: Timestamp,
        role: &Role,
        id: &Ulid,
        answer: &PermissionAnswer,
    ) -> Result<AnswerOutcome, StoreError> {
        let reader = self.reader();
        let Some(record) = reader.dialog(id, at)? else {
            return Ok(AnswerOutcome::NoDialog);
        };
        if &record.role != role {
            return Ok(AnswerOutcome::NoDialog);
        }
        let Some(member) = reader.member(role)? else {
            return Ok(AnswerOutcome::NoDialog);
        };
        if member.generation != record.generation
            || member.session_id.as_ref() != Some(&record.request.session)
        {
            return Ok(AnswerOutcome::Changed);
        }
        if record.state != DialogState::Pending {
            return Ok(AnswerOutcome::NotPending(record.state));
        }
        let data = json!({
            "verdict": answer.verdict.as_str(),
            "reason": answer.reason,
            "by": "lead",
            "tool": record.request.tool,
            "summary": record.request.summary,
        });
        self.record_event(at, EventType::DialogAnswered, Some(role), Some(id), &data)?;
        Ok(AnswerOutcome::Recorded(Box::new(DialogRecord {
            state: DialogState::Answered,
            answer: Some(answer.clone()),
            answered_at: Some(at),
            ..record
        })))
    }

    /// Writes `dialog.closed`: `taken`, with the answer, when one is
    /// recorded, even past `expires_at`; else `expired`. Returns the answer
    /// when taken. Returns `None`, and writes nothing, when the dialog is
    /// already closed or unknown.
    pub fn close_dialog(
        &self,
        at: Timestamp,
        id: &Ulid,
    ) -> Result<Option<PermissionAnswer>, StoreError> {
        let Some(record) = self.reader().dialog(id, at)? else {
            return Ok(None);
        };
        if record.closed_at.is_some() {
            return Ok(None);
        }
        let (outcome, verdict) = match &record.answer {
            Some(a) => (DialogState::Taken, Some(a.verdict.as_str())),
            None => (DialogState::Expired, None),
        };
        let data = json!({ "outcome": outcome.as_str(), "verdict": verdict });
        self.record_event(
            at,
            EventType::DialogClosed,
            Some(&record.role),
            Some(id),
            &data,
        )?;
        Ok(record.answer)
    }
}

impl ReadTx<'_> {
    /// The dialog `id` as it stands at `now`, or `None` when no
    /// `dialog.opened` row has that id. A damaged row is an error.
    pub fn dialog(&self, id: &Ulid, now: Timestamp) -> Result<Option<DialogRecord>, StoreError> {
        let Some(opened) = self.event(id)? else {
            return Ok(None);
        };
        if opened.kind != EventType::DialogOpened {
            return Ok(None);
        }
        let related = self.events_about(id)?.all_or_corrupt()?;
        record_of(&opened, &related, now)
            .map(Some)
            .ok_or_else(|| StoreError::bad_value("events", &id.to_string(), "a damaged dialog row"))
    }

    /// Every dialog of `role` as it stands at `now`, newest first.
    pub fn dialogs_of_role(
        &self,
        role: &Role,
        now: Timestamp,
    ) -> Result<Vec<DialogRecord>, StoreError> {
        let events = self.events_of_role(role)?.all_or_corrupt()?;
        Ok(dialogs_from(&events, now))
    }
}

/// The dialogs among `events`, as they stand at `now`, newest first. Pure,
/// so the watch reuses the rows it already read with `events_of_role`. A
/// damaged `dialog.opened` row is left out.
pub fn dialogs_from(events: &[Event], now: Timestamp) -> Vec<DialogRecord> {
    let mut opened: Vec<&Event> = events
        .iter()
        .filter(|e| e.kind == EventType::DialogOpened)
        .collect();
    // `seq` orders rows written in the same millisecond; a ULID would not.
    opened.sort_by_key(|e| std::cmp::Reverse(e.seq));
    opened
        .into_iter()
        .filter_map(|opened| {
            let related: Vec<Event> = events
                .iter()
                .filter(|e| e.subject.as_ref() == Some(&opened.ulid))
                .cloned()
                .collect();
            record_of(opened, &related, now)
        })
        .collect()
}

/// One dialog from its opened row and the rows that name it.
fn record_of(opened: &Event, related: &[Event], now: Timestamp) -> Option<DialogRecord> {
    let d = &opened.data;
    let text = |k: &str| d.get(k).and_then(Value::as_str);
    let request = PermissionRequest {
        session: SessionId::parse(text("session")?).ok()?,
        subagent: d.get("subagent").and_then(Value::as_bool).unwrap_or(false),
        tool: text("tool")?.to_owned(),
        summary: text("summary")?.to_owned(),
        input: d.get("input").filter(|v| !v.is_null()).cloned(),
        why_not: text("why_not").map(str::to_owned),
    };
    let generation = u32::try_from(d.get("generation").and_then(Value::as_u64)?).ok()?;
    let expires_at = Timestamp::parse(text("expires_at")?).ok()?;
    let answered = related.iter().find(|e| e.kind == EventType::DialogAnswered);
    let answer = answered.and_then(|e| {
        Some(PermissionAnswer {
            verdict: DialogVerdict::parse(e.data.get("verdict")?.as_str()?).ok()?,
            reason: e.data.get("reason")?.as_str()?.to_owned(),
        })
    });
    let closed = related.iter().find(|e| e.kind == EventType::DialogClosed);
    let closed_state = closed.map(|e| {
        e.data
            .get("outcome")
            .and_then(Value::as_str)
            .and_then(|o| DialogState::parse(o).ok())
            .unwrap_or(DialogState::Expired)
    });
    let state = match closed_state {
        Some(s) => s,
        None if now >= expires_at => DialogState::Expired,
        None if answer.is_some() => DialogState::Answered,
        None => DialogState::Pending,
    };
    Some(DialogRecord {
        id: opened.ulid,
        role: opened.role.clone()?,
        generation,
        opened_at: opened.time,
        expires_at,
        request,
        state,
        answer,
        answered_at: answered.map(|e| e.time),
        closed_at: closed.map(|e| e.time),
    })
}
