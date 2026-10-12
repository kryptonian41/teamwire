//! A message that arrives from another team (ADR 0016). The sender's team
//! stores its own copy first and hands this team the same ULID, so the
//! copy here can be written again after a crash without making a second
//! message. The sender's rules ran in the sender's team; this store only
//! keeps its own counters and its own `pending` rows.

use serde_json::json;
use teamwire_core::{EventType, MessageType, Recipient, Remote, TeamState, Timestamp, Ulid};

use crate::error::StoreError;
use crate::message::StoredMessage;
use crate::tx::{Tx, sha256};

/// A message from `sender` to agents of this team.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivedMessage {
    /// The ULID the sender's team gave its own copy.
    pub ulid: Ulid,
    pub sender: Remote,
    pub kind: MessageType,
    /// The question this answers. A question that never reached this
    /// team, such as one sent before the team was made again under the same
    /// name, is dropped and the answer stands alone.
    pub re: Option<Ulid>,
    pub commit: Option<String>,
    pub body: String,
    /// This team's own parties: `lead` or a role.
    pub recipients: Vec<Recipient>,
}

impl Tx<'_> {
    /// Stores a message from another team with one `pending` delivery per
    /// recipient, under the sender's ULID, and writes `message.sent`.
    /// `None` when this team is not open. When the ULID is already here
    /// with the same sender and body, returns that copy and changes
    /// nothing; any other row under that ULID is a constraint error.
    pub fn insert_received(
        &self,
        at: Timestamp,
        new: &ReceivedMessage,
    ) -> Result<Option<StoredMessage>, StoreError> {
        let reader = self.reader();
        if reader.team()?.is_none_or(|t| t.state != TeamState::Open) {
            return Ok(None);
        }
        let hash = sha256(new.body.as_bytes());
        if let Some(message) = reader.message(&new.ulid)? {
            if message.sender.as_str() != new.sender.as_str() || message.body_sha256 != hash {
                return Err(StoreError::Constraint(format!(
                    "message {} is already stored with another sender or text",
                    new.ulid
                )));
            }
            let deliveries = reader.deliveries_of(&message.ulid)?.all_or_corrupt()?;
            return Ok(Some(StoredMessage {
                message,
                deliveries,
                duplicate: true,
            }));
        }
        let mut recipients: Vec<&Recipient> = Vec::new();
        for r in &new.recipients {
            if let Recipient::Other(_) = r {
                return Err(StoreError::Constraint(format!(
                    "{r} is not a party of this team"
                )));
            }
            if !recipients.contains(&r) {
                recipients.push(r);
            }
        }
        if recipients.is_empty() {
            return Err(StoreError::Constraint(
                "a message needs at least one recipient".to_owned(),
            ));
        }
        let re = match new.re {
            Some(q) if reader.message(&q)?.is_some() => Some(q),
            _ => None,
        };
        let n = self.next_counter("display_id")?;
        let seq = self.next_counter(&format!("seq:{}", new.sender.as_str()))?;
        let display_id = teamwire_core::DisplayId::message(n)
            .ok_or_else(|| StoreError::Constraint(format!("display id {n} is out of range")))?;
        let ulid = new.ulid.to_string();
        self.connection().execute(
            "INSERT INTO messages (ulid, display_id, sender, client_key, seq, type, re, commit_sha,
                                   body, body_sha256, created_at)
             VALUES (?1, ?2, ?3, ?1, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                ulid,
                display_id.as_str(),
                new.sender.as_str(),
                i64::try_from(seq).unwrap_or(i64::MAX),
                new.kind.as_str(),
                re.map(|u| u.to_string()),
                new.commit,
                new.body,
                hash.as_slice(),
                at.to_string(),
            ],
        )?;
        for r in &recipients {
            self.connection().execute(
                "INSERT INTO deliveries (message_ulid, recipient, state) VALUES (?1, ?2, 'pending')",
                [ulid.clone(), r.as_str().to_owned()],
            )?;
        }
        self.record_event(
            at,
            EventType::MessageSent,
            None,
            Some(&new.ulid),
            &json!({
                "type": new.kind.as_str(),
                "sender_kind": "other_team",
                "from": new.sender.as_str(),
                "recipients": recipients.iter().map(|r| r.as_str()).collect::<Vec<_>>(),
                "chars": new.body.chars().count(),
            }),
        )?;
        let message = reader
            .message(&new.ulid)?
            .ok_or_else(|| StoreError::Corrupt(format!("message {ulid} vanished")))?;
        let deliveries = reader.deliveries_of(&new.ulid)?.all_or_corrupt()?;
        Ok(Some(StoredMessage {
            message,
            deliveries,
            duplicate: false,
        }))
    }
}

#[cfg(test)]
mod tests;
