//! `teamwire export`: every record of a team as JSON lines.

use std::process::ExitCode;

use serde::Serialize;
use serde_json::{Value, json};
use teamwire_core::{EventType, TeamName, Timestamp};
use teamwire_store::{BadRow, Listing, MessageFilter, ReadTx, Store, StoreError, database_path};

use crate::cmd::send::{REFUSED, STORE, carries_member_identity, home_or_refuse};
use crate::wiring::Wiring;
use crate::{ExportArgs, Global, USAGE};

/// One record in `CloudEvents` 1.0 names. `seq` and `role` are extension
/// attributes: they let a reader put events back in order and find a
/// member's records without looking inside `data`.
#[derive(Serialize)]
struct Record {
    specversion: &'static str,
    id: String,
    source: String,
    #[serde(rename = "type")]
    kind: String,
    time: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seq: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    data: Value,
}

pub(crate) fn run(args: &ExportArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let ExportArgs { team, out } = args;
    let team = match TeamName::parse(team) {
        Ok(team) => team,
        Err(e) => return fail(&e.to_string(), USAGE),
    };
    let home = match home_or_refuse("export", global) {
        Ok(home) => home,
        Err(code) => return code,
    };
    if !database_path(&home, &team).is_file() {
        return fail(&format!("no database for team {team}"), REFUSED);
    }
    let (text, skipped) = match Store::open_read_only(&home, &team).and_then(|s| render(&s, &team))
    {
        Ok(done) => done,
        Err(e) => return fail(&e.to_string(), STORE),
    };
    for row in &skipped {
        warn(&format!(
            "skipped {} row {}: {}",
            row.table, row.key, row.reason
        ));
    }
    if let Some(path) = out {
        return match wiring.platform.write_atomic(path, text.as_bytes()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => fail(&format!("{}: {e}", path.display()), STORE),
        };
    }
    say(&text);
    ExitCode::SUCCESS
}

/// Every event, message with its deliveries, member, completion, waiver and
/// announcement, one JSON object per line. A row that cannot be read is
/// skipped and returned, so the caller can name it: the rest of the record
/// is still worth having (STO-15).
fn render(store: &Store, team: &TeamName) -> Result<(String, Vec<BadRow>), StoreError> {
    let source = format!("teamwire/{team}");
    let (records, skipped) = store.read(|tx| records(tx, &source))?;
    let mut text = String::new();
    for record in records {
        let line = serde_json::to_string(&record)
            .map_err(|e| StoreError::Corrupt(format!("a record does not serialize: {e}")))?;
        text.push_str(&line);
        text.push('\n');
    }
    Ok((text, skipped))
}

/// The rows of `listing` that read; the ones that did not go to `skipped`.
fn keep<T>(skipped: &mut Vec<BadRow>, listing: Listing<T>) -> Vec<T> {
    skipped.extend(listing.bad);
    listing.items
}

