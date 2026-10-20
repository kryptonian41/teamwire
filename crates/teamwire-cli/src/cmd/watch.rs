//! `teamwire watch`: one line per poll when something needs the lead.

use std::io::Write as _;
use std::path::Path;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use teamwire_core::{Config, OneLine, Pid, TeamName};
use teamwire_deliver::{DeliveryOutcome, WATCH_LEASE_STALE, deliver_line, deliver_line_within};
use teamwire_otel::{OtlpConfig, StopCutoff};
use teamwire_store::{Ctx, Party, Store, StoreError};
use teamwire_watch::{WatchEnd, WatchError, WatchMemory};

use crate::cmd::send::{home_or_refuse, no_database_text, refuse_member};
use crate::recorded::Recorded;
use crate::wiring::Wiring;
use crate::{Global, USAGE, WatchArgs};

/// Exit codes of `docs/api/cli.md`.
const REFUSED: u8 = 1;
const STORE_ERROR: u8 = 6;

/// The longest `--interval`. Each poll refreshes the lease, and a lease
/// unrefreshed for `WATCH_LEASE_STALE` looks dead to a second watch. Half
/// of that leaves room for a slow poll.
const MAX_INTERVAL_S: u64 = WATCH_LEASE_STALE.as_secs() / 2;

/// How often the exporter runs while the watch does (ARCHITECTURE "Metrics").
const EXPORT_EVERY: Duration = Duration::from_secs(60);
/// How long the exporter thread sleeps between looks at the stop flag.
const EXPORT_SLICE: Duration = Duration::from_millis(200);

/// How long an export request still waiting when the watch stops may go on.
/// The watch waits for the exporter thread before it exits, so a collector
/// that never answers would otherwise hold the exit for the whole configured
/// timeout. While the watch runs, each request has that whole timeout.
const WATCH_EXPORT_TIMEOUT: Duration = Duration::from_secs(3);

/// How long one `--deliver` delivery may take. The poll waits for it, so a
/// stuck host must not hold the next poll for longer than this.
const DELIVER_BUDGET: Duration = Duration::from_secs(15);

/// The exporter's target: `OTEL_EXPORTER_OTLP_*` from the environment, else
/// the same keys in `$TEAMWIRE_HOME/config.toml` (ADR 0005 point 4a), read
/// the same way `teamwire metrics export` reads them.
fn otlp_settings(home: &Path, env: &dyn Fn(&str) -> Option<String>) -> Option<OtlpConfig> {
    teamwire_otel::load_otlp_config(home, env)
}

/// The cutoff of each watch export: the short grace applies only once the
/// watch is stopping.
fn cutoff(stop: &AtomicBool) -> StopCutoff<'_> {
    StopCutoff {
        stopping: stop,
        grace: WATCH_EXPORT_TIMEOUT,
    }
}

/// Sends the export mark's batch every minute until `stop` is set. It has
/// its own store connection, and what goes wrong is logged and never
/// changes what the watch prints or how it exits (OBS-2).
fn export_loop(
    home: &Path,
    team: &TeamName,
    wiring: &Wiring,
    config: &Config,
    target: &OtlpConfig,
    stop: &AtomicBool,
) {
    let mut waited = Duration::ZERO;
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(EXPORT_SLICE);
        waited += EXPORT_SLICE;
        if waited < EXPORT_EVERY {
            continue;
        }
        waited = Duration::ZERO;
        let mut store = match Store::open_existing(home, team) {
            Ok(store) => Recorded::new(store, &wiring.clock, "metrics"),
            Err(e) => {
                tracing::warn!(error = %e, "the exporter could not open the store");
                continue;
            }
        };
        let mut ctx = Ctx {
            home,
            store: &mut store,
            registry: &wiring.registry,
            platform: wiring.platform.as_ref(),
            clock: &wiring.clock,
            config,
        };
        match teamwire_otel::export_once_until(&mut ctx, target, Some(cutoff(stop))) {
            Ok(report) => tracing::debug!(?report, "exported"),
            Err(teamwire_otel::ExportError::Claimed(by)) => {
                tracing::debug!(%by, "another exporter holds the batch");
            }
            Err(e) => tracing::warn!(error = %e, "the export failed"),
        }
    }
}

#[expect(
    clippy::print_stderr,
    reason = "the binary reports a refusal on stderr"
)]
fn fail(message: &str, code: u8) -> ExitCode {
    eprintln!("teamwire watch: {message}");
    ExitCode::from(code)
}

#[expect(clippy::print_stdout, reason = "the watch's lines are its output")]
fn say(line: &str) {
    println!("{line}");
    // A line must reach the reader now: it is read by another program.
    if let Err(e) = std::io::stdout().flush() {
        tracing::warn!(error = %e, "could not flush standard output");
    }
}

fn host_name() -> String {
    std::env::var("HOSTNAME").unwrap_or_else(|_| "localhost".to_owned())
}

