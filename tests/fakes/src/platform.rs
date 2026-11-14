//! `FakePlatform`: an in-process `Platform` with environments per pid, free
//! space, a network-filesystem flag and a sleep hold that records its life.
//! It writes real files (in a directory the test owns) so that a test can
//! read them back, takes real `flock` locks so that a test's threads wait
//! for each other, and it never starts a process.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::TryLockError;
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use teamwire_core::{
    Command, FileLock, HoldInfo, Pid, Platform, PlatformError, PlatformErrorCode, ReleaseOutcome,
    RunOutput, SleepHold, Timestamp,
};

use crate::calls::{CallKind, CallLog, lock};

/// Something the fake sleep hold did, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldEvent {
    Started { lead: Pid, hold: Pid },
    Released { hold: Pid, outcome: ReleaseOutcome },
}

#[derive(Debug)]
struct HoldRecord {
    pid: Pid,
    lead: Pid,
    owned: bool,
    /// False once the process has exited.
    running: bool,
    /// False once the pid has been reused by some other program.
    is_hold: bool,
}

#[derive(Debug, Default)]
struct HoldState {
    records: Vec<HoldRecord>,
    events: Vec<HoldEvent>,
    start_fault: Option<PlatformError>,
    /// `find_existing` says `owned: false` for every hold, as `MacOs` does.
    report_unowned: bool,
    /// `find_existing` reports this pid instead of the hold's own.
    misreport_pid: Option<Pid>,
}

const FIRST_HOLD_PID: u32 = 900_000;

/// The fake `SleepHold`. Reached through `FakePlatform::sleep_hold`.
#[derive(Debug)]
pub struct FakeSleepHold {
    state: Mutex<HoldState>,
    next_pid: AtomicU64,
    log: std::sync::Arc<CallLog>,
}

impl SleepHold for FakeSleepHold {
    fn find_existing(&self, lead_pid: Pid) -> Option<HoldInfo> {
        self.log
            .record("hold_find_existing", CallKind::Read, lead_pid.to_string());
        let state = lock(&self.state);
        state
            .records
            .iter()
            .find(|r| r.lead == lead_pid && r.running && r.is_hold)
            .map(|r| HoldInfo {
                pid: state.misreport_pid.unwrap_or(r.pid),
                owned: r.owned && !state.report_unowned,
            })
    }

    fn start(&self, lead_pid: Pid) -> Result<HoldInfo, PlatformError> {
        self.log
            .record("hold_start", CallKind::Write, lead_pid.to_string());
        let mut state = lock(&self.state);
        if let Some(err) = state.start_fault.clone() {
            return Err(err);
        }
        let raw = self.next_pid.fetch_add(1, Ordering::Relaxed);
        let pid = Pid::new(u32::try_from(raw).unwrap_or(u32::MAX));
        state.records.push(HoldRecord {
            pid,
            lead: lead_pid,
            owned: true,
            running: true,
            is_hold: true,
        });
        state.events.push(HoldEvent::Started {
            lead: lead_pid,
            hold: pid,
        });
        Ok(HoldInfo { pid, owned: true })
    }

    fn release(&self, hold: &HoldInfo, lead_pid: Pid) -> Result<ReleaseOutcome, PlatformError> {
        self.log
            .record("hold_release", CallKind::Write, hold.pid.to_string());
        let mut state = lock(&self.state);
        let outcome = match state.records.iter_mut().find(|r| r.pid == hold.pid) {
            None => ReleaseOutcome::AlreadyGone,
            Some(r) if !r.running => ReleaseOutcome::AlreadyGone,
            Some(r) if !r.is_hold || r.lead != lead_pid => ReleaseOutcome::NotAHold,
            Some(r) if !r.owned || !hold.owned => ReleaseOutcome::NotOwned,
            Some(r) => {
                r.running = false;
                ReleaseOutcome::Released
            }
        };
        state.events.push(HoldEvent::Released {
            hold: hold.pid,
            outcome,
        });
        Ok(outcome)
    }
}

#[derive(Debug)]
struct State {
    alive: HashSet<Pid>,
    /// Start times by pid. A pid with none reports `None`, as a platform
    /// that cannot read start times does.
    started: HashMap<Pid, Timestamp>,
    envs: HashMap<(Pid, String), String>,
    /// Each pid's parent and program name, as `parent_of` reports them.
    parents: HashMap<Pid, (Pid, String)>,
    env_fault: Option<PlatformError>,
    run_queue: VecDeque<Result<RunOutput, PlatformError>>,
    runs: Vec<(Command, Duration)>,
    write_fault: Option<PlatformError>,
    written: Vec<PathBuf>,
    free_bytes: u64,
    free_fault: Option<PlatformError>,
    network_fs: bool,
    network_fs_fault: Option<PlatformError>,
}

