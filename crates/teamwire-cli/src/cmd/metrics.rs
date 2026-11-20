//! `teamwire metrics`: local metrics, and `metrics export` to OpenTelemetry.

use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use teamwire_core::{Clock, Config, TeamName};
use teamwire_otel::{ExportError, MetricValue, MetricsQuery, MetricsView};
use teamwire_store::{Ctx, Store, database_path};

use crate::cmd::shared::{REFUSED, STORE, fail, home_or_refuse, say, warn};
use crate::recorded::Recorded;
use crate::wiring::Wiring;
use crate::{Global, MetricsArgs, MetricsCommand, USAGE};

pub(crate) fn run(args: &MetricsArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let MetricsArgs {
        team,
        since,
        json,
        action,
    } = args;
    let home = match home_or_refuse("metrics", global) {
        Ok(home) => home,
        Err(code) => return code,
    };
    match action {
        Some(MetricsCommand::Export { once }) => export(&home, wiring, *once),
        None => show(&home, wiring, team.as_deref(), since.as_deref(), *json),
    }
}

fn show(
    home: &Path,
    wiring: &Wiring,
    team: Option<&str>,
    since: Option<&str>,
    json: bool,
) -> ExitCode {
    let team = match team.map(TeamName::parse).transpose() {
        Ok(team) => team,
        Err(e) => return fail("metrics", &e.to_string(), USAGE),
    };
    let since = match since.map(parse_duration).transpose() {
        Ok(since) => since,
        Err(message) => return fail("metrics", &message, USAGE),
    };
    if let Some(team) = &team
        && !database_path(home, team).is_file()
        && !teamwire_otel::team_is_known(home, team)
    {
        if let Some(error) = unreadable_archive_of(home, team) {
            return fail("metrics", &error, STORE);
        }
        return fail("metrics", &format!("no database for team {team}"), REFUSED);
    }
    let named = team.is_some();
    let query = MetricsQuery { team, since };
    let view = teamwire_otel::metrics_at(home, wiring.clock.now(), &query);
    for error in &view.errors {
        warn(&format!("teamwire metrics: {error}"));
    }
    // A named team has one source, so an error means its store was not read
    // and an empty view would be a false "no events"; a listing goes on past
    // one bad team.
    if named && !view.errors.is_empty() {
        return ExitCode::from(STORE);
    }
    if json {
        match serde_json::to_string_pretty(&view) {
            Ok(text) => say(&format!("{text}\n")),
            Err(e) => return fail("metrics", &e.to_string(), STORE),
        }
    } else {
        say(&render(&view));
    }
    ExitCode::SUCCESS
}

/// An archive of `team` whose database cannot be opened, as an error line.
/// The metrics reader finds a closed team's archive by the team row inside
/// it, so it cannot see one it fails to open; the folder name `<team>-<date>`
/// still names the team, because a team name holds no hyphen. A folder
/// without `SHA256SUMS` is a copy cut short, which the reader skips too.
fn unreadable_archive_of(home: &Path, team: &TeamName) -> Option<String> {
    std::fs::read_dir(home.join("archive"))
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|dir| {
            dir.file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.split_once('-'))
                .is_some_and(|(name, _)| name == team.as_str())
                && dir.join("SHA256SUMS").is_file()
        })
        .map(|dir| dir.join("team.db"))
        .filter(|db| db.is_file())
        .find_map(|db| {
            Store::open_read_only_at(&db)
                .err()
                .map(|e| format!("{}: {e}", db.display()))
        })
}

/// `30s`, `15m`, `2h` or `1d`.
fn parse_duration(text: &str) -> Result<Duration, String> {
    let bad = || format!("invalid duration {text:?}: write a number and a unit, such as 30m or 2h");
    let split = text.find(|c: char| !c.is_ascii_digit()).ok_or_else(bad)?;
    let (number, unit) = text.split_at(split);
    let n: u64 = number.parse().map_err(|_| bad())?;
    let seconds = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86_400,
        _ => return Err(bad()),
    };
    Ok(Duration::from_secs(n.saturating_mul(seconds)))
}

/// One line per value: `name{label=value,...} number unit`.
fn render(view: &MetricsView) -> String {
    if view.metrics.is_empty() {
        return "no metrics: no events in range\n".to_owned();
    }
    view.metrics.iter().map(line).collect()
}

