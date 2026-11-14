//! `platform_contract`: what every `Platform` must do.

use std::path::Path;
use std::time::Duration;

use teamwire_core::{
    Command, HoldInfo, Pid, Platform, PlatformErrorCode, ReleaseOutcome, RunOutput, SleepHold,
};

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
    /// The first line of stdout is the pid of a process the child started,
    /// and that process must be gone once `run` has returned. A run that
    /// timed out before the child printed its pid is tried again with ten
    /// times the timeout, twice at most: a busy machine can take longer than
    /// the case's timeout to start a shell.
    pub ends_grandchild: bool,
}

impl RunExpect {
    /// An output that meets the expectation. A test of a fake queues it as
    /// the fake's next result.
    pub fn conforming_output(&self) -> RunOutput {
        RunOutput {
            exit_code: self.exit_code,
            stdout: if self.ends_grandchild {
                // A pid no fake process has, so it reads as gone.
                b"4194303\n".to_vec()
            } else {
                self.stdout_contains
                    .clone()
                    .unwrap_or_default()
                    .into_bytes()
            },
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
            ends_grandchild: false,
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
                    ends_grandchild: false,
                },
            },
            Self {
                name: "a timeout kills what the child started too",
                // The grandchild outlives the longest attempt (30 s): one that
                // ended on its own would hide a leak, and would let the shell
                // exit 0 before a correct platform's timeout.
                command: sh("sleep 300 & echo $!; wait"),
                timeout: Duration::from_millis(300),
                expect: RunExpect {
                    exit_code: RunOutput::TIMEOUT_EXIT,
                    timed_out: true,
                    stdout_contains: None,
                    ends_grandchild: true,
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

fn first_line_pid(stdout: &[u8]) -> Option<u32> {
    String::from_utf8_lossy(stdout)
        .lines()
        .next()
        .and_then(|l| l.trim().parse::<u32>().ok())
}

/// The first line of `stdout` names a process the child started. A timeout
/// must have ended it with the child, so it is gone within a short wait (the
/// kernel needs a moment to reap a killed process).
fn grandchild_rule(p: &dyn Platform, case: &str, stdout: &[u8]) {
    let rule = format!("run: {case}: the grandchild is gone");
    let Some(pid) = first_line_pid(stdout) else {
        fail(&rule, "the child printed no pid");
    };
    for _ in 0..40 {
        if !p.pid_alive(Pid::new(pid)) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    fail(&rule, pid);
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
        let mut timeout = case.timeout;
        let mut attempts = if case.expect.ends_grandchild { 3 } else { 1 };
        let out = loop {
            let out = ok(case.name, p.run(&case.command, timeout));
            require(
                &format!("run: {}: exit code", case.name),
                out.exit_code == case.expect.exit_code,
            );
            require(
                &format!("run: {}: timed_out", case.name),
                out.timed_out == case.expect.timed_out,
            );
            attempts -= 1;
            if attempts == 0 || first_line_pid(&out.stdout).is_some() {
                break out;
            }
            timeout = timeout.saturating_mul(10);
        };
        if let Some(want) = &case.expect.stdout_contains {
            let got = String::from_utf8_lossy(&out.stdout);
            require(
                &format!("run: {}: stdout", case.name),
                got.contains(want.as_str()),
            );
        }
        if case.expect.ends_grandchild {
            grandchild_rule(p, case.name, &out.stdout);
        }
    }

    write_rules(setup);
    lock_rules(setup);

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
    mode_rules(setup);
}

/// A free lock is taken at once and its file and folder are made, a busy
/// one fails with `TimedOut` once the wait runs out, and a dropped guard
/// frees it. The file is never written. Both guards are opened in this one
/// thread, so the rules prove that two opens of one process refuse each
/// other. The rules never check that a waiting taker gets the lock once the
/// holder lets go. The real platform's waiting is proven in
/// `crates/teamwire-platform/tests/lock.rs`, by
/// `a_second_taker_waits_until_the_first_guard_drops` and
/// `a_wait_too_large_for_the_clock_waits_as_long_as_the_holder_holds`. The
/// fake's waiting is proven by the tests in
/// `crates/teamwire-launch/tests/worktree_lock.rs` and
/// `crates/teamwire-close/tests/worktree_lock.rs` that hold a lock, let it
/// go, and check that the waiting launch or close then goes on.
fn lock_rules(setup: &PlatformSetup<'_>) {
    let (p, dir) = (setup.platform, setup.dir);
    let folder = dir.join("contract-locks");
    let path = folder.join("a.lock");
    let first = ok("lock_file", p.lock_file(&path, Duration::ZERO));
    require(
        "lock_file makes the lock file and its folder",
        path.is_file(),
    );
    match p.lock_file(&path, Duration::from_millis(100)) {
        Err(e) if e.code == PlatformErrorCode::TimedOut => {}
        other => fail(
            "a busy lock fails with TimedOut once the wait runs out",
            other,
        ),
    }
    drop(first);
    let again = ok(
        "lock_file once the guard has dropped",
        p.lock_file(&path, Duration::ZERO),
    );
    drop(again);
    require(
        "lock_file never writes the lock file",
        std::fs::read(&path).is_ok_and(|bytes| bytes.is_empty()),
    );
    ok("remove_file", std::fs::remove_file(&path));
    ok("remove_dir", std::fs::remove_dir(&folder));
}

/// A replaced file keeps its mode, and a new file is 0644.
fn mode_rules(setup: &PlatformSetup<'_>) {
    use std::os::unix::fs::PermissionsExt;
    let (p, dir) = (setup.platform, setup.dir);
    let mode = |path: &Path| {
        std::fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o7777)
            .ok()
    };
    let fresh = dir.join("contract-mode-new.json");
    ok("write_atomic", p.write_atomic(&fresh, b"new"));
    require(
        "write_atomic makes a new file 0644",
        mode(&fresh) == Some(0o644),
    );
    let private = dir.join("contract-mode-kept.json");
    ok("write_atomic", p.write_atomic(&private, b"old"));
    ok(
        "set_permissions",
        std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o600)),
    );
    ok("write_atomic", p.write_atomic(&private, b"replaced"));
    require(
        "write_atomic keeps the mode of the file it replaces",
        mode(&private) == Some(0o600),
    );
    for path in [&fresh, &private] {
        ok("remove_file", std::fs::remove_file(path));
    }
}
