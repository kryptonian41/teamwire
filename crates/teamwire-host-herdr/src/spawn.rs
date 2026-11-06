//! `Host::spawn`: one workspace per team, one tab per member, then
//! `agent start` in the tab's first pane.

use std::time::{Duration, Instant};

use teamwire_core::{HostError, HostErrorCode, Place, SpawnRequest};

use crate::Herdr;
use crate::shapes::{NewTab, new_tab};
use teamwire_core::Host;

/// The slack `agent start` gets on top of herdr's own `--timeout`, so herdr
/// answers before the process is killed.
const START_SLACK: Duration = Duration::from_secs(10);
/// herdr accepts an `agent start` timeout above this and up to `MAX_START_MS`.
const MIN_START_MS: u64 = 3_001;
const MAX_START_MS: u64 = 300_000;
/// The longest any other call in a spawn may take.
const LAYOUT_TIMEOUT: Duration = Duration::from_secs(30);
/// The one call a spawn may make after its deadline: closing the pane of an
/// agent that failed to start. Leaving it open would leave an agent behind.
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);

/// The kinds herdr detects that this adapter passes through; any other
/// program name starts `claude`.
const KINDS: [&str; 5] = ["claude", "codex", "gemini", "opencode", "pi"];

/// The team part of `tw_<team>_<role>`. A team name has no underscore.
fn team_of(name: &str) -> &str {
    name.strip_prefix("tw_")
        .and_then(|rest| rest.split('_').next())
        .unwrap_or(name)
}

/// `argv[0]` is the program; herdr picks the program by `--kind`, so only
/// the arguments after it are passed on.
fn kind_and_args(argv: &[String]) -> (&'static str, &[String]) {
    match argv.split_first() {
        Some((first, rest)) if !first.starts_with('-') => {
            let base = first.rsplit('/').next().unwrap_or(first);
            let kind = KINDS.iter().find(|k| **k == base).copied();
            (kind.unwrap_or("claude"), rest)
        }
        _ => ("claude", argv),
    }
}

fn place_for(host: &Herdr, tab: &NewTab) -> Place {
    Place {
        host: host.id(),
        id: tab.pane.clone(),
    }
}

/// What is left of the caller's timeout at `deadline`.
fn left(host: &Herdr, deadline: Instant) -> Duration {
    deadline.saturating_duration_since(host.clock.monotonic())
}

/// The timeout of one call: `ask`, or what is left of the caller's timeout
/// when that is less. `Timeout` when nothing is left.
fn budget(host: &Herdr, deadline: Instant, ask: Duration) -> Result<Duration, HostError> {
    let left = left(host, deadline).min(ask);
    if left.is_zero() {
        return Err(out_of_time());
    }
    Ok(left)
}

fn out_of_time() -> HostError {
    HostError::new(
        HostErrorCode::Timeout,
        "spawn ran past the caller's timeout",
    )
}

/// The workspace a live member of the team is in. A label match would pick a
/// user's workspace or a leftover one, so the live agents named
/// `tw_<team>_*` decide.
fn team_workspace(
    host: &Herdr,
    team: &str,
    deadline: Instant,
) -> Result<Option<String>, HostError> {
    let prefix = format!("tw_{team}_");
    Ok(host
        .agents(budget(host, deadline, LAYOUT_TIMEOUT)?)?
        .iter()
        .find(|a| a.name.as_deref().is_some_and(|n| n.starts_with(&prefix)))
        .and_then(|a| a.workspace().map(str::to_owned)))
}

/// The tab for a new member: a new workspace labelled with the team when no
/// member of the team is live, a new tab labelled with the member's title
/// after that. Labels are set here and never changed.
fn make_tab(host: &Herdr, req: &SpawnRequest, deadline: Instant) -> Result<NewTab, HostError> {
    let team = team_of(req.name.as_str());
    let cwd = req.cwd.display().to_string();
    if let Some(ws) = team_workspace(host, team, deadline)? {
        let result = host.call(
            &[
                "tab",
                "create",
                "--workspace",
                &ws,
                "--cwd",
                &cwd,
                "--label",
                &req.title,
                "--no-focus",
            ],
            budget(host, deadline, LAYOUT_TIMEOUT)?,
        )?;
        return new_tab(result);
    }
    let result = host.call(
        &[
            "workspace",
            "create",
            "--cwd",
            &cwd,
            "--label",
            team,
            "--no-focus",
        ],
        budget(host, deadline, LAYOUT_TIMEOUT)?,
    )?;
    let tab = new_tab(result)?;
    host.call(
        &["tab", "rename", &tab.tab, &req.title],
        budget(host, deadline, LAYOUT_TIMEOUT)?,
    )?;
    Ok(tab)
}