/// An in-process `Platform`.
///
/// Faults: env read failure (`fail_env_reads`), low disk
/// (`set_free_bytes`), a network filesystem (`set_network_fs`), a full disk
/// on write (`fail_writes`), a scripted `run` result (`queue_run`).
#[derive(Debug)]
pub struct FakePlatform {
    log: std::sync::Arc<CallLog>,
    state: Mutex<State>,
    hold: Option<FakeSleepHold>,
    temp_counter: AtomicU64,
}

impl FakePlatform {
    /// A platform with a sleep hold (macOS-like) and 10 GiB free.
    pub fn new() -> Self {
        let log = std::sync::Arc::new(CallLog::default());
        Self {
            hold: Some(FakeSleepHold {
                state: Mutex::new(HoldState::default()),
                next_pid: AtomicU64::new(u64::from(FIRST_HOLD_PID)),
                log: std::sync::Arc::clone(&log),
            }),
            log,
            state: Mutex::new(State {
                alive: HashSet::new(),
                started: HashMap::new(),
                envs: HashMap::new(),
                parents: HashMap::new(),
                env_fault: None,
                run_queue: VecDeque::new(),
                runs: Vec::new(),
                write_fault: None,
                written: Vec::new(),
                free_bytes: 10 * 1024 * 1024 * 1024,
                free_fault: None,
                network_fs: false,
                network_fs_fault: None,
            }),
            temp_counter: AtomicU64::new(0),
        }
    }

    /// A platform with no sleep hold (Linux-like): `sleep_hold()` is `None`.
    pub fn without_sleep_hold() -> Self {
        Self {
            hold: None,
            ..Self::new()
        }
    }

    pub fn log(&self) -> &CallLog {
        &self.log
    }

    pub fn set_alive(&self, pid: Pid, alive: bool) {
        let mut state = lock(&self.state);
        if alive {
            state.alive.insert(pid);
        } else {
            state.alive.remove(&pid);
        }
    }

    /// When the process with `pid` started; `None` makes the start time
    /// unknown. `process_started` reports it only while the pid is alive.
    pub fn set_started(&self, pid: Pid, started: Option<Timestamp>) {
        let mut state = lock(&self.state);
        match started {
            Some(at) => state.started.insert(pid, at),
            None => state.started.remove(&pid),
        };
    }

    /// The process with `pid` exited and a new process, started at
    /// `started`, got the same pid: alive again, with a new start time.
    pub fn reuse_pid(&self, pid: Pid, started: Timestamp) {
        let mut state = lock(&self.state);
        state.alive.insert(pid);
        state.started.insert(pid, started);
    }

    pub fn set_env(&self, pid: Pid, var: &str, value: &str) {
        lock(&self.state)
            .envs
            .insert((pid, var.to_owned()), value.to_owned());
    }

    /// Makes `parent_of(pid)` report `parent`, a program named `name`.
    pub fn set_parent(&self, pid: Pid, parent: Pid, name: &str) {
        lock(&self.state)
            .parents
            .insert(pid, (parent, name.to_owned()));
    }

    /// Every later `env_of` fails with `err` until cleared with `None`.
    pub fn fail_env_reads(&self, err: Option<PlatformError>) {
        lock(&self.state).env_fault = err;
    }

    /// Every later `write_atomic`, `create_dir_all` and `remove_dir_all`
    /// fails with `err` until cleared.
    pub fn fail_writes(&self, err: Option<PlatformError>) {
        lock(&self.state).write_fault = err;
    }

    pub fn set_free_bytes(&self, bytes: u64) {
        lock(&self.state).free_bytes = bytes;
    }

    pub fn fail_free_bytes(&self, err: Option<PlatformError>) {
        lock(&self.state).free_fault = err;
    }

    pub fn set_network_fs(&self, yes: bool) {
        lock(&self.state).network_fs = yes;
    }

    pub fn fail_network_fs_check(&self, err: Option<PlatformError>) {
        lock(&self.state).network_fs_fault = err;
    }

    /// The next `run` returns `result`. With nothing queued, `run` returns
    /// exit code 0 and empty output.
    pub fn queue_run(&self, result: Result<RunOutput, PlatformError>) {
        lock(&self.state).run_queue.push_back(result);
    }

    /// Every command passed to `run`, with its timeout.
    pub fn runs(&self) -> Vec<(Command, Duration)> {
        lock(&self.state).runs.clone()
    }

    /// Paths `write_atomic` wrote successfully, in order.
    pub fn written(&self) -> Vec<PathBuf> {
        lock(&self.state).written.clone()
    }

