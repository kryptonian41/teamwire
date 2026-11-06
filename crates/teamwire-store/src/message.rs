//! Messages: one transaction stores the message, its display id, its
//! per-sender `seq`, its hash, one `pending` delivery per recipient and the
//! `message.sent` event.

use rusqlite::Row;
use serde_json::json;
use teamwire_core::{
    Actor, DisplayId, EventType, Message, MessageType, Recipient, Timestamp, Ulid,
};

use crate::delivery::{DELIVERY_COLS, DeliveryRecord, delivery_from_row_at};
use crate::error::StoreError;
use crate::tx::{
    Listing, ReadTx, Tx, bad, col, listing, one, parsed, parsed_opt, sha256, ts, ulid_col,
};

/// A message to store. The sender is the proven actor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMessage {
    pub sender: Actor,
    pub kind: MessageType,
    /// The message this answers, already looked up by the service.
    pub re: Option<Ulid>,
    pub commit: Option<String>,
    pub body: String,
    /// Unique per sender. `None` makes the store use the message's own ULID.
    pub client_key: Option<String>,
    pub recipients: Vec<Recipient>,
}

/// The result of `insert_message`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMessage {
    pub message: Message,
    pub deliveries: Vec<DeliveryRecord>,
    /// True when `(sender, client_key)` already named a message: nothing was
    /// written, and `message` is the one stored earlier.
    pub duplicate: bool,
}

/// Which messages `ReadTx::messages` returns.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MessageFilter {
    pub sender: Option<Actor>,
    pub recipient: Option<Recipient>,
    pub kind: Option<MessageType>,
    /// Only messages created at or after this time.
    pub since: Option<Timestamp>,
    /// Messages sent by or delivered to this party (`log --role`). Combines
    /// with the other fields by AND.
    pub involving: Option<Actor>,
    pub limit: Option<u32>,
    /// With `limit`, keep the newest messages instead of the oldest. The
    /// result is still oldest first.
    pub newest: bool,
}

/// One message in a recipient's inbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxEntry {
    /// The message, body included even when it fails its hash.
    pub message: Message,
    /// False when the body no longer matches `message.body_sha256`. The
    /// service reports it and records a `store.error` event.
    pub hash_ok: bool,
    pub delivery: DeliveryRecord,
}

/// A message this sender sent to one recipient, and how that delivery stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentRecord {
    pub message: Message,
    pub state: teamwire_core::DeliveryState,
}

pub(crate) const MESSAGE_COLS: &str = "m.ulid, m.display_id, m.sender, m.seq, m.type, m.re, \
     m.commit_sha, m.body, m.body_sha256, m.created_at";

pub(crate) fn message_from_row(row: &Row<'_>) -> Result<Message, String> {
    let (message, hash_ok) = message_and_hash_ok(row)?;
    if hash_ok {
        Ok(message)
    } else {
        Err("the body does not match its stored hash".to_owned())
    }
}

/// Parses a message row without refusing a body that fails its hash. The
/// message carries the hash that was stored; the flag says whether the body
/// still matches it.
fn message_and_hash_ok(row: &Row<'_>) -> Result<(Message, bool), String> {
    let seq: i64 = col(row, 3)?;
    let body: String = col(row, 7)?;
    let stored: Vec<u8> = col(row, 8)?;
    let stored: [u8; 32] =
        <[u8; 32]>::try_from(stored).map_err(|v| format!("stored hash of {} bytes", v.len()))?;
    let hash_ok = stored == sha256(body.as_bytes());
    let message = Message {
        ulid: ulid_col(row, 0)?,
        display_id: parsed(row, 1, "display id", DisplayId::parse)?,
        sender: parsed(row, 2, "sender", Actor::parse)?,
        seq: u64::try_from(seq).map_err(|e| bad("seq", e))?,
        kind: parsed(row, 4, "type", MessageType::parse)?,
        re: parsed_opt(row, 5, "re", Ulid::parse)?,
        commit: col(row, 6)?,
        body,
        body_sha256: stored,
        created_at: ts(row, 9)?,
    };
    Ok((message, hash_ok))
}

