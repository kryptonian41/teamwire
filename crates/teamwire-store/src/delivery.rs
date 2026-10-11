//! Delivery rows: one per message and recipient. States only move forward.

use rusqlite::Row;
use serde_json::json;
use teamwire_core::{
    Delivery, DeliveryError, DeliveryErrorCode, DeliveryState, EventType, Recipient, Route,
    Timestamp, Ulid,
};

use crate::error::StoreError;
use crate::tx::{
    Listing, ReadTx, Tx, bad, col, listing, one, parsed, parsed_opt, ts_opt, ulid_col,
};

/// A delivery and the one column core's `Delivery` leaves out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryRecord {
    pub delivery: Delivery,
    pub reminded_at: Option<Timestamp>,
}

/// The outcome of one delivery attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryUpdate {
    pub state: DeliveryState,
    pub route: Option<Route>,
    pub error: Option<DeliveryError>,
    /// The total attempts so far, not the number added.
    pub attempts: u32,
}

pub(crate) const DELIVERY_COLS: &str = "d.message_ulid, d.recipient, d.state, d.attempts, d.route, \
     d.error_code, d.error_detail, d.reminded_at, d.acked_at";

pub(crate) fn delivery_from_row(row: &Row<'_>) -> Result<DeliveryRecord, String> {
    delivery_from_row_at(row, 0)
}

/// Parses the `DELIVERY_COLS` that start at column `o`, for queries that
/// select other columns first.
pub(crate) fn delivery_from_row_at(row: &Row<'_>, o: usize) -> Result<DeliveryRecord, String> {
    let attempts: i64 = col(row, o + 3)?;
    let code: Option<String> = col(row, o + 5)?;
    let detail: Option<String> = col(row, o + 6)?;
    let error = match code {
        Some(code) => Some(DeliveryError {
            code: DeliveryErrorCode::parse(&code).map_err(|e| bad("error code", e))?,
            detail: detail.unwrap_or_default(),
        }),
        None => None,
    };
    Ok(DeliveryRecord {
        delivery: Delivery {
            message: ulid_col(row, o)?,
            recipient: parsed(row, o + 1, "recipient", Recipient::parse)?,
            state: parsed(row, o + 2, "state", DeliveryState::parse)?,
            attempts: u32::try_from(attempts).map_err(|e| bad("attempts", e))?,
            route: parsed_opt(row, o + 4, "route", Route::parse)?,
            error,
            acked_at: ts_opt(row, o + 8)?,
        },
        reminded_at: ts_opt(row, o + 7)?,
    })
}

/// Forward moves only: `acked` is final, `pending` never comes back, an
/// `unconfirmed` copy may have been typed so it never becomes `undelivered`,
/// and a `handed_off` line can only be acknowledged.
fn may_move(from: DeliveryState, to: DeliveryState) -> bool {
    match (from, to) {
        (DeliveryState::Acked, _)
        | (_, DeliveryState::Pending)
        | (DeliveryState::Unconfirmed, DeliveryState::Undelivered) => false,
        (DeliveryState::HandedOff, to) => to == DeliveryState::Acked,
        _ => true,
    }
}

fn outcome_event(state: DeliveryState) -> EventType {
    match state {
        DeliveryState::Pending | DeliveryState::HandedOff => EventType::DeliveryHandedOff,
        DeliveryState::Unconfirmed => EventType::DeliveryUnconfirmed,
        DeliveryState::Undelivered => EventType::DeliveryUndelivered,
        DeliveryState::Acked => EventType::DeliveryAcked,
    }
}

fn seconds_since(from: Timestamp, at: Timestamp) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "milliseconds since a message was sent fit an f64 exactly for any real span"
    )]
    let millis = at.saturating_since(from).as_millis() as f64;
    millis / 1000.0
}