    /// A sleep hold the user started on `lead`, which teamwire may reuse
    /// but never kill (SLP-1).
    pub fn add_external_hold(&self, lead: Pid, hold: Pid) {
        if let Some(h) = &self.hold {
            lock(&h.state).records.push(HoldRecord {
                pid: hold,
                lead,
                owned: false,
                running: true,
                is_hold: true,
            });
        }
    }

    /// The hold process `hold` exited on its own.
    pub fn end_hold_process(&self, hold: Pid) {
        if let Some(h) = &self.hold {
            for r in &mut lock(&h.state).records {
                if r.pid == hold {
                    r.running = false;
                }
            }
        }
    }

    /// The operating system reused `hold`'s pid for an unrelated program.
    pub fn reuse_hold_pid(&self, hold: Pid) {
        if let Some(h) = &self.hold {
            for r in &mut lock(&h.state).records {
                if r.pid == hold {
                    r.is_hold = false;
                }
            }
        }
    }

    /// The next `SleepHold::start` fails with `err`.
    pub fn fail_hold_start(&self, err: Option<PlatformError>) {
        if let Some(h) = &self.hold {
            lock(&h.state).start_fault = err;
        }
    }

    /// From now on `find_existing` says `owned: false` for every hold, as the
    /// real macOS platform does (a process list cannot say who started it).
    pub fn report_holds_as_unowned(&self, yes: bool) {
        if let Some(h) = &self.hold {
            lock(&h.state).report_unowned = yes;
        }
    }

    /// From now on `find_existing` reports `pid` for the hold it finds.
    pub fn misreport_hold_pid(&self, pid: Option<Pid>) {
        if let Some(h) = &self.hold {
            lock(&h.state).misreport_pid = pid;
        }
    }

    /// What the sleep hold did, in order.
    pub fn hold_events(&self) -> Vec<HoldEvent> {
        self.hold
            .as_ref()
            .map(|h| lock(&h.state).events.clone())
            .unwrap_or_default()
    }
}

impl Default for FakePlatform {
    fn default() -> Self {
        Self::new()
    }
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "std::io::ErrorKind is non-exhaustive; every kind without its own code maps to Io"
)]
fn io_to_platform(e: &std::io::Error) -> PlatformError {
    let code = match e.kind() {
        std::io::ErrorKind::NotFound => PlatformErrorCode::NotFound,
        std::io::ErrorKind::PermissionDenied => PlatformErrorCode::PermissionDenied,
        std::io::ErrorKind::StorageFull => PlatformErrorCode::NoSpace,
        _ => PlatformErrorCode::Io,
    };
    PlatformError::new(code, e.to_string())
}

/// The mode `write_atomic` gives the new file, as the real platform does:
/// the mode of the file it replaces, else 0644.
fn mode_of(target: &Path) -> u32 {
    std::fs::metadata(target).map_or(0o644, |meta| meta.permissions().mode() & 0o7777)
}

/// How long apart the fake's tries for a busy lock are, as on the real
/// platform.
const LOCK_PAUSE: Duration = Duration::from_millis(50);

/// The real kernel `flock` on the real `path`, as the real platform takes
/// it, so a test's threads and processes wait for each other. The fake
/// never reads a clock: it counts the time it slept, so a lock it waits for
/// gives up a little later than `wait`. The error details are the real
/// platform's texts.
fn take_lock(path: &Path, wait: Duration) -> Result<FileLock, PlatformError> {
    let named = |what: String, e: &std::io::Error| PlatformError {
        detail: format!("{what}: {e}"),
        ..io_to_platform(e)
    };
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)
            .map_err(|e| named(format!("cannot make {}", dir.display()), &e))?;
    }
    #[expect(
        clippy::disallowed_methods,
        reason = "the fake Platform is the lock_file stand-in for tests; a lock file is never written"
    )]
    let opened = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path);
    let file = opened.map_err(|e| named(format!("cannot open the lock {}", path.display()), &e))?;
    let mut slept = Duration::ZERO;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(FileLock::new(file)),
            Err(TryLockError::WouldBlock) if slept < wait => {
                let pause = LOCK_PAUSE.min(wait.saturating_sub(slept));
                std::thread::sleep(pause);
                slept = slept.saturating_add(pause);
            }
            Err(TryLockError::WouldBlock) => {
                return Err(PlatformError::new(
                    PlatformErrorCode::TimedOut,
                    format!(
                        "the lock {} stayed busy for {} ms",
                        path.display(),
                        wait.as_millis()
                    ),
                ));
            }
            Err(TryLockError::Error(e)) => {
                return Err(named(
                    format!("cannot take the lock {}", path.display()),
                    &e,
                ));
            }
        }
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "the fake platform is what stands in for Platform::write_atomic, so it does the temp-file-then-rename itself"
)]
fn rename(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::rename(from, to)
}