impl Tx<'_> {
    /// Stores a message and its `pending` deliveries, or, when the sender's
    /// `client_key` names a message already stored, returns that one and
    /// changes nothing (no counter moves, no event). The display id and the
    /// sender's `seq` come from counters in this transaction. Writes
    /// `message.sent`.
    pub fn insert_message(
        &self,
        at: Timestamp,
        new: &NewMessage,
    ) -> Result<StoredMessage, StoreError> {
        let reader = self.reader();
        if let Some(key) = &new.client_key
            && let Some(message) = reader.message_by_client_key(&new.sender, key)?
        {
            let deliveries = reader.deliveries_of(&message.ulid)?.all_or_corrupt()?;
            return Ok(StoredMessage {
                message,
                deliveries,
                duplicate: true,
            });
        }
        let mut recipients: Vec<&Recipient> = Vec::new();
        for r in &new.recipients {
            if !recipients.contains(&r) {
                recipients.push(r);
            }
        }
        if recipients.is_empty() {
            return Err(StoreError::Constraint(
                "a message needs at least one recipient".to_owned(),
            ));
        }

        let n = self.next_counter("display_id")?;
        let seq = self.next_counter(&format!("seq:{}", new.sender))?;
        let display_id = DisplayId::message(n)
            .ok_or_else(|| StoreError::Constraint(format!("display id {n} is out of range")))?;
        let ulid = self.new_ulid(at);
        let hash = sha256(new.body.as_bytes());
        let key = new.client_key.clone().unwrap_or_else(|| ulid.to_string());
        self.connection().execute(
            "INSERT INTO messages (ulid, display_id, sender, client_key, seq, type, re, commit_sha,
                                   body, body_sha256, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                ulid.to_string(),
                display_id.as_str(),
                new.sender.as_str(),
                key,
                i64::try_from(seq).unwrap_or(i64::MAX),
                new.kind.as_str(),
                new.re.map(|u| u.to_string()),
                new.commit,
                new.body,
                hash.as_slice(),
                at.to_string(),
            ],
        )?;
        for r in &recipients {
            self.connection().execute(
                "INSERT INTO deliveries (message_ulid, recipient, state) VALUES (?1, ?2, 'pending')",
                [ulid.to_string(), r.as_str().to_owned()],
            )?;
        }
        let (sender_role, sender_kind) = match &new.sender {
            Actor::Lead => (None, "lead"),
            Actor::Member(role) => (Some(role), "member"),
            Actor::User => (None, "user"),
            Actor::Other(_) => (None, "other_team"),
        };
        self.record_event(
            at,
            EventType::MessageSent,
            sender_role,
            Some(&ulid),
            &json!({
                "type": new.kind.as_str(),
                "sender_kind": sender_kind,
                "recipients": recipients.iter().map(|r| r.as_str()).collect::<Vec<_>>(),
                "chars": new.body.chars().count(),
            }),
        )?;
        let message = reader
            .message(&ulid)?
            .ok_or_else(|| StoreError::Corrupt(format!("message {ulid} vanished")))?;
        let deliveries = reader.deliveries_of(&ulid)?.all_or_corrupt()?;
        Ok(StoredMessage {
            message,
            deliveries,
            duplicate: false,
        })
    }
}