impl Tx<'_> {
    fn delivery_or_missing(
        &self,
        message: &Ulid,
        recipient: &Recipient,
    ) -> Result<(DeliveryRecord, Timestamp), StoreError> {
        let reader = self.reader();
        let record = reader.delivery(message, recipient)?.ok_or_else(|| {
            StoreError::Constraint(format!("no delivery of {message} to {recipient}"))
        })?;
        let sent = reader
            .message(message)?
            .map(|m| m.created_at)
            .ok_or_else(|| StoreError::Constraint(format!("no such message {message}")))?;
        Ok((record, sent))
    }

    /// Records the outcome of an attempt to hand a message to one recipient
    /// (`handed_off`, `unconfirmed`, `undelivered`) and writes the matching
    /// `delivery.*` event with route, attempts, code and seconds since the
    /// message was sent. A move that goes backward is refused here, whatever
    /// the caller thinks the state is.
    pub fn set_delivery_state(
        &self,
        at: Timestamp,
        message: &Ulid,
        recipient: &Recipient,
        update: &DeliveryUpdate,
    ) -> Result<(), StoreError> {
        let (current, sent) = self.delivery_or_missing(message, recipient)?;
        let from = current.delivery.state;
        if update.state == DeliveryState::Acked || !may_move(from, update.state) {
            return Err(StoreError::Constraint(format!(
                "delivery of {message} to {recipient} cannot move from {from} to {}",
                update.state
            )));
        }
        self.connection().execute(
            "UPDATE deliveries SET state = ?3, attempts = ?4, route = ?5, error_code = ?6,
                    error_detail = ?7
             WHERE message_ulid = ?1 AND recipient = ?2 AND state = ?8",
            rusqlite::params![
                message.to_string(),
                recipient.as_str(),
                update.state.as_str(),
                update.attempts,
                update.route.map(Route::as_str),
                update.error.as_ref().map(|e| e.code.as_str()),
                update.error.as_ref().map(|e| e.detail.as_str()),
                from.as_str(),
            ],
        )?;
        self.record_event(
            at,
            outcome_event(update.state),
            recipient_role(recipient),
            Some(message),
            &json!({
                "route": update.route.map(Route::as_str),
                "attempts": update.attempts,
                "code": update.error.as_ref().map(|e| e.code.as_str()),
                "seconds": seconds_since(sent, at),
            }),
        )?;
        Ok(())
    }

    /// The recipient read the message. Moves any state but `acked` to
    /// `acked` and writes `delivery.acked`. False, and no event, when it was
    /// already acked: reading twice changes nothing.
    pub fn ack_delivery(
        &self,
        at: Timestamp,
        message: &Ulid,
        recipient: &Recipient,
    ) -> Result<bool, StoreError> {
        let (current, sent) = self.delivery_or_missing(message, recipient)?;
        if current.delivery.state == DeliveryState::Acked {
            return Ok(false);
        }
        self.connection().execute(
            "UPDATE deliveries SET state = 'acked', acked_at = ?3
             WHERE message_ulid = ?1 AND recipient = ?2 AND state != 'acked'",
            rusqlite::params![message.to_string(), recipient.as_str(), at.to_string()],
        )?;
        self.record_event(
            at,
            EventType::DeliveryAcked,
            recipient_role(recipient),
            Some(message),
            &json!({ "seconds": seconds_since(sent, at) }),
        )?;
        Ok(true)
    }

    /// Starts one more attempt of an explicit resend: `attempts + 1`, state
    /// unchanged, `delivery.resent` written. Only `pending`, `unconfirmed`
    /// and `undelivered` rows can be resent. Returns the new attempt number.
    pub fn begin_resend(
        &self,
        at: Timestamp,
        message: &Ulid,
        recipient: &Recipient,
    ) -> Result<u32, StoreError> {
        let (current, _) = self.delivery_or_missing(message, recipient)?;
        let from = current.delivery.state;
        if matches!(from, DeliveryState::HandedOff | DeliveryState::Acked) {
            return Err(StoreError::Constraint(format!(
                "delivery of {message} to {recipient} is {from} and cannot be resent"
            )));
        }
        let attempt = current.delivery.attempts + 1;
        self.connection().execute(
            "UPDATE deliveries SET attempts = ?3 WHERE message_ulid = ?1 AND recipient = ?2",
            rusqlite::params![message.to_string(), recipient.as_str(), attempt],
        )?;
        self.record_event(
            at,
            EventType::DeliveryResent,
            recipient_role(recipient),
            Some(message),
            &json!({ "attempt": attempt, "previous_state": from.as_str() }),
        )?;
        Ok(attempt)
    }

    /// A hook reminded the recipient of an unread message: sets
    /// `reminded_at` and writes `delivery.reminded`.
    pub fn mark_reminded(
        &self,
        at: Timestamp,
        message: &Ulid,
        recipient: &Recipient,
    ) -> Result<(), StoreError> {
        let (current, sent) = self.delivery_or_missing(message, recipient)?;
        if current.delivery.state == DeliveryState::Acked {
            return Err(StoreError::Constraint(format!(
                "delivery of {message} to {recipient} was already read"
            )));
        }
        self.connection().execute(
            "UPDATE deliveries SET reminded_at = ?3 WHERE message_ulid = ?1 AND recipient = ?2",
            rusqlite::params![message.to_string(), recipient.as_str(), at.to_string()],
        )?;
        self.record_event(
            at,
            EventType::DeliveryReminded,
            recipient_role(recipient),
            Some(message),
            &json!({ "seconds": seconds_since(sent, at) }),
        )?;
        Ok(())
    }
}