fn records(tx: &ReadTx<'_>, source: &str) -> Result<(Vec<Record>, Vec<BadRow>), StoreError> {
    let mut skipped = Vec::new();
    let record =
        |id: String, kind: &str, time: Timestamp, subject: Option<String>, data: Value| Record {
            specversion: "1.0",
            id,
            source: source.to_owned(),
            kind: format!("teamwire.{kind}"),
            time: time.to_string(),
            subject,
            seq: None,
            role: None,
            data,
        };
    // The lead session id is what a member would need to speak as the lead,
    // so an export taken from a member's session leaves it out everywhere.
    let member = carries_member_identity();
    let mut out = Vec::new();
    for mut e in keep(&mut skipped, tx.events_after(0, None, None)?) {
        if member && e.kind == EventType::TeamLeadAdopted {
            for key in ["old_session", "new_session"] {
                if let Some(v) = e.data.get_mut(key) {
                    *v = Value::Null;
                }
            }
        }
        out.push(Record {
            seq: Some(e.seq),
            role: e.role.as_ref().map(|r| r.as_str().to_owned()),
            ..record(
                e.ulid.to_string(),
                e.kind.as_str(),
                e.time,
                e.subject.map(|s| s.to_string()),
                e.data,
            )
        });
    }
    let messages = keep(&mut skipped, tx.messages(&MessageFilter::default())?);
    for m in messages {
        let deliveries: Vec<Value> = keep(&mut skipped, tx.deliveries_of(&m.ulid)?)
            .iter()
            .map(|d| json!(d.delivery))
            .collect();
        out.push(record(
            m.ulid.to_string(),
            "message",
            m.created_at,
            Some(m.display_id.to_string()),
            json!({ "message": m, "deliveries": deliveries }),
        ));
    }
    let members = keep(&mut skipped, tx.members()?);
    for m in &members {
        out.push(Record {
            role: Some(m.role.as_str().to_owned()),
            ..record(
                format!("member:{}", m.role),
                "member",
                m.gen_started_at,
                None,
                json!(m),
            )
        });
    }
    // Read for every role, not through the member rows above: a damaged
    // member row is skipped and must not hide its completions.
    for c in keep(&mut skipped, tx.completions()?) {
        out.push(Record {
            role: Some(c.completion.role.as_str().to_owned()),
            ..record(
                c.completion.ulid.to_string(),
                "completion",
                c.completion.created_at,
                None,
                json!({ "completion": c.completion }),
            )
        });
    }
    for v in keep(&mut skipped, tx.all_verify_records()?) {
        let r = &v.record;
        out.push(Record {
            role: Some(r.role.as_str().to_owned()),
            ..record(
                format!("verify:{}", v.id),
                "verify",
                v.recorded_at,
                None,
                json!({
                    "head_sha": r.head_sha,
                    "verdict": r.verdict,
                    "exit_code": r.exit_code,
                    "seconds": r.seconds,
                    "flags": r.flags,
                    "log_sha256": r.log_sha256.map(|h| hex(&h)),
                }),
            )
        });
    }
    if let Some(lease) = tx.watch_lease()? {
        out.push(record(
            "watch_lease".to_owned(),
            "watch_lease",
            lease.heartbeat_at,
            None,
            json!({
                "pid": lease.pid,
                "host": lease.host,
                "started_at": lease.started_at.to_string(),
                "heartbeat_at": lease.heartbeat_at.to_string(),
            }),
        ));
    }
    if let Some(team) = tx.team()? {
        let lead_session = (!member)
            .then(|| team.lead_session.as_ref().map(ToString::to_string))
            .flatten();
        out.push(record(
            format!("team:{}", team.name),
            "team",
            team.created_at,
            None,
            json!({
                "ulid": team.ulid.to_string(),
                "name": team.name.as_str(),
                "title": team.title,
                "state": team.state.as_str(),
                "closing_pid": team.closing_pid,
                "closing_pid_started_at": team.closing_started.map(|t| t.to_string()),
                "lead_session": lead_session,
                "lead_kind": team.lead_kind.map(|k| k.as_str().to_owned()),
                "lead_harness": team.lead_harness.to_string(),
                "lead_host": team.lead_host.to_string(),
                "lead_route": team.lead_route.to_string(),
                "sleep_hold": team.sleep_hold.as_ref().map(|h| json!({ "pid": h.pid, "owned": h.owned })),
            }),
        ));
        for (name, value) in keep(&mut skipped, tx.counters()?) {
            out.push(record(
                format!("counter:{name}"),
                "counter",
                team.created_at,
                None,
                json!({ "name": name, "value": value }),
            ));
        }
        let mark = tx.export_mark()?;
        out.push(record(
            "metric_export".to_owned(),
            "metric_export",
            mark.claimed_at.unwrap_or(team.created_at),
            None,
            json!({
                "last_event_seq": mark.last_event_seq,
                "claimed_to": mark.claimed_to,
                "claimed_by": mark.claimed_by,
                "claimed_by_started_at": mark.claimed_by_started.map(|t| t.to_string()),
                "claimed_at": mark.claimed_at.map(|t| t.to_string()),
            }),
        ));
    }
    for w in keep(&mut skipped, tx.all_waivers()?) {
        out.push(Record {
            role: Some(w.role.as_str().to_owned()),
            ..record(
                format!("waiver:{}:{}:{}", w.role, w.flag_id, w.head_sha),
                "waiver",
                w.created_at,
                None,
                json!({ "flag": w.flag_id, "head_sha": w.head_sha, "reason": w.reason }),
            )
        });
    }
    for a in keep(&mut skipped, tx.announcements()?) {
        out.push(record(
            format!("announcement:{}", a.key),
            "announcement",
            a.at,
            None,
            json!({ "key": a.key }),
        ));
    }
    Ok((out, skipped))
}

/// Lower-case hex, the form a checksum is written in.
fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .flat_map(|b| [b >> 4, b & 0x0f])
        .filter_map(|nibble| char::from_digit(u32::from(nibble), 16))
        .collect()
}

#[expect(clippy::print_stdout, reason = "the records are the command's output")]
fn say(text: &str) {
    print!("{text}");
}

#[expect(clippy::print_stderr, reason = "the binary reports problems on stderr")]
fn warn(message: &str) {
    eprintln!("teamwire export: {message}");
}

fn fail(message: &str, code: u8) -> ExitCode {
    warn(message);
    ExitCode::from(code)
}