fn line(m: &MetricValue) -> String {
    let labels = if m.labels.is_empty() {
        String::new()
    } else {
        let list: Vec<String> = m.labels.iter().map(|(k, v)| format!("{k}={v}")).collect();
        format!("{{{}}}", list.join(","))
    };
    let value = if m.unit == MONEY_UNIT {
        money(m.value)
    } else if m.value.fract() == 0.0 {
        format!("{:.0}", m.value)
    } else {
        format!("{:.3}", m.value)
    };
    format!("{}{labels} {value} {}\n", m.name, m.unit)
}

/// The unit of a metric in US dollars, such as `teamwire.member.cost`.
const MONEY_UNIT: &str = "usd";

/// Dollars always to 4 places: 3 would print a reply's $0.0004 as 0.000,
/// which reads as free.
fn money(dollars: f64) -> String {
    format!("{dollars:.4}")
}

/// How long a repeating export waits between passes (ADR 0005 point 3).
const EXPORT_EVERY: Duration = Duration::from_secs(60);

/// One pass with `--once`; otherwise a pass every minute until the process
/// is stopped. A pass that finds export off ends the command either way,
/// because nothing in the environment will turn it on later.
fn export(home: &Path, wiring: &Wiring, once: bool) -> ExitCode {
    loop {
        let Some(config) = teamwire_otel::load_otlp_config(home, &|key| std::env::var(key).ok())
        else {
            warn("teamwire metrics: export is off: set OTEL_EXPORTER_OTLP_ENDPOINT to turn it on");
            return ExitCode::SUCCESS;
        };
        let code = export_pass(home, wiring, &config);
        if once || code != ExitCode::SUCCESS {
            return code;
        }
        std::thread::sleep(EXPORT_EVERY);
    }
}

/// Sends what every team's export mark has not yet covered. A missing or
/// slow collector never changes the exit code (OBS-2): the failure is
/// printed, the mark stays, and the next run sends the batch.
fn export_pass(home: &Path, wiring: &Wiring, config: &teamwire_otel::OtlpConfig) -> ExitCode {
    let teams = match teamwire_store::team_names(home) {
        Ok(teams) => teams,
        Err(e) => return fail("metrics", &e.to_string(), STORE),
    };
    let app_config = Config::default();
    for team in teams {
        let mut store = match Store::open_existing(home, &team) {
            Ok(store) => Recorded::new(store, &wiring.clock, "metrics"),
            Err(e) => {
                warn(&format!("teamwire metrics: team {team}: {e}"));
                continue;
            }
        };
        let mut ctx = Ctx {
            home,
            store: &mut store,
            registry: &wiring.registry,
            platform: wiring.platform.as_ref(),
            clock: &wiring.clock,
            config: &app_config,
        };
        match teamwire_otel::export_once(&mut ctx, config) {
            Ok(r) if r.to_seq == r.from_seq => say(&format!("{team}: nothing new to send\n")),
            Ok(r) => say(&format!(
                "{team}: sent events {} to {} ({} metric points, {} spans)\n",
                r.from_seq + 1,
                r.to_seq,
                r.metric_points,
                r.spans
            )),
            Err(ExportError::Claimed(why)) => {
                say(&format!("{team}: another exporter has the batch ({why})\n"));
            }
            Err(e) => warn(&format!(
                "teamwire metrics: team {team}: export failed, will retry: {e}"
            )),
        }
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Money keeps 4 places even on a whole dollar figure; every other
    /// metric keeps the shared format.
    #[test]
    fn money_prints_4_places_and_other_values_do_not() {
        let value = |unit: &str, value: f64| MetricValue {
            name: "m".to_owned(),
            unit: unit.to_owned(),
            labels: Vec::new(),
            value,
        };
        assert_eq!(line(&value("usd", 2.0)), "m 2.0000 usd\n");
        assert_eq!(line(&value("usd", 0.000_42)), "m 0.0004 usd\n");
        assert_eq!(line(&value("{token}", 2.0)), "m 2 {token}\n");
        assert_eq!(line(&value("s", 0.123_45)), "m 0.123 s\n");
    }

    #[test]
    fn durations_take_a_unit() {
        assert_eq!(parse_duration("30s"), Ok(Duration::from_secs(30)));
        assert_eq!(parse_duration("15m"), Ok(Duration::from_secs(900)));
        assert_eq!(parse_duration("2h"), Ok(Duration::from_secs(7200)));
        assert_eq!(parse_duration("1d"), Ok(Duration::from_secs(86_400)));
        for bad in ["", "30", "m", "5x", "-5m", "1.5h"] {
            assert!(parse_duration(bad).is_err(), "{bad}");
        }
    }
}
