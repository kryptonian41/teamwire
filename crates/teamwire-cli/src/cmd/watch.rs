//! `teamwire watch`: one line per poll when something needs the lead.

use std::path::Path;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use teamwire_core::{Config, OneLine, Pid, TeamName};
use teamwire_deliver::{DeliveryOutcome, WATCH_LEASE_STALE, deliver_line, deliver_line_within};
use teamwire_otel::{OtlpConfig, StopCutoff};
use teamwire_store::{Ctx, Party, Store};
use teamwire_watch::{WatchEnd, WatchError, WatchMemory};

use crate::cmd::shared::{Opening, REFUSED, STORE, fail, refuse_member, say_line, with_ctx_using};
use crate::recorded::Recorded;
use crate::wiring::Wiring;
use crate::{Global, USAGE, WatchArgs};

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
        export_pass(home, team, wiring, config, target, stop);
    }
}

/// One export of the loop, with its own store connection.
fn export_pass(
    home: &Path,
    team: &TeamName,
    wiring: &Wiring,
    config: &Config,
    target: &OtlpConfig,
    stop: &AtomicBool,
) {
    let mut store = match Store::open_existing(home, team) {
        Ok(store) => Recorded::new(store, &wiring.clock, "metrics"),
        Err(e) => {
            tracing::warn!(error = %e, "the exporter could not open the store");
            return;
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
        Err(e) => return fail("watch", &e.to_string(), USAGE),
    };
    // A member's single poll takes the lease and uses up announcement keys
    // that the lead's watch would then never print, so `--once` is refused too.
    if let Some(code) = refuse_member("watch") {
        return code;
    }
    if *interval == 0 {
        return fail("watch", "--interval must be at least 1 second", USAGE);
    }
    if *interval > MAX_INTERVAL_S {
        return fail(
            "watch",
            &format!("--interval must be at most {MAX_INTERVAL_S} seconds"),
            USAGE,
        );
    }
    let opening = Opening {
        command: "watch",
        config: Config {
            poll_s: *interval,
            ..Config::default()
        },
        ..Opening::default()
    };
    with_ctx_using(global, wiring, &team, "watch", opening, |ctx| {
        watch(ctx, &team, wiring, *once, *deliver)
    })
    .unwrap_or_else(|code| code)
}

/// Takes the lease, polls until the team ends, and lets the lease go.
fn watch(
    ctx: &mut Ctx<'_>,
    team: &TeamName,
    wiring: &Wiring,
    once: bool,
    deliver: bool,
) -> ExitCode {
    let (home, config) = (ctx.home, ctx.config);
    let mut memory = WatchMemory::for_team(team.as_str());
    match teamwire_watch::start(ctx, &mut memory, Pid::new(std::process::id()), &host_name()) {
        Ok(()) => {}
        Err(e @ WatchError::Held { .. }) => return fail("watch", &e.to_string(), REFUSED),
        Err(WatchError::Store(e)) => return fail("watch", &e.to_string(), STORE),
    }

    let stop = AtomicBool::new(false);
    let exporter = if once {
        None
    } else {
        otlp_settings(home, &|key| std::env::var(key).ok())
    };
    let code = std::thread::scope(|scope| {
        if let Some(target) = &exporter {
            scope.spawn(|| export_loop(home, team, wiring, config, target, &stop));
        }
        let code = poll_loop(ctx, &mut memory, once, deliver);
        stop.store(true, Ordering::Relaxed);
        code
    });
    if let Err(e) = teamwire_watch::stop(ctx, &mut memory) {
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
            say_line(line.as_str());
            if deliver {
                deliver_to_lead(ctx, &line);
            }
        }
        match memory.end {
            Some(WatchEnd::TeamComplete | WatchEnd::TeamClosed) => return ExitCode::SUCCESS,
            Some(WatchEnd::StoreError) => return ExitCode::from(STORE),
            Some(WatchEnd::LeaseLost) => return ExitCode::from(REFUSED),
            None if once => return ExitCode::SUCCESS,
            None => std::thread::sleep(Duration::from_secs(ctx.config.poll_s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use teamwire_store::StoreError;

    use super::*;

    #[test]
    fn an_export_that_fails_in_the_store_leaves_one_store_error_row() {
        let home = teamwire_fakes::TestHome::new().unwrap();
        let team = TeamName::parse("alpha").unwrap();
        let mut store = Store::open(home.path(), &team).unwrap();
        // A function that does not exist fails the claim's update with an
        // `io` error, before anything is sent. The claim is only made when
        // an event waits to be sent.
        store
            .write(|tx| {
                let long_ago = teamwire_core::Timestamp::from_unix_millis(1_000_000_000_000)
                    .expect("a valid time");
                tx.record_event(
                    long_ago,
                    teamwire_core::EventType::WatchLine,
                    None,
                    None,
                    &serde_json::json!({ "events": [] }),
                )?;
                tx.connection()
                    .execute_batch(
                        "CREATE TRIGGER no_claim BEFORE UPDATE ON metric_export
                         BEGIN SELECT no_such_function(); END",
                    )
                    .map_err(StoreError::from)
            })
            .unwrap();
        let wiring = crate::wiring::wire(&crate::wiring::WiringEnv {
            herdr_env: None,
            tmux: None,
            user_home: None,
            claude_dir: None,
            claude_config_dir: None,
            codex_test_home: None,
            codex_home: None,
            pi_agent_dir: None,
            lead_harness: None,
            claude_session: None,
            codex_thread: None,
            pi_marker: None,
        })
        .unwrap();
        let target = OtlpConfig {
            endpoint: "http://127.0.0.1:1".to_owned(),
            headers: Vec::new(),
            timeout: Duration::from_secs(1),
        };
        export_pass(
            home.path(),
            &team,
            &wiring,
            &Config::default(),
            &target,
            &AtomicBool::new(false),
        );
        let errors: Vec<serde_json::Value> = store
            .read(|tx| tx.events_after(0, None, None)?.all_or_corrupt())
            .unwrap()
            .into_iter()
            .filter(|e| e.kind == teamwire_core::EventType::StoreError)
            .map(|e| e.data)
            .collect();
        assert_eq!(
            errors,
            [serde_json::json!({ "op": "metrics", "code": "io" })]
        );
    }

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
