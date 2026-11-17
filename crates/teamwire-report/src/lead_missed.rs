//! `teamwire lead --missed` (ADR 0051): the results and messages that may
//! have reached the old lead while it handed off, so the new lead checks
//! them. Reads only.
//!
//! The window of one team starts 120 seconds before the old lead's handoff
//! document was last written, and ends when the team adopted the new lead.
//! An item is left out only when its newest delivery event is before the
//! window. Its creation time alone never leaves it out: a result made long
//! before the document can wait behind text in the lead's input box and be
//! typed after the document was written.

use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use teamwire_core::{
    DeliveryState, EventType, HandoffFiles, Recipient, SessionId, TeamName, Timestamp, Ulid,
};
use teamwire_deliver::result_path;
use teamwire_store::{ReadTx, Store, StoreError};

/// How long before the document's last write the window starts: the old
/// lead may have read nothing new while it wrote the document.
const BEFORE_DOCUMENT: Duration = Duration::from_secs(120);
/// The window when the document is gone.
const NO_DOCUMENT: Duration = Duration::from_secs(3600);

/// The delivery events that say a result reached the lead.
const RESULT_EVENTS: [EventType; 1] = [EventType::ResultDelivered];
/// The delivery events that say a message reached the lead.
const MESSAGE_EVENTS: [EventType; 3] = [
    EventType::DeliveryHandedOff,
    EventType::DeliveryAcked,
    EventType::DeliveryResent,
];
const EVERY_STATE: [DeliveryState; 5] = [
    DeliveryState::Pending,
    DeliveryState::HandedOff,
    DeliveryState::Unconfirmed,
    DeliveryState::Undelivered,
    DeliveryState::Acked,
];

/// One line per result and per message to the lead that may have reached
/// the old lead while it handed off, in each of `teams`, which `session`
/// leads. Each line names the team, the item, its newest delivery event or
/// `not delivered`, and the file to read. A team with no handoff to
/// `session` says so. With nothing to list, one line says nothing was
/// missed.
pub fn lead_missed(
    home: &Path,
    session: &SessionId,
    teams: &[TeamName],
) -> Result<Vec<String>, StoreError> {
    let mut lines = Vec::new();
    let mut listed = 0;
    for team in teams {
        let store = Store::open_read_only(home, team)?;
        let (notes, items) = store.read(|tx| team_missed(home, team, session, tx))?;
        listed += items.len();
        lines.extend(notes);
        lines.extend(items);
    }
    if listed == 0 {
        lines.push(
            "nothing was missed: no result or message reached the old lead while it handed off"
                .to_owned(),
        );
    }
    Ok(lines)
}

/// The notes and the listed items of one team.
fn team_missed(
    home: &Path,
    team: &TeamName,
    session: &SessionId,
    tx: &ReadTx<'_>,
) -> Result<(Vec<String>, Vec<String>), StoreError> {
    let adopted = tx
        .events_of_type(EventType::TeamLeadAdopted)?
        .all_or_corrupt()?
        .into_iter()
        .rev()
        .find(|e| text(&e.data, "new_session") == Some(session.as_str()));
    let Some(adopted) = adopted else {
        return Ok((
            vec![format!(
                "team {team}: no handoff to this session is recorded"
            )],
            Vec::new(),
        ));
    };
    let end = adopted.time;
    let old = text(&adopted.data, "old_session").and_then(|s| SessionId::parse(s).ok());
    let written = old
        .as_ref()
        .and_then(|o| HandoffFiles::of(home, o))
        .and_then(|f| modified(&f.document));
    let mut notes = Vec::new();
    let start = if let Some(written) = written {
        before(written, BEFORE_DOCUMENT)
    } else {
        notes.push(format!(
            "team {team}: the old lead's handoff document is gone, so the window starts one hour \
             before the handoff"
        ));
        before(end, NO_DOCUMENT)
    };
    let team_dir = home.join("teams").join(team.as_str());
    let mut items = Vec::new();
    for record in tx.completions()?.all_or_corrupt()? {
        let c = record.completion;
        if c.created_at > end {
            continue;
        }
        let Some(when) = newest(tx, &c.ulid, &RESULT_EVENTS, None, start)? else {
            continue;
        };
        let path = result_path(&team_dir, &c.role, record.result_sha256.as_ref());
        items.push(format!(
            "team {team}: {}'s result #{} ({}), {when}: read {path}",
            c.role, c.n, c.status
        ));
    }
    for record in tx
        .deliveries_for(&Recipient::Lead, &EVERY_STATE)?
        .all_or_corrupt()?
    {
        let Some(m) = tx.message(&record.delivery.message)? else {
            continue;
        };
        if m.created_at > end {
            continue;
        }
        let to = Some(Recipient::Lead.as_str());
        let Some(when) = newest(tx, &m.ulid, &MESSAGE_EVENTS, to, start)? else {
            continue;
        };
        let file = team_dir.join("comms").join(format!("{}.md", m.display_id));
        items.push(format!(
            "team {team}: message {} ({} from {}), {when}: read {}",
            m.display_id,
            m.kind,
            m.sender,
            file.display()
        ));
    }
    Ok((notes, items))
}

/// How the item's newest delivery event reads, or `None` when that event is
/// before `start`. An item with no such event is `not delivered`.
fn newest(
    tx: &ReadTx<'_>,
    subject: &Ulid,
    kinds: &[EventType],
    to: Option<&str>,
    start: Timestamp,
) -> Result<Option<String>, StoreError> {
    let newest = tx
        .events_about(subject)?
        .all_or_corrupt()?
        .into_iter()
        .filter(|e| kinds.contains(&e.kind))
        .filter(|e| to.is_none_or(|to| text(&e.data, "to") == Some(to)))
        .max_by_key(|e| e.seq);
    Ok(match newest {
        None => Some("not delivered".to_owned()),
        Some(e) if e.time < start => None,
        Some(e) => Some(format!("{} at {}", e.kind, e.time)),
    })
}

fn text<'a>(data: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    data.get(key).and_then(serde_json::Value::as_str)
}

fn modified(path: &Path) -> Option<Timestamp> {
    let at = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
    let millis = at.duration_since(UNIX_EPOCH).ok()?.as_millis();
    Timestamp::from_unix_millis(i64::try_from(millis).ok()?)
}

fn before(at: Timestamp, by: Duration) -> Timestamp {
    let by = i64::try_from(by.as_millis()).unwrap_or(i64::MAX);
    Timestamp::from_unix_millis(at.unix_millis().saturating_sub(by)).unwrap_or(at)
}