fn recipient_role(recipient: &Recipient) -> Option<&teamwire_core::Role> {
    match recipient {
        Recipient::Lead => None,
        Recipient::Member(role) => Some(role),
    }
}

impl ReadTx<'_> {
    pub fn delivery(
        &self,
        message: &Ulid,
        recipient: &Recipient,
    ) -> Result<Option<DeliveryRecord>, StoreError> {
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {DELIVERY_COLS} FROM deliveries d WHERE d.message_ulid = ?1 AND d.recipient = ?2"
        ))?;
        one(listing(
            "deliveries",
            &mut stmt,
            [message.to_string(), recipient.as_str().to_owned()],
            delivery_from_row,
        )?)
    }

    /// Every delivery of one message.
    pub fn deliveries_of(&self, message: &Ulid) -> Result<Listing<DeliveryRecord>, StoreError> {
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {DELIVERY_COLS} FROM deliveries d WHERE d.message_ulid = ?1 ORDER BY d.rowid"
        ))?;
        listing(
            "deliveries",
            &mut stmt,
            [message.to_string()],
            delivery_from_row,
        )
    }

    /// The deliveries to `recipient` in any of `states`, in the order their
    /// messages were stored: the inbox and the unread list.
    pub fn deliveries_for(
        &self,
        recipient: &Recipient,
        states: &[DeliveryState],
    ) -> Result<Listing<DeliveryRecord>, StoreError> {
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {DELIVERY_COLS} FROM deliveries d
             JOIN messages m ON m.ulid = d.message_ulid
             WHERE d.recipient = ?1 AND d.state IN ({}) ORDER BY m.rowid",
            state_list(states)
        ))?;
        listing(
            "deliveries",
            &mut stmt,
            [recipient.as_str()],
            delivery_from_row,
        )
    }

    /// Every delivery in any of `states`, for the watch and `status`.
    pub fn deliveries_in_states(
        &self,
        states: &[DeliveryState],
    ) -> Result<Listing<DeliveryRecord>, StoreError> {
        let mut stmt = self.connection().prepare(&format!(
            "SELECT {DELIVERY_COLS} FROM deliveries d
             JOIN messages m ON m.ulid = d.message_ulid
             WHERE d.state IN ({}) ORDER BY m.rowid, d.rowid",
            state_list(states)
        ))?;
        listing("deliveries", &mut stmt, [], delivery_from_row)
    }
}

/// `'a', 'b'` for a SQL `IN` list. The values are the fixed strings of a
/// closed enum, never caller text.
fn state_list(states: &[DeliveryState]) -> String {
    states
        .iter()
        .map(|s| format!("'{}'", s.as_str()))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests;
