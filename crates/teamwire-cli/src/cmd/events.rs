//! `teamwire events`: a team's event rows, oldest first, for the dashboard
//! and for anyone who wants the timeline without opening the store.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;
use serde_json::Value;
use teamwire_core::TeamName;
use teamwire_store::{Store, StoreError, database_path};

use crate::cmd::send::{REFUSED, STORE, carries_member_identity, fail, home_or_refuse, say};
use crate::{EventsArgs, Global, USAGE};

#[derive(Serialize)]
struct EventsView {
    team: String,
    source: &'static str,
    archive: Option<String>,
    last_seq: u64,
    events: Vec<Row>,
    errors: Vec<String>,
}

#[derive(Serialize)]
struct Row {
    seq: u64,
    time: String,
    #[serde(rename = "type")]
    kind: String,
    role: Option<String>,
    message: Option<String>,
    data: Value,
}

/// Where the rows come from.
struct Source {
    db: PathBuf,
    /// The archive's directory name; `None` for a live store.
    archive: Option<String>,
    /// An archive copied without its `SHA256SUMS` may be cut short.
    unchecked: bool,
}

pub(crate) fn run(args: &EventsArgs, global: &Global, _wiring: &crate::wiring::Wiring) -> ExitCode {
    let EventsArgs {
        team,
        archive,
        after,
        json,
    } = args;
    let team = match TeamName::parse(team) {
        Ok(team) => team,
        Err(e) => return fail("events", &e.to_string(), USAGE),
    };
    // The rows name every member's work, so only the person at the terminal reads them.
    if carries_member_identity() {
        return fail(
            "events",
            "refused.identity: the event rows are for the lead; a member session cannot read them",
            REFUSED,
        );
    }
    let home = match home_or_refuse("events", global) {
        Ok(home) => home,
        Err(code) => return code,
    };
    let mut errors = Vec::new();
    let source = match locate(&home, &team, archive.as_deref(), &mut errors) {
        Ok(source) => source,
        Err(message) => return fail("events", &message, REFUSED),
    };
    let view = match read(&source, &team, *after, errors) {
        Ok(view) => view,
        Err(Failure::Refused(message)) => return fail("events", &message, REFUSED),
        Err(Failure::Store(e)) => return fail("events", &e.to_string(), STORE),
    };
    if *json {
        match serde_json::to_string_pretty(&view) {
            Ok(text) => say(&format!("{text}\n")),
            Err(e) => return fail("events", &e.to_string(), STORE),
        }
    } else {
        for row in &view.events {
            say(&format!(
                "{} {} {} {} {}\n",
                row.time,
                row.seq,
                row.kind,
                row.role.as_deref().unwrap_or("-"),
                row.message.as_deref().unwrap_or("-"),
            ));
        }
    }
    ExitCode::SUCCESS
}

fn locate(
    home: &Path,
    team: &TeamName,
    archive: Option<&str>,
    errors: &mut Vec<String>,
) -> Result<Source, String> {
    if let Some(name) = archive {
        if !archive_name_ok(team, name) {
            return Err(format!(
                "invalid archive {name:?}: write {team}-YYYY-MM-DD or {team}-YYYY-MM-DD-HHMMSS"
            ));
        }
        let dir = home.join("archive").join(name);
        if !dir.join("team.db").is_file() {
            return Err(format!("no archive {name} for team {team}"));
        }
        return Ok(Source {
            db: dir.join("team.db"),
            archive: Some(name.to_owned()),
            unchecked: !dir.join("SHA256SUMS").is_file(),
        });
    }
    let live = database_path(home, team);
    if live.is_file() {
        return Ok(Source {
            db: live,
            archive: None,
            unchecked: false,
        });
    }
    // A closed team: close deleted its database, so read its newest archive.
    let dir = teamwire_otel::newest_archive_dir_of(home, team, errors)
        .ok_or_else(|| format!("no database for team {team}"))?;
    Ok(Source {
        db: dir.join("team.db"),
        archive: dir.file_name().map(|n| n.to_string_lossy().into_owned()),
        unchecked: false,
    })
}

/// `<team>-YYYY-MM-DD`, `<team>-YYYY-MM-DD-HHMMSS` or
/// `<team>-YYYY-MM-DD-HHMMSS-<n>`, the names `close` gives an archive. A team name holds no hyphen, so the prefix is unambiguous.
fn archive_name_ok(team: &TeamName, name: &str) -> bool {
    let Some(stamp) = name
        .strip_prefix(team.as_str())
        .and_then(|rest| rest.strip_prefix('-'))
    else {
        return false;
    };
    let shape = |text: &str, pattern: &str| {
        text.len() == pattern.len()
            && text.bytes().zip(pattern.bytes()).all(|(c, p)| match p {
                b'9' => c.is_ascii_digit(),
                _ => c == p,
            })
    };
    let numbered = stamp.get(18..).is_some_and(|n| {
        stamp.as_bytes().get(17) == Some(&b'-')
            && !n.is_empty()
            && !n.starts_with('0')
            && n.bytes().all(|c| c.is_ascii_digit())
    });
    shape(stamp, "9999-99-99")
        || shape(stamp, "9999-99-99-999999")
        || (numbered
            && stamp
                .get(..17)
                .is_some_and(|t| shape(t, "9999-99-99-999999")))
}

enum Failure {
    Refused(String),
    Store(StoreError),
}

impl From<StoreError> for Failure {
    fn from(e: StoreError) -> Self {
        Self::Store(e)
    }
}

fn read(
    source: &Source,
    team: &TeamName,
    after: u64,
    mut errors: Vec<String>,
) -> Result<EventsView, Failure> {
    let store = Store::open_read_only_at(&source.db)?;
    let (last_seq, found, named, messages) = store.read(|tx| {
        let found = tx.events_after(after, None, None)?;
        let mut messages = Vec::with_capacity(found.items.len());
        for event in &found.items {
            let id = match event.subject.as_ref().map(|ulid| tx.message(ulid)) {
                Some(Ok(message)) => message.map(|m| m.display_id.to_string()),
                // A damaged message must not hide the event that names it.
                Some(Err(StoreError::Corrupt(_))) | None => None,
                Some(Err(e)) => return Err(e),
            };
            messages.push(id);
        }
        Ok((
            tx.last_event_seq()?,
            found,
            tx.team()?.map(|t| t.name),
            messages,
        ))
    })?;
    if source.archive.is_some() && named.as_ref() != Some(team) {
        return Err(Failure::Refused(format!(
            "archive {} is not an archive of team {team}",
            source.archive.as_deref().unwrap_or_default()
        )));
    }
    if source.unchecked {
        errors.push("the archive has no SHA256SUMS file: the copy may be incomplete".to_owned());
    }
    errors.extend(
        found
            .bad
            .iter()
            .map(|bad| format!("skipped {} row {}: {}", bad.table, bad.key, bad.reason)),
    );
    let events = found
        .items
        .into_iter()
        .zip(messages)
        .map(|(e, message)| Row {
            seq: e.seq,
            time: e.time.to_string(),
            kind: e.kind.as_str().to_owned(),
            role: e.role.map(|r| r.as_str().to_owned()),
            message,
            data: e.data,
        })
        .collect();
    Ok(EventsView {
        team: team.as_str().to_owned(),
        source: if source.archive.is_some() {
            "archive"
        } else {
            "live"
        },
        archive: source.archive.clone(),
        last_seq,
        events,
        errors,
    })
}