impl Platform for FakePlatform {
    fn pid_alive(&self, pid: Pid) -> bool {
        self.log
            .record("pid_alive", CallKind::Read, pid.to_string());
        lock(&self.state).alive.contains(&pid)
    }

    fn process_started(&self, pid: Pid) -> Option<Timestamp> {
        self.log
            .record("process_started", CallKind::Read, pid.to_string());
        let state = lock(&self.state);
        state
            .started
            .get(&pid)
            .copied()
            .filter(|_| state.alive.contains(&pid))
    }

    fn parent_of(&self, pid: Pid) -> Option<(Pid, String)> {
        self.log
            .record("parent_of", CallKind::Read, pid.to_string());
        lock(&self.state).parents.get(&pid).cloned()
    }

    fn env_of(&self, pid: Pid, var: &str) -> Result<Option<String>, PlatformError> {
        self.log
            .record("env_of", CallKind::Read, format!("{pid} {var}"));
        let state = lock(&self.state);
        if let Some(err) = &state.env_fault {
            return Err(err.clone());
        }
        if !state.alive.contains(&pid) {
            return Err(PlatformError::new(
                PlatformErrorCode::NotFound,
                format!("no process {pid}"),
            ));
        }
        Ok(state.envs.get(&(pid, var.to_owned())).cloned())
    }

    fn run(&self, cmd: &Command, timeout: Duration) -> Result<RunOutput, PlatformError> {
        self.log.record("run", CallKind::Write, format!("{cmd:?}"));
        let mut state = lock(&self.state);
        state.runs.push((cmd.clone(), timeout));
        state.run_queue.pop_front().unwrap_or_else(|| {
            Ok(RunOutput {
                exit_code: 0,
                stdout: Vec::new(),
                stderr: Vec::new(),
                timed_out: false,
            })
        })
    }

    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), PlatformError> {
        self.log
            .record("write_atomic", CallKind::Write, path.display().to_string());
        if let Some(err) = lock(&self.state).write_fault.clone() {
            return Err(err);
        }
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        let n = self.temp_counter.fetch_add(1, Ordering::Relaxed);
        let name = path.file_name().map(|s| s.to_string_lossy().into_owned());
        let temp = dir.join(format!(".{}.tmp{n}", name.unwrap_or_default()));
        #[expect(
            clippy::disallowed_methods,
            reason = "the fake Platform is the write_atomic stand-in for tests"
        )]
        let result = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .and_then(|mut f| {
                f.write_all(bytes)?;
                f.set_permissions(std::fs::Permissions::from_mode(mode_of(path)))?;
                f.sync_all()
            })
            .and_then(|()| rename(&temp, path));
        match result {
            Ok(()) => {
                lock(&self.state).written.push(path.to_path_buf());
                Ok(())
            }
            Err(e) => {
                // Best effort: a failed write must leave no temporary file.
                std::fs::remove_file(&temp).ok();
                Err(io_to_platform(&e))
            }
        }
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), PlatformError> {
        self.log.record(
            "create_dir_all",
            CallKind::Write,
            path.display().to_string(),
        );
        if let Some(err) = lock(&self.state).write_fault.clone() {
            return Err(err);
        }
        std::fs::create_dir_all(path).map_err(|e| io_to_platform(&e))
    }

    fn remove_dir_all(&self, path: &Path) -> Result<(), PlatformError> {
        self.log.record(
            "remove_dir_all",
            CallKind::Write,
            path.display().to_string(),
        );
        if let Some(err) = lock(&self.state).write_fault.clone() {
            return Err(err);
        }
        std::fs::remove_dir_all(path).map_err(|e| io_to_platform(&e))
    }

    fn lock_file(&self, path: &Path, wait: Duration) -> Result<FileLock, PlatformError> {
        self.log
            .record("lock_file", CallKind::Write, path.display().to_string());
        take_lock(path, wait)
    }

    fn sleep_hold(&self) -> Option<&dyn SleepHold> {
        self.hold.as_ref().map(|h| h as &dyn SleepHold)
    }

    fn free_bytes(&self, path: &Path) -> Result<u64, PlatformError> {
        self.log
            .record("free_bytes", CallKind::Read, path.display().to_string());
        let state = lock(&self.state);
        match &state.free_fault {
            Some(err) => Err(err.clone()),
            None => Ok(state.free_bytes),
        }
    }

    fn on_network_fs(&self, path: &Path) -> Result<bool, PlatformError> {
        self.log
            .record("on_network_fs", CallKind::Read, path.display().to_string());
        let state = lock(&self.state);
        match &state.network_fs_fault {
            Some(err) => Err(err.clone()),
            None => Ok(state.network_fs),
        }
    }
}
