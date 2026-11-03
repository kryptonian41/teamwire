//! The database checks of `teamwire doctor`: each team's database opens
//! read-only, its schema, `quick_check`, the close lock, the message
//! counters, members another build started, and the watch lease. Reads only.

use teamwire_core::platform::ProcessRef;
use teamwire_core::{Config, ReportState, TeamName, TeamState};
use teamwire_store::{Ctx, SCHEMA_VERSION, Store};

use crate::cmd::doctor::{Check, Inputs};

/// The database checks of one team: opens, `quick_check`, the close lock and
/// the watch lease.
fn team_checks(inputs: &Inputs<'_>, team: &TeamName) -> Vec<Check> {
    let name = |what: &str| format!("team {team} {what}");
    let mut store = match Store::open_read_only(&inputs.home, team) {
        Ok(store) => store,
        Err(e) => return vec![Check::fail(name("database"), e.to_string())],
    };
    let mut checks = Vec::new();
    match store.schema_version() {
        Ok(v) if v == SCHEMA_VERSION => {
            checks.push(Check::pass(name("schema"), format!("version {v}")));
        }
        // A read-only open refuses a newer schema, so this is an older one.
        // Reads work on it, and the next command that writes migrates it.
        Ok(v) => checks.push(Check::pass(
            name("schema"),
            format!("version {v}; the next write migrates it to {SCHEMA_VERSION}"),
        )),
        Err(e) => checks.push(Check::fail(name("schema"), e.to_string())),
    }
    match store.quick_check() {
        Ok(text) if text == "ok" => checks.push(Check::pass(name("quick_check"), "ok")),
        Ok(text) => checks.push(Check::fail(
            name("quick_check"),
            format!("the database has problems: {text}"),
        )),
        Err(e) => checks.push(Check::fail(name("quick_check"), e.to_string())),
    }
    let rows = store.read(|tx| Ok((tx.team()?, tx.watch_lease()?)));
    match rows {
        Ok((team_row, lease)) => {
            if team_row
                .as_ref()
                .is_some_and(|r| r.state == TeamState::Open)
            {
                checks.push(older_members_check(inputs, &mut store, team));
            }
            // The pid with its recorded start time: a pid that a later
            // process reused is gone, as `close --clear-stale-lock` judges it.
            if let Some(row) = team_row
                && row.state == TeamState::Closing
                && let Some(pid) = row.closing_pid
                && !inputs.platform.process_alive(ProcessRef {
                    pid,
                    started: row.closing_started,
                })
            {
                checks.push(Check::fail(
                    name("close lock"),
                    format!(
                        "held by pid {pid}, which is gone; run \
                         `teamwire close {team} --clear-stale-lock`"
                    ),
                ));
            }
            checks.push(counters_check(&mut store, team));
            if let Some(lease) = lease
                && teamwire_deliver::watch_lease_is_dead(
                    &lease,
                    inputs.platform,
                    inputs.clock.now(),
                )
            {
                // The same rule as `status`: a live pid with an old heartbeat
                // is a pid the system gave to another process.
                let why = if inputs.platform.pid_alive(lease.pid) {
                    "is alive but has not beaten for over 600 s"
                } else {
                    "is gone"
                };
                checks.push(Check::warn(
                    name("watch lease"),
                    format!(
                        "held by pid {}, which {why}; the next `teamwire watch {team}` takes it over",
                        lease.pid
                    ),
                ));
            }
        }
        Err(e) => checks.push(Check::fail(name("rows"), e.to_string())),
    }
    checks
}

/// A message counter below the largest value in use makes every later send
/// collide. Doctor only prints the repair, so the user decides to run it.
fn counters_check(store: &mut Store, team: &TeamName) -> Check {
    let name = format!("team {team} counters");
    let found = store.read(|tx| Ok((tx.counter_floors()?, tx.counters()?.all_or_corrupt()?)));
    let (floors, counters) = match found {
        Ok(found) => found,
        Err(e) => return Check::fail(name, e.to_string()),
    };
    let damaged: Vec<String> = floors
        .iter()
        .filter_map(|(counter, used)| {
            let stored = counters.iter().find(|(n, _)| n == counter).map(|c| c.1);
            (stored.unwrap_or(0) < *used).then(|| {
                let now = stored.map_or_else(|| "missing".to_owned(), |v| v.to_string());
                // An upsert, because a missing row is also damage and an
                // UPDATE of it would change nothing.
                format!(
                    "counter {counter} is {now} but {used} is in use; repair with: \
                     INSERT INTO counters (name, value) VALUES ('{counter}', {used}) \
                     ON CONFLICT (name) DO UPDATE SET value = excluded.value;"
                )
            })
        })
        .collect();
    if damaged.is_empty() {
        return Check::pass(name, "every message counter is at or above its largest id");
    }
    Check::fail(name, damaged.join(" "))
}

/// The members `status` marks as started by another build of teamwire. The
/// check reuses the status view, so doctor and `status` cannot disagree.
fn older_members_check(inputs: &Inputs<'_>, store: &mut Store, team: &TeamName) -> Check {
    let name = format!("team {team} older members");
    let config = Config::default();
    let ctx = Ctx {
        home: &inputs.home,
        store,
        registry: inputs.registry,
        platform: inputs.platform,
        clock: inputs.clock,
        config: &config,
    };
    let view = teamwire_watch::status_with(&ctx, None);
    if !view.errors.is_empty() {
        return Check::warn(
            name,
            format!("could not read the members: {}", view.errors.join("; ")),
        );
    }
    let older: Vec<String> = view
        .members
        .iter()
        // A member whose result is settled has finished, so relaunching it
        // changes nothing (the rule of the lead's Stop hook). A result still
        // pending or delivering is what the watch's UNREPORTED line is about,
        // so that member stays listed.
        .filter(|m| {
            m.older_build
                && m.result
                    .is_none_or(|r| matches!(r, ReportState::Pending | ReportState::Delivering))
        })
        .map(|m| format!("{} ({})", m.role, m.started_by.as_deref().unwrap_or("?")))
        .collect();
    if older.is_empty() {
        return Check::pass(name, "no member was started by another build");
    }
    Check::warn(
        name,
        format!(
            "{} started by a build other than {}; relaunch the member, or ignore if its \
             wiring did not change. A member started before builds recorded their \
             commit shows as 0.0.0",
            older.join(", "),
            teamwire_store::WRITER_VERSION
        ),
    )
}

pub(crate) fn database_checks(inputs: &Inputs<'_>) -> Vec<Check> {
    let teams = match teamwire_store::team_names(&inputs.home) {
        Ok(teams) => teams,
        Err(e) => return vec![Check::fail("databases", e.to_string())],
    };
    if teams.is_empty() {
        return vec![Check::pass(
            "databases",
            format!("no teams under {}", inputs.home.display()),
        )];
    }
    teams.iter().flat_map(|t| team_checks(inputs, t)).collect()
}