pub(crate) fn run(args: &WatchArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let WatchArgs {
        team,
        interval,
        once,
        deliver,
    } = args;
    let team = match TeamName::parse(team) {
        Ok(team) => team,
        Err(e) => return fail(&e.to_string(), USAGE),
    };
    // A member's single poll takes the lease and uses up announcement keys
    // that the lead's watch would then never print, so `--once` is refused too.
    if let Some(code) = refuse_member("watch") {
        return code;
    }
    if *interval == 0 {
        return fail("--interval must be at least 1 second", USAGE);
    }
    if *interval > MAX_INTERVAL_S {
        return fail(
            &format!("--interval must be at most {MAX_INTERVAL_S} seconds"),
            USAGE,
        );
    }
    let home = match home_or_refuse("watch", global) {
        Ok(home) => home,
        Err(code) => return code,
    };
    let mut store = match Store::open_existing(&home, &team) {
        Ok(store) => Recorded::new(store, &wiring.clock, "watch"),
        Err(StoreError::NoDatabase { path }) => {
            let message = no_database_text(&team, &path);
            return fail(&message, REFUSED);
        }
        Err(e) => return fail(&e.to_string(), STORE_ERROR),
    };
    let config = Config {
        poll_s: *interval,
        ..Config::default()
    };
    let mut ctx = Ctx {
        home: &home,
        store: &mut store,
        registry: &wiring.registry,
        platform: wiring.platform.as_ref(),
        clock: &wiring.clock,
        config: &config,
    };
    let mut memory = WatchMemory::for_team(team.as_str());
    match teamwire_watch::start(
        &mut ctx,
        &mut memory,
        Pid::new(std::process::id()),
        &host_name(),
    ) {
        Ok(()) => {}
        Err(e @ WatchError::Held { .. }) => return fail(&e.to_string(), REFUSED),
        Err(WatchError::Store(e)) => return fail(&e.to_string(), STORE_ERROR),
    }

    let stop = AtomicBool::new(false);
    let exporter = if *once {
        None
    } else {
        otlp_settings(&home, &|key| std::env::var(key).ok())
    };
    let code = std::thread::scope(|scope| {
        if let Some(target) = &exporter {
            scope.spawn(|| export_loop(&home, &team, wiring, &config, target, &stop));
        }
        let code = poll_loop(&mut ctx, &mut memory, *once, *deliver);
        stop.store(true, Ordering::Relaxed);
        code
    });
    if let Err(e) = teamwire_watch::stop(&mut ctx, &mut memory) {
        tracing::warn!(error = %e, "could not release the watch lease");
    }
    code
}

/// Types a printed line into the lead's place, for a lead that cannot read the
/// watch's output (ADR 0023). The line is already printed, so a line that
/// cannot be delivered is only logged and the next poll goes on.
fn deliver_to_lead(ctx: &mut Ctx<'_>, line: &OneLine) {
    let outcome = match ctx.clock.monotonic().checked_add(DELIVER_BUDGET) {
        Some(deadline) => deliver_line_within(ctx, &Party::Lead, line, deadline),
        None => deliver_line(ctx, &Party::Lead, line),
    };
    match outcome {
        DeliveryOutcome::HandedOff => {}
        DeliveryOutcome::Unconfirmed(e) | DeliveryOutcome::Undelivered(e) => {
            tracing::warn!(error = %e, "a watch line was not delivered to the lead");
        }
    }
}

fn poll_loop(ctx: &mut Ctx<'_>, memory: &mut WatchMemory, once: bool, deliver: bool) -> ExitCode {
    loop {
        if let Some(line) = teamwire_watch::poll(ctx, memory) {
            say(line.as_str());
            if deliver {
                deliver_to_lead(ctx, &line);
            }
        }
        match memory.end {
            Some(WatchEnd::TeamComplete | WatchEnd::TeamClosed) => return ExitCode::SUCCESS,
            Some(WatchEnd::StoreError) => return ExitCode::from(STORE_ERROR),
            Some(WatchEnd::LeaseLost) => return ExitCode::from(REFUSED),
            None if once => return ExitCode::SUCCESS,
            None => std::thread::sleep(Duration::from_secs(ctx.config.poll_s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_short_grace_applies_only_once_the_watch_is_stopping() {
        let stop = AtomicBool::new(false);
        let c = cutoff(&stop);
        assert_eq!(c.grace, WATCH_EXPORT_TIMEOUT);
        assert!(!c.stopping.load(Ordering::Relaxed));
        stop.store(true, Ordering::Relaxed);
        assert!(cutoff(&stop).stopping.load(Ordering::Relaxed));
    }

    /// A collector slower than the stop grace still gets each export while
    /// the watch runs: the configured timeout is not cut.
    #[test]
    fn a_watch_export_keeps_the_configured_timeout() {
        let home = tempfile::tempdir().unwrap();
        let env = |key: &str| match key {
            "OTEL_EXPORTER_OTLP_ENDPOINT" => Some("http://c:4318".to_owned()),
            "OTEL_EXPORTER_OTLP_TIMEOUT" => Some("600000".to_owned()),
            _ => None,
        };
        let target = otlp_settings(home.path(), &env);
        assert_eq!(target.map(|t| t.timeout), Some(Duration::from_secs(600)),);
    }
}
