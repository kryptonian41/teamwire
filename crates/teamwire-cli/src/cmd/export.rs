//! `teamwire export`: every record of a team as JSON lines.

use std::process::ExitCode;

use serde::Serialize;
use serde_json::{Value, json};
use teamwire_core::{TeamName, Timestamp};
use teamwire_store::{BadRow, Listing, MessageFilter, ReadTx, Store, StoreError, database_path};

use crate::cmd::send::home_or_refuse;
use crate::wiring::Wiring;
use crate::{ExportArgs, Global, USAGE};

/// Refused by a rule; nothing changed (docs/api/cli.md).
const REFUSED: u8 = 1;
/// The store could not be read, or the file could not be written.
const STORE: u8 = 6;

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
    let mut out = Vec::new();
    for e in keep(&mut skipped, tx.events_after(0, None, None)?) {
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
        for c in keep(&mut skipped, tx.completions_of(&m.role)?) {
            out.push(Record {
                role: Some(m.role.as_str().to_owned()),
                ..record(
                    c.completion.ulid.to_string(),
                    "completion",
                    c.completion.created_at,
                    None,
                    json!({ "completion": c.completion }),
                )
            });
        }
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
