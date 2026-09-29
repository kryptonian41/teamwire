//! `platform_contract`: what every `Platform` must do.

use std::path::Path;
use std::time::Duration;

use teamwire_core::{Command, HoldInfo, Pid, Platform, ReleaseOutcome, RunOutput, SleepHold};

use super::{fail, ok, require};

/// A process started with one environment variable set, so the suite can read
/// it back through `env_of`. The caller starts the process, for example
/// `sh -c 'sleep 30'` with the variable in its environment, and keeps it alive
/// until the suite returns.
#[derive(Debug, Clone)]
pub struct EnvProbe {
    pub pid: Pid,
    pub var: String,
    pub value: String,
}

/// What one `run` must produce.
#[derive(Debug, Clone)]
pub struct RunExpect {
    pub exit_code: i32,
    pub timed_out: bool,
    pub stdout_contains: Option<String>,
}

impl RunExpect {
    /// An output that meets the expectation. A test of a fake queues it as
    /// the fake's next result.
    pub fn conforming_output(&self) -> RunOutput {
        RunOutput {
            exit_code: self.exit_code,
            stdout: self
                .stdout_contains
                .clone()
                .unwrap_or_default()
                .into_bytes(),
            stderr: Vec::new(),
            timed_out: self.timed_out,
        }
    }
}

/// One process for `run` and what must come back.
#[derive(Debug, Clone)]
pub struct RunCase {
    pub name: &'static str,
    pub command: Command,
    pub timeout: Duration,
    pub expect: RunExpect,
}

impl RunCase {
    /// Cases for any Unix `/bin/sh`: output, exit code, stdin, added and
    /// cleared environment, and a timeout that must give exit 124.
    pub fn unix_standard() -> Vec<Self> {
        let sh = |script: &str| Command::new("/bin/sh").arg("-c").arg(script);
        let secs = Duration::from_secs(10);
        let expect = |exit_code, contains: Option<&str>| RunExpect {
            exit_code,
            timed_out: false,
            stdout_contains: contains.map(str::to_owned),
        };
        vec![
            Self {
                name: "stdout and exit 0",
                command: sh("printf contract-out"),
                timeout: secs,
                expect: expect(0, Some("contract-out")),
            },
            Self {
                name: "a non-zero exit code",
                command: sh("exit 3"),
                timeout: secs,
                expect: expect(3, None),
            },
            Self {
                name: "stdin reaches the child",
                command: sh("cat").stdin(b"from-stdin".to_vec()),
                timeout: secs,
                expect: expect(0, Some("from-stdin")),
            },
            Self {
                name: "env is added",
                command: sh("printf \"[%s]\" \"$CONTRACT_VAR\"").env("CONTRACT_VAR", "added"),
                timeout: secs,
                expect: expect(0, Some("[added]")),
            },
            Self {
                name: "env_clear removes the inherited environment",
                command: sh("printf \"[%s]\" \"${HOME-unset}\"").env_clear(),
                timeout: secs,
                expect: expect(0, Some("[unset]")),
            },
            Self {
                name: "a timeout kills the child and reports 124",
                command: sh("sleep 30"),
                timeout: Duration::from_millis(300),
                expect: RunExpect {
                    exit_code: RunOutput::TIMEOUT_EXIT,
                    timed_out: true,
                    stdout_contains: None,
                },
            },
        ]
    }
}

/// How a caller lets `platform_contract` reach one platform.
pub struct PlatformSetup<'a> {
    pub platform: &'a dyn Platform,
    /// An empty directory the suite may write files in.
    pub dir: &'a Path,
    /// A process that is running now (the test's own pid is fine).
    pub live_pid: Pid,
    /// A pid known to be dead, if the caller has one (a reaped child).
    pub dead_pid: Option<Pid>,
    pub env_probe: Option<EnvProbe>,
    /// Processes to run. Empty skips `run`.
    pub run_cases: Vec<RunCase>,
    /// The pid a sleep hold protects. Used only when the platform has a hold.
    /// It must be a process the caller started and keeps alive until the
    /// suite returns (a child running `sleep`), never a pid the test does not
    /// own: a real hold watches this process and ends with it.
    pub hold_lead: Pid,
}

impl std::fmt::Debug for PlatformSetup<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlatformSetup")
            .field("dir", &self.dir)
            .field("live_pid", &self.live_pid)
            .finish_non_exhaustive()
    }
}