impl ReadTx<'_> {
    pub fn message(&self, ulid: &Ulid) -> Result<Option<Message>, StoreError> {
        self.message_where("m.ulid = ?1", &ulid.to_string())
    }

    pub fn message_by_display_id(&self, id: &DisplayId) -> Result<Option<Message>, StoreError> {
        self.message_where("m.display_id = ?1", id.as_str())
    }

    /// The message this sender stored under `key`, if any.
    pub fn message_by_client_key(
        &self,
        sender: &Actor,
        key: &str,
    ) -> Result<Option<Message>, StoreError> {
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {MESSAGE_COLS} FROM messages m WHERE m.sender = ?1 AND m.client_key = ?2"
        ))?;
        one(listing(
            "messages",
            &mut stmt,
            [sender.as_str(), key],
            message_from_row,
        )?)
    }

    fn message_where(&self, condition: &str, arg: &str) -> Result<Option<Message>, StoreError> {
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {MESSAGE_COLS} FROM messages m WHERE {condition}"
        ))?;
        one(listing("messages", &mut stmt, [arg], message_from_row)?)
    }

    /// Messages in the order they were stored. A damaged row is reported in
    /// `bad` and left out.
    pub fn messages(&self, filter: &MessageFilter) -> Result<Listing<Message>, StoreError> {
        let mut conditions = vec!["1 = 1".to_owned()];
        let mut args: Vec<String> = Vec::new();
        let mut bind = |sql: &str, value: String| {
            args.push(value);
            conditions.push(sql.replace('?', &format!("?{}", args.len())));
        };
        if let Some(sender) = &filter.sender {
            bind("m.sender = ?", sender.as_str().to_owned());
        }
        if let Some(kind) = filter.kind {
            bind("m.type = ?", kind.as_str().to_owned());
        }
        if let Some(since) = filter.since {
            bind("m.created_at >= ?", since.to_string());
        }
        if let Some(recipient) = &filter.recipient {
            bind(
                "EXISTS (SELECT 1 FROM deliveries d WHERE d.message_ulid = m.ulid AND d.recipient = ?)",
                recipient.as_str().to_owned(),
            );
        }
        if let Some(who) = &filter.involving {
            bind(
                "(m.sender = ? OR EXISTS (SELECT 1 FROM deliveries d2
                   WHERE d2.message_ulid = m.ulid AND d2.recipient = ?))",
                who.as_str().to_owned(),
            );
        }
        let limit = filter
            .limit
            .map_or_else(String::new, |n| format!(" LIMIT {n}"));
        let order = if filter.newest && filter.limit.is_some() {
            "DESC"
        } else {
            "ASC"
        };
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {MESSAGE_COLS} FROM messages m WHERE {} ORDER BY m.rowid {order}{limit}",
            conditions.join(" AND ")
        ))?;
        let mut found = listing(
            "messages",
            &mut stmt,
            rusqlite::params_from_iter(args),
            message_from_row,
        )?;
        if order == "DESC" {
            found.items.reverse();
            found.bad.reverse();
        }
        Ok(found)
    }

    /// Every `question` that no `answer` points at through `re`, in the order
    /// they were stored. A question to a member and one to the lead count alike.
    pub fn unanswered_questions(&self) -> Result<Listing<Message>, StoreError> {
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {MESSAGE_COLS} FROM messages m
             WHERE m.type = 'question'
               AND NOT EXISTS (SELECT 1 FROM messages a WHERE a.type = 'answer' AND a.re = m.ulid)
             ORDER BY m.rowid"
        ))?;
        listing("messages", &mut stmt, [], message_from_row)
    }

    /// The deliveries to `recipient` in any of `states`, each with its message
    /// body and whether the body still matches its hash. A body that fails
    /// its hash is returned, not skipped, so the recipient can be told. Rows
    /// that do not parse otherwise are reported in `bad`.
    pub fn inbox(
        &self,
        recipient: &Recipient,
        states: &[teamwire_core::DeliveryState],
    ) -> Result<Listing<InboxEntry>, StoreError> {
        let list = states
            .iter()
            .map(|s| format!("'{}'", s.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {MESSAGE_COLS}, {DELIVERY_COLS} FROM messages m
             JOIN deliveries d ON d.message_ulid = m.ulid
             WHERE d.recipient = ?1 AND d.state IN ({list}) ORDER BY m.rowid"
        ))?;
        listing("messages", &mut stmt, [recipient.as_str()], |row| {
            let (message, hash_ok) = message_and_hash_ok(row)?;
            Ok(InboxEntry {
                message,
                hash_ok,
                delivery: delivery_from_row_at(row, 10)?,
            })
        })
    }

    /// What `sender` sent to `recipient` at or after `since`, with each
    /// delivery's state: the input of the burst and repeat checks.
    pub fn sent_to_since(
        &self,
        sender: &Actor,
        recipient: &Recipient,
        since: Timestamp,
    ) -> Result<Listing<SentRecord>, StoreError> {
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {MESSAGE_COLS}, d.state FROM messages m
             JOIN deliveries d ON d.message_ulid = m.ulid
             WHERE m.sender = ?1 AND d.recipient = ?2 AND m.created_at >= ?3
             ORDER BY m.rowid"
        ))?;
        listing(
            "messages",
            &mut stmt,
            [sender.as_str(), recipient.as_str(), &since.to_string()],
            |row| {
                Ok(SentRecord {
                    message: message_from_row(row)?,
                    state: parsed(row, 10, "state", teamwire_core::DeliveryState::parse)?,
                })
            },
        )
    }

    /// Every counter with its value, by name (`teamwire export`).
    pub fn counters(&self) -> Result<Listing<(String, u64)>, StoreError> {
        let mut stmt = self
            .connection()
            .prepare("SELECT name, value FROM counters ORDER BY name")?;
        listing("counters", &mut stmt, [], |row| {
            let name: String = col(row, 0)?;
            let v: i64 = col(row, 1)?;
            Ok((name, u64::try_from(v).map_err(|e| bad("counter", e))?))
        })
    }

    /// The largest value each message counter must at least hold, by counter
    /// name: `display_id` and one `seq:<sender>` per sender with a message.
    /// A counter below its entry makes every later send collide (`doctor`).
    pub fn counter_floors(&self) -> Result<Vec<(String, u64)>, StoreError> {
        let mut stmt = self.connection().prepare(
            "SELECT 'display_id', max(CAST(substr(display_id, 2) AS INTEGER)) FROM messages
             HAVING count(*) > 0
             UNION ALL
             SELECT 'seq:' || sender, max(seq) FROM messages GROUP BY sender
             ORDER BY 1",
        )?;
        listing("messages", &mut stmt, [], |row| {
            let name: String = col(row, 0)?;
            let v: i64 = col(row, 1)?;
            Ok((name, u64::try_from(v).map_err(|e| bad("counter", e))?))
        })?
        .all_or_corrupt()
    }

    /// The value of a named counter, or `None` if it was never used.
    pub fn counter(&self, name: &str) -> Result<Option<u64>, StoreError> {
        let mut stmt = self
            .connection()
            .prepare("SELECT name, value FROM counters WHERE name = ?1")?;
        let found = listing("counters", &mut stmt, [name], |row| {
            let v: i64 = col(row, 1)?;
            u64::try_from(v).map_err(|e| bad("counter", e))
        })?;
        one(found)
    }
}

#[cfg(test)]
mod tests;