/// Runs `agent start`, trying again while the pane is busy (not yet at its
/// shell prompt). `agent_not_ready` means the agent started and waits at a
/// dialog: it stays in the pane, so that is a started member, and the caller
/// answers the dialog. Retrying would only meet `agent_pane_busy`. Each try
/// gets `run_for` or what is left before `deadline`, and no try starts after
/// it.
fn start(
    host: &Herdr,
    args: &[String],
    run_for: Duration,
    deadline: Instant,
) -> Result<(), HostError> {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut last: Option<HostError> = None;
    for attempt in 0..host.start_tries {
        let mut room = left(host, deadline);
        if attempt > 0 {
            let nap = host.start_retry_delay.min(room);
            std::thread::sleep(nap);
            room = room.saturating_sub(nap);
        }
        if room.is_zero() {
            return Err(out_of_time());
        }
        match host.call(&refs, run_for.min(room)) {
            Ok(_) => return Ok(()),
            Err(e) if e.code == HostErrorCode::NotReady => return Ok(()),
            Err(e) if e.code == HostErrorCode::Busy => last = Some(e),
            Err(e) => return Err(e),
        }
    }
    Err(last.unwrap_or_else(|| HostError::new(HostErrorCode::Other, "agent start made no attempt")))
}

pub(crate) fn spawn(
    host: &Herdr,
    req: &SpawnRequest,
    timeout: Duration,
) -> Result<Place, HostError> {
    let deadline = host.clock.monotonic() + timeout;
    let tab = make_tab(host, req, deadline)?;
    let (kind, rest) = kind_and_args(&req.argv);
    let wait = u64::try_from(timeout.as_millis())
        .unwrap_or(MAX_START_MS)
        .clamp(MIN_START_MS, MAX_START_MS);
    let wait_ms = wait.to_string();
    let mut args: Vec<String> = [
        "agent",
        "start",
        req.name.as_str(),
        "--kind",
        kind,
        "--pane",
        &tab.pane,
        "--timeout",
        &wait_ms,
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    if !rest.is_empty() {
        args.push("--".to_owned());
        args.extend(rest.iter().cloned());
    }
    if let Err(e) = start(
        host,
        &args,
        Duration::from_millis(wait) + START_SLACK,
        deadline,
    ) {
        // Launch records a place only on success, so nothing else can close the
        // pane made above. Herdr closes the tab with its last pane, and the
        // workspace with its last tab. On a timeout this also stops an agent
        // that started late.
        drop(host.call(&["pane", "close", &tab.pane], CLEANUP_TIMEOUT));
        return Err(e);
    }
    Ok(place_for(host, &tab))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use teamwire_core::{Clock, RunOutput, Timestamp};
    use teamwire_fakes::{FakeClock, FakePlatform};

    use super::*;

    /// A clock that moves one step every time `monotonic` reads it.
    struct Ticking {
        clock: FakeClock,
        step: Duration,
    }

    impl Clock for Ticking {
        fn now(&self) -> Timestamp {
            self.clock.now()
        }

        fn monotonic(&self) -> Instant {
            self.clock.advance(self.step);
            self.clock.monotonic()
        }
    }

    fn json(stdout: &str) -> RunOutput {
        RunOutput {
            exit_code: 0,
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
            timed_out: false,
        }
    }

    fn busy() -> RunOutput {
        RunOutput {
            exit_code: 1,
            stdout: Vec::new(),
            stderr: br#"{"error":{"code":"agent_pane_busy","message":"busy"}}"#.to_vec(),
            timed_out: false,
        }
    }

    #[test]
    fn a_known_program_is_its_own_kind_and_any_other_starts_claude() {
        let argv = |words: &[&str]| words.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>();
        let pi = argv(&["/opt/pi/bin/pi", "--session-id", "s"]);
        let (kind, rest) = kind_and_args(&pi);
        assert_eq!(kind, "pi");
        assert_eq!(Some(rest), pi.get(1..));
        assert_eq!(kind_and_args(&argv(&["codex", "-C", "/w"])).0, "codex");
        assert_eq!(kind_and_args(&argv(&["pix"])).0, "claude");
        assert_eq!(kind_and_args(&argv(&["--flag"])).0, "claude");
    }

    #[test]
    fn a_spawn_whose_clock_runs_out_stops_retrying() {
        let platform = Arc::new(FakePlatform::new());
        platform.queue_run(Ok(json(r#"{"result":{"agents":[]}}"#)));
        platform.queue_run(Ok(json(
            r#"{"result":{"tab":{"tab_id":"w1:t1"},"root_pane":{"pane_id":"w1:p1"}}}"#,
        )));
        platform.queue_run(Ok(json(r#"{"result":{}}"#)));
        for _ in 0..8 {
            platform.queue_run(Ok(busy()));
        }
        // Every read of the clock costs a second, and the caller gave five.
        let clock = Ticking {
            clock: FakeClock::new(),
            step: Duration::from_secs(1),
        };
        let herdr = Herdr::new(platform.clone())
            .with_start_retry(8, Duration::ZERO)
            .with_clock(Arc::new(clock));
        let req = SpawnRequest {
            name: teamwire_core::Address::parse("tw_demo_api").unwrap(),
            title: "api".to_owned(),
            team_title: "Demo team".to_owned(),
            generation: 1,
            cwd: std::env::temp_dir(),
            argv: vec!["claude".to_owned()],
            env: Vec::new(),
        };
        let err = spawn(&herdr, &req, Duration::from_secs(5)).unwrap_err();
        assert_eq!(err.code, HostErrorCode::Timeout);
        let runs = platform.runs();
        let starts = runs
            .iter()
            .filter(|(c, _)| c.args.iter().any(|a| a == "start"))
            .count();
        // The clock ran out during the first try, so no second try starts.
        assert_eq!(starts, 1, "{runs:?}");
    }
}
