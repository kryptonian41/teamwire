//! Reads the "Decisions I made" blocks of members' reports, for one team,
//! for the teams with a ticket ref, or for every block that matches a text.
//! Nothing is written.

mod find;

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use teamwire_core::{Member, MemberKind, TeamName, TicketRef};
use teamwire_store::{Store, TeamDb, TeamRow, database_path, newest_archive, team_databases};
use thiserror::Error;

pub use find::{Block, find_blocks};

/// What to read. At least one of `team`, `grep` and `reference` must be set.
#[derive(Debug, Clone, Default)]
pub struct DecisionsQuery {
    /// The running team, else its newest archive.
    pub team: Option<TeamName>,
    /// Only this member.
    pub role: Option<String>,
    /// Only blocks whose text holds this, ignoring case.
    pub grep: Option<String>,
    /// Only teams with this ref: `provider:id` for that provider, or a bare
    /// `id` under any provider. The id is compared ignoring case.
    pub reference: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DecisionsError {
    #[error("name a team or give --ref or --grep")]
    NoFilter,
    #[error("no such team or archive: {0}")]
    NoSuchTeam(String),
}

/// The JSON of `teamwire decisions --json` and `team_decisions`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DecisionsView {
    pub teams: Vec<TeamDecisions>,
    /// Databases and reports that could not be read; the rest is still here.
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TeamDecisions {
    pub team: String,
    pub title: String,
    pub ulid: String,
    pub created_at: String,
    /// A running team, not an archive.
    pub live: bool,
    /// The folder that holds the team's database and its `reports/`.
    pub path: String,
    pub refs: Vec<TicketRef>,
    pub members: Vec<MemberDecisions>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemberDecisions {
    pub role: String,
    pub kind: MemberKind,
    pub title: String,
    /// The report file, or `None` when the member wrote none.
    pub report: Option<String>,
    pub blocks: Vec<Block>,
}

impl DecisionsView {
    /// Keeps the first `limit` blocks in print order and drops what comes
    /// after the cut. A member before the cut stays, even with no report.
    /// Returns whether anything was dropped.
    pub fn limit_blocks(&mut self, limit: usize) -> bool {
        let mut left = limit;
        let mut cut = None;
        'teams: for (t, team) in self.teams.iter_mut().enumerate() {
            for (m, member) in team.members.iter_mut().enumerate() {
                if member.blocks.len() > left {
                    member.blocks.truncate(left);
                    team.members.truncate(if left == 0 { m } else { m + 1 });
                    cut = Some(if team.members.is_empty() { t } else { t + 1 });
                    break 'teams;
                }
                left -= member.blocks.len();
            }
        }
        let Some(keep) = cut else {
            return false;
        };
        self.teams.truncate(keep);
        true
    }
}

/// One team database with the rows this reader needs.
struct Loaded {
    db: TeamDb,
    team: TeamRow,
    members: Vec<Member>,
}

pub fn read_decisions(
    home: &Path,
    query: &DecisionsQuery,
) -> Result<DecisionsView, DecisionsError> {
    if query.team.is_none() && query.grep.is_none() && query.reference.is_none() {
        return Err(DecisionsError::NoFilter);
    }
    let mut errors = Vec::new();
    let mut loaded = match &query.team {
        Some(team) => {
            let db = of_team(home, team, &mut errors)
                .ok_or_else(|| DecisionsError::NoSuchTeam(team.to_string()))?;
            load(db, &mut errors).into_iter().collect()
        }
        None => one_copy_of_each_team(
            team_databases(home, &mut errors)
                .into_iter()
                .filter_map(|db| load(db, &mut errors))
                .collect(),
        ),
    };
    loaded.sort_by(|a, b| {
        (a.team.created_at, a.team.name.as_str()).cmp(&(b.team.created_at, b.team.name.as_str()))
    });
    let grep = query.grep.as_deref().map(str::to_lowercase);
    let mut teams = Vec::new();
    for item in loaded {
        if let Some(reference) = &query.reference
            && !has_ref(&item.team.refs, reference)
        {
            continue;
        }
        let members = members_of(&item, query.role.as_deref(), grep.as_deref(), &mut errors);
        if grep.is_some() && members.is_empty() {
            continue;
        }
        let dir = item.db.path.parent().unwrap_or(&item.db.path);
        teams.push(TeamDecisions {
            team: item.team.name.to_string(),
            title: item.team.title.clone(),
            ulid: item.team.ulid.to_string(),
            created_at: item.team.created_at.to_string(),
            live: item.db.live,
            path: dir.display().to_string(),
            refs: item.team.refs.clone(),
            members,
        });
    }
    Ok(DecisionsView { teams, errors })
}

/// Whether `refs` holds `wanted`. `provider:id` needs that provider (when the
/// part before the first colon is a valid provider name, else the whole text
/// is an id); a bare id matches under any provider.
fn has_ref(refs: &[TicketRef], wanted: &str) -> bool {
    let (provider, id) = match wanted.split_once(':') {
        Some((p, id)) if is_provider_name(p) => (Some(p), id),
        _ => (None, wanted),
    };
    refs.iter()
        .any(|r| r.id.eq_ignore_ascii_case(id) && provider.is_none_or(|p| r.provider == p))
}

/// `^[a-z][a-z0-9_]{0,15}$`.
fn is_provider_name(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && text.len() <= 16
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// The running team's database, else its newest archive.
fn of_team(home: &Path, team: &TeamName, errors: &mut Vec<String>) -> Option<TeamDb> {
    let path = database_path(home, team);
    if path.is_file() {
        return Some(TeamDb {
            path,
            live: true,
            rank: None,
        });
    }
    newest_archive(home, team, errors)
}

fn load(db: TeamDb, errors: &mut Vec<String>) -> Option<Loaded> {
    let read = Store::open_read_only_at(&db.path).and_then(|store| {
        store.read(|tx| {
            let members = tx.members()?;
            Ok((tx.team()?, members))
        })
    });
    match read {
        Ok((Some(team), members)) => {
            for bad in &members.bad {
                errors.push(format!("{}: {bad:?}", db.path.display()));
            }
            Some(Loaded {
                db,
                team,
                members: members.items,
            })
        }
        Ok((None, _)) => {
            errors.push(format!(
                "{}: the database has no team row",
                db.path.display()
            ));
            None
        }
        Err(e) => {
            errors.push(format!("{}: {e}", db.path.display()));
            None
        }
    }
}

/// An archive is a full copy of a team, so a team seen running and archived,
/// or archived twice, is read once: the running copy, else the newest
/// archive. The team row's ULID names the team, whatever the folder is called.
fn one_copy_of_each_team(loaded: Vec<Loaded>) -> Vec<Loaded> {
    let mut best: BTreeMap<String, Loaded> = BTreeMap::new();
    for item in loaded {
        let key = item.team.ulid.to_string();
        let better = match best.get(&key) {
            None => true,
            Some(kept) if kept.db.live => false,
            Some(_) if item.db.live => true,
            Some(kept) => item.db.rank > kept.db.rank,
        };
        if better {
            best.insert(key, item);
        }
    }
    best.into_values().collect()
}

fn members_of(
    item: &Loaded,
    role: Option<&str>,
    grep: Option<&str>,
    errors: &mut Vec<String>,
) -> Vec<MemberDecisions> {
    let dir = item.db.path.parent().unwrap_or(&item.db.path);
    let mut members: Vec<&Member> = item
        .members
        .iter()
        .filter(|m| role.is_none_or(|r| m.role.as_str() == r))
        .collect();
    members.sort_by(|a, b| a.role.as_str().cmp(b.role.as_str()));
    let mut out = Vec::new();
    for member in members {
        let path = teamwire_verify::report_path(dir, &member.role);
        let (report, mut blocks) = match std::fs::read_to_string(&path) {
            Ok(text) => (Some(path.display().to_string()), find_blocks(&text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (None, Vec::new()),
            Err(e) => {
                errors.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        if let Some(needle) = grep {
            blocks.retain(|b| b.text.to_lowercase().contains(needle));
            if blocks.is_empty() {
                continue;
            }
        }
        out.push(MemberDecisions {
            role: member.role.to_string(),
            kind: member.kind,
            title: member.title.clone(),
            report,
            blocks,
        });
    }
    out
}