/// Runs every rule on one platform.
pub fn platform_contract(setup: &PlatformSetup<'_>) {
    let p = setup.platform;

    require("a running process is alive", p.pid_alive(setup.live_pid));
    if let Some(dead) = setup.dead_pid {
        require("a reaped process is not alive", !p.pid_alive(dead));
    }

    if let Some(probe) = &setup.env_probe {
        let got = ok("env_of", p.env_of(probe.pid, &probe.var));
        require(
            "env_of reads a variable the process was started with",
            got.as_deref() == Some(probe.value.as_str()),
        );
        let unset = ok("env_of", p.env_of(probe.pid, "TEAMWIRE_CONTRACT_NEVER_SET"));
        require("env_of gives None for an unset variable", unset.is_none());
    }

    for case in &setup.run_cases {
        let out = ok(case.name, p.run(&case.command, case.timeout));
        require(
            &format!("run: {}: exit code", case.name),
            out.exit_code == case.expect.exit_code,
        );
        require(
            &format!("run: {}: timed_out", case.name),
            out.timed_out == case.expect.timed_out,
        );
        if let Some(want) = &case.expect.stdout_contains {
            let got = String::from_utf8_lossy(&out.stdout);
            require(
                &format!("run: {}: stdout", case.name),
                got.contains(want.as_str()),
            );
        }
    }

    write_rules(setup);

    let free = ok("free_bytes", p.free_bytes(setup.dir));
    require("free_bytes of a real directory is more than zero", free > 0);
    ok("on_network_fs", p.on_network_fs(setup.dir));

    if let Some(hold) = p.sleep_hold() {
        hold_rules(hold, setup.hold_lead);
    }
}

/// Releases a hold the suite started, also when a rule fails first, so a
/// failing suite never leaves a `caffeinate` running.
struct HoldGuard<'a> {
    hold: &'a dyn SleepHold,
    info: HoldInfo,
    lead: Pid,
    armed: bool,
}

impl Drop for HoldGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.hold.release(&self.info, self.lead).ok();
        }
    }
}

/// True when `find_existing` reports no hold on `lead`, waiting a moment:
/// `release` may signal the process and return before it is gone.
fn hold_gone(hold: &dyn SleepHold, lead: Pid) -> bool {
    for _ in 0..40 {
        if hold.find_existing(lead).is_none() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// The suite compares pids, not `owned`: a platform reads a hold from the
/// process list, which cannot say who started it, so the service sets
/// ownership from team records (SLP-1).
fn hold_rules(hold: &dyn SleepHold, lead: Pid) {
    require(
        "no hold exists before the suite starts one",
        hold.find_existing(lead).is_none(),
    );
    let info = ok("SleepHold::start", hold.start(lead));
    let mut guard = HoldGuard {
        hold,
        info,
        lead,
        armed: true,
    };
    require("a hold the platform starts is owned", info.owned);
    require(
        "find_existing finds the hold just started",
        hold.find_existing(lead).map(|h| h.pid) == Some(info.pid),
    );
    let released = ok("SleepHold::release", hold.release(&info, lead));
    guard.armed = false;
    match released {
        ReleaseOutcome::Released => {}
        other @ (ReleaseOutcome::NotOwned
        | ReleaseOutcome::AlreadyGone
        | ReleaseOutcome::NotAHold) => fail("releasing an owned hold releases it", other),
    }
    require(
        "find_existing finds nothing after release",
        hold_gone(hold, lead),
    );
    match ok("SleepHold::release", hold.release(&info, lead)) {
        ReleaseOutcome::AlreadyGone | ReleaseOutcome::NotAHold => {}
        other @ (ReleaseOutcome::Released | ReleaseOutcome::NotOwned) => fail(
            "releasing twice finds the hold gone or someone else's",
            other,
        ),
    }
}

fn write_rules(setup: &PlatformSetup<'_>) {
    let (p, dir) = (setup.platform, setup.dir);
    let target = dir.join("contract.json");
    ok("write_atomic", p.write_atomic(&target, b"first"));
    require(
        "write_atomic writes the bytes",
        std::fs::read(&target).ok().as_deref() == Some(&b"first"[..]),
    );
    ok(
        "write_atomic",
        p.write_atomic(&target, b"second, and longer"),
    );
    require(
        "write_atomic replaces the whole file",
        std::fs::read(&target).ok().as_deref() == Some(&b"second, and longer"[..]),
    );
    ok("write_atomic", p.write_atomic(&target, b"3"));
    require(
        "write_atomic replaces with shorter bytes",
        std::fs::read(&target).ok().as_deref() == Some(&b"3"[..]),
    );
    let entries = std::fs::read_dir(dir).map(Iterator::count).unwrap_or(0);
    require("write_atomic leaves no temporary file behind", entries == 1);
}
