//! `FakePlatform`: an in-process `Platform` with environments per pid, free
//! space, a network-filesystem flag and a sleep hold that records its life.
//! It writes real files (in a directory the test owns) so that a test can
//! read them back, and it never starts a process.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use teamwire_core::{
    Command, HoldInfo, Pid, Platform, PlatformError, PlatformErrorCode, ReleaseOutcome, RunOutput,
    SleepHold,
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
        lock(&self.state)
            .records
            .iter()
            .find(|r| r.lead == lead_pid && r.running && r.is_hold)
            .map(|r| HoldInfo {
                pid: r.pid,
                owned: r.owned,
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
    envs: HashMap<(Pid, String), String>,
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
                envs: HashMap::new(),
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

    pub fn set_env(&self, pid: Pid, var: &str, value: &str) {
        lock(&self.state)
            .envs
            .insert((pid, var.to_owned()), value.to_owned());
    }

    /// Every later `env_of` fails with `err` until cleared with `None`.
    pub fn fail_env_reads(&self, err: Option<PlatformError>) {
        lock(&self.state).env_fault = err;
    }

    /// Every later `write_atomic` fails with `err` until cleared.
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
        let result = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .and_then(|mut f| f.write_all(bytes).and_then(|()| f.sync_all()))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn pid(n: u32) -> Pid {
        Pid::new(n)
    }

    #[test]
    fn env_read_failure_is_reported_and_env_needs_a_live_pid() {
        let p = FakePlatform::new();
        p.set_alive(pid(10), true);
        p.set_env(pid(10), "HERDR_PANE_ID", "w1:p2");
        assert_eq!(
            p.env_of(pid(10), "HERDR_PANE_ID").unwrap().as_deref(),
            Some("w1:p2")
        );
        assert_eq!(p.env_of(pid(10), "OTHER").unwrap(), None);
        assert_eq!(
            p.env_of(pid(11), "X").unwrap_err().code,
            PlatformErrorCode::NotFound
        );
        p.fail_env_reads(Some(PlatformError::new(
            PlatformErrorCode::PermissionDenied,
            "no",
        )));
        assert_eq!(
            p.env_of(pid(10), "HERDR_PANE_ID").unwrap_err().code,
            PlatformErrorCode::PermissionDenied
        );
        p.fail_env_reads(None);
        assert!(p.env_of(pid(10), "HERDR_PANE_ID").is_ok());
    }

    #[test]
    fn low_disk_and_network_fs_are_reported() {
        let p = FakePlatform::new();
        p.set_free_bytes(1000);
        assert_eq!(p.free_bytes(Path::new("/x")).unwrap(), 1000);
        p.fail_free_bytes(Some(PlatformError::new(PlatformErrorCode::Io, "stat")));
        assert!(p.free_bytes(Path::new("/x")).is_err());
        assert!(!p.on_network_fs(Path::new("/x")).unwrap());
        p.set_network_fs(true);
        assert!(p.on_network_fs(Path::new("/x")).unwrap());
    }

    #[test]
    fn write_atomic_replaces_whole_file_and_full_disk_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = FakePlatform::new();
        let path = dir.path().join("a.json");
        p.write_atomic(&path, b"one").unwrap();
        p.write_atomic(&path, b"two").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        p.fail_writes(Some(PlatformError::new(PlatformErrorCode::NoSpace, "full")));
        assert_eq!(
            p.write_atomic(&path, b"three").unwrap_err().code,
            PlatformErrorCode::NoSpace
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        assert_eq!(p.written(), [path.clone(), path]);
    }

    #[test]
    fn write_into_a_missing_directory_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let err = FakePlatform::new()
            .write_atomic(&dir.path().join("no/such/f"), b"x")
            .unwrap_err();
        assert_eq!(err.code, PlatformErrorCode::NotFound);
    }

    #[test]
    fn run_replays_queued_results_and_records_commands() {
        let p = FakePlatform::new();
        p.queue_run(Ok(RunOutput {
            exit_code: RunOutput::TIMEOUT_EXIT,
            stdout: Vec::new(),
            stderr: Vec::new(),
            timed_out: true,
        }));
        let cmd = Command::new("sleep").arg("9");
        assert!(p.run(&cmd, Duration::from_secs(1)).unwrap().timed_out);
        assert!(p.run(&cmd, Duration::from_secs(1)).unwrap().success());
        assert_eq!(p.runs().len(), 2);
        assert_eq!(p.log().count(CallKind::Write), 2);
    }

    #[test]
    fn sleep_hold_records_start_and_release() {
        let p = FakePlatform::new();
        let hold = p.sleep_hold().unwrap();
        let lead = pid(42);
        assert!(hold.find_existing(lead).is_none());
        let info = hold.start(lead).unwrap();
        assert!(info.owned);
        assert_eq!(hold.find_existing(lead), Some(info));
        assert_eq!(hold.release(&info, lead).unwrap(), ReleaseOutcome::Released);
        assert_eq!(
            hold.release(&info, lead).unwrap(),
            ReleaseOutcome::AlreadyGone
        );
        assert_eq!(
            p.hold_events(),
            [
                HoldEvent::Started {
                    lead,
                    hold: info.pid
                },
                HoldEvent::Released {
                    hold: info.pid,
                    outcome: ReleaseOutcome::Released
                },
                HoldEvent::Released {
                    hold: info.pid,
                    outcome: ReleaseOutcome::AlreadyGone
                },
            ]
        );
    }

    #[test]
    fn a_hold_the_user_started_is_never_released() {
        let p = FakePlatform::new();
        p.add_external_hold(pid(42), pid(500));
        let hold = p.sleep_hold().unwrap();
        let found = hold.find_existing(pid(42)).unwrap();
        assert!(!found.owned);
        assert_eq!(
            hold.release(&found, pid(42)).unwrap(),
            ReleaseOutcome::NotOwned
        );
        assert!(hold.find_existing(pid(42)).is_some());
    }

    #[test]
    fn a_reused_hold_pid_is_left_alone() {
        let p = FakePlatform::new();
        let hold = p.sleep_hold().unwrap();
        let info = hold.start(pid(1)).unwrap();
        p.reuse_hold_pid(info.pid);
        assert_eq!(
            hold.release(&info, pid(1)).unwrap(),
            ReleaseOutcome::NotAHold
        );
        assert!(hold.find_existing(pid(1)).is_none());
    }

    #[test]
    fn hold_start_can_fail_and_linux_has_no_hold() {
        let p = FakePlatform::new();
        p.fail_hold_start(Some(PlatformError::unsupported()));
        assert!(p.sleep_hold().unwrap().start(pid(1)).is_err());
        assert!(FakePlatform::without_sleep_hold().sleep_hold().is_none());
    }
}
