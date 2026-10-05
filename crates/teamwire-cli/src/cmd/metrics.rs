//! `teamwire metrics`: local metrics, and `metrics export` to OpenTelemetry.

use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use teamwire_core::{Clock, Config, TeamName};
use teamwire_otel::{ExportError, MetricValue, MetricsQuery, MetricsView};
use teamwire_store::{Ctx, Store, database_path};

use crate::cmd::send::home_or_refuse;
use crate::wiring::Wiring;
use crate::{Global, MetricsArgs, MetricsCommand, USAGE};

/// Refused by a rule; nothing changed (docs/api/cli.md).
const REFUSED: u8 = 1;
/// The store could not be read or written.
const STORE: u8 = 6;

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
        Some(MetricsCommand::Export { once: _ }) => export(&home, wiring),
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
        Err(e) => return fail(&e.to_string(), USAGE),
    };
    let since = match since.map(parse_duration).transpose() {
        Ok(since) => since,
        Err(message) => return fail(&message, USAGE),
    };
    if let Some(team) = &team
        && !database_path(home, team).is_file()
    {
        return fail(&format!("no database for team {team}"), REFUSED);
    }
    let query = MetricsQuery { team, since };
    let view = teamwire_otel::metrics_at(home, wiring.clock.now(), &query);
    for error in &view.errors {
        warn(error);
    }
    if json {
        match serde_json::to_string_pretty(&view) {
            Ok(text) => say(&format!("{text}\n")),
            Err(e) => return fail(&e.to_string(), STORE),
        }
    } else {
        say(&render(&view));
    }
    ExitCode::SUCCESS
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
    let value = if m.value.fract() == 0.0 {
        format!("{:.0}", m.value)
    } else {
        format!("{:.3}", m.value)
    };
    format!("{}{labels} {value} {}\n", m.name, m.unit)
}

/// Sends what every team's export mark has not yet covered. A missing or
/// slow collector never changes the exit code (OBS-2): the failure is
/// printed, the mark stays, and the next run sends the batch.
fn export(home: &Path, wiring: &Wiring) -> ExitCode {
    let Some(config) = teamwire_otel::load_otlp_config(home, &|key| std::env::var(key).ok()) else {
        warn("export is off: set OTEL_EXPORTER_OTLP_ENDPOINT to turn it on");
        return ExitCode::SUCCESS;
    };
    let teams = match team_dirs(home) {
        Ok(teams) => teams,
        Err(e) => return fail(&e, STORE),
    };
    let app_config = Config::default();
    for team in teams {
        let mut store = match Store::open_existing(home, &team) {
            Ok(store) => store,
            Err(e) => {
                warn(&format!("team {team}: {e}"));
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
        match teamwire_otel::export_once(&mut ctx, &config) {
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
            Err(e) => warn(&format!("team {team}: export failed, will retry: {e}")),
        }
    }
    ExitCode::SUCCESS
}

/// The teams that have a database under `home`.
fn team_dirs(home: &Path) -> Result<Vec<TeamName>, String> {
    let root = home.join("teams");
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Ok(Vec::new());
    };
    let mut teams = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", root.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Ok(team) = TeamName::parse(&name)
            && database_path(home, &team).is_file()
        {
            teams.push(team);
        }
    }
    teams.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    Ok(teams)
}

#[expect(clippy::print_stdout, reason = "the metrics are the command's output")]
fn say(text: &str) {
    print!("{text}");
}

#[expect(clippy::print_stderr, reason = "the binary reports problems on stderr")]
fn warn(message: &str) {
    eprintln!("teamwire metrics: {message}");
}

fn fail(message: &str, code: u8) -> ExitCode {
    warn(message);
    ExitCode::from(code)
}

#[cfg(test)]
mod tests {
    use super::*;

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
