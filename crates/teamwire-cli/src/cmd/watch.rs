//! `teamwire watch`: one line per poll when something needs the lead.

use std::io::Write as _;
use std::path::Path;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use teamwire_core::{Config, Pid, TeamName};
use teamwire_otel::OtlpConfig;
use teamwire_store::{Ctx, Store, StoreError};
use teamwire_watch::{WatchEnd, WatchError, WatchMemory};

use crate::cmd::send::{home_or_refuse, no_database_text, refuse_member};
use crate::wiring::Wiring;
use crate::{Global, USAGE, WatchArgs};

/// Exit codes of `docs/api/cli.md`.
const REFUSED: u8 = 1;
const STORE_ERROR: u8 = 6;

/// How often the exporter runs while the watch does (ARCHITECTURE "Metrics").
const EXPORT_EVERY: Duration = Duration::from_secs(60);
/// How long the exporter thread sleeps between looks at the stop flag.
const EXPORT_SLICE: Duration = Duration::from_millis(200);

/// The exporter's target: `OTEL_EXPORTER_OTLP_*` from the environment, else
/// the same keys in `$TEAMWIRE_HOME/config.toml` (ADR 0005 point 4a), read
/// the same way `teamwire metrics export` reads them.
fn otlp_settings(home: &Path) -> Option<OtlpConfig> {
    teamwire_otel::load_otlp_config(home, &|key| std::env::var(key).ok())
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
            Ok(store) => store,
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
        match teamwire_otel::export_once(&mut ctx, target) {
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
    } = args;
    let team = match TeamName::parse(team) {
        Ok(team) => team,
        Err(e) => return fail(&e.to_string(), USAGE),
    };
    if !*once {
        if let Some(code) = refuse_member("watch") {
            return code;
        }
    }
    if *interval == 0 {
        return fail("--interval must be at least 1 second", USAGE);
    }
    let home = match home_or_refuse("watch", global) {
        Ok(home) => home,
        Err(code) => return code,
    };
    let mut store = match Store::open_existing(&home, &team) {
        Ok(store) => store,
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
    let mut memory = WatchMemory::default();
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
    let exporter = if *once { None } else { otlp_settings(&home) };
    let code = std::thread::scope(|scope| {
        if let Some(target) = &exporter {
            scope.spawn(|| export_loop(&home, &team, wiring, &config, target, &stop));
        }
        let code = poll_loop(&mut ctx, &mut memory, *once);
        stop.store(true, Ordering::Relaxed);
        code
    });
    if let Err(e) = teamwire_watch::stop(&mut ctx, &mut memory) {
        tracing::warn!(error = %e, "could not release the watch lease");
    }
    code
}

fn poll_loop(ctx: &mut Ctx<'_>, memory: &mut WatchMemory, once: bool) -> ExitCode {
    loop {
        if let Some(line) = teamwire_watch::poll(ctx, memory) {
            say(line.as_str());
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
