//! `Platform::run`: one command in its own process group, with a timeout.

use std::io::{ErrorKind, Read, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Child, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use nix::sys::signal::{Signal, killpg};
use teamwire_core::{Clock, Command, PlatformError, PlatformErrorCode, RunOutput};

use crate::{SystemClock, io_error};

/// How often the wait loop looks at the child. Short enough that a fast
/// command does not wait long, long enough not to burn a core.
const POLL: Duration = Duration::from_millis(10);

/// How long to wait, for stdout and stderr together, for the output
/// readers after the child has ended. A process the child left behind can
/// keep the pipes open forever; the output read so far is still returned.
const DRAIN: Duration = Duration::from_secs(2);

/// Held while a child is started, so that one thread at a time starts one.
/// With `process_group` set, the standard library on macOS can let a child
/// started on one thread inherit a pipe another thread has just made for
/// its own child. The pipe then stays open until the other child exits, and
/// the first command's reader waits for it: 4 of 43,363 parallel runs of
/// `pwd` waited about 2 s that way. Starting a child takes milliseconds.
static SPAWN: Mutex<()> = Mutex::new(());

pub(crate) fn run_command(cmd: &Command, timeout: Duration) -> Result<RunOutput, PlatformError> {
    let clock = SystemClock;
    let mut child = spawn(cmd)?;
    let group = i32::try_from(child.id())
        .ok()
        .map(nix::unistd::Pid::from_raw);

    let stdin_done = child.stdin.take().map(|mut pipe| {
        let bytes = cmd.stdin.clone().unwrap_or_default();
        // A child that exits without reading its stdin gives BrokenPipe;
        // that is the child's choice, not an error of ours.
        thread::spawn(move || {
            drop(pipe.write_all(&bytes));
        })
    });
    let stdout = child.stdout.take().map(reader);
    let stderr = child.stderr.take().map(reader);

    // A timeout too large for `Instant` means "wait as long as the command
    // takes": adding it would panic.
    let deadline = clock.monotonic().checked_add(timeout);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(err) => {
                kill_group(group);
                drop(child.wait());
                return Err(io_error("waiting for the process", &err));
            }
        }
        let now = clock.monotonic();
        let Some(deadline) = deadline else {
            thread::sleep(POLL);
            continue;
        };
        if now >= deadline {
            kill_group(group);
            // Reap the direct child so it does not stay a zombie.
            drop(child.wait());
            break None;
        }
        thread::sleep(POLL.min(deadline - now));
    };
    drop(stdin_done);

    let drained = clock.monotonic().checked_add(DRAIN);
    let stdout = drain(stdout, drained, &cmd.program, "stdout");
    let stderr = drain(stderr, drained, &cmd.program, "stderr");
    Ok(match status {
        Some(status) => RunOutput {
            exit_code: exit_code(status),
            stdout,
            stderr,
            timed_out: false,
        },
        None => RunOutput {
            exit_code: RunOutput::TIMEOUT_EXIT,
            stdout,
            stderr,
            timed_out: true,
        },
    })
}

#[expect(
    clippy::disallowed_methods,
    reason = "teamwire-platform is the one crate that starts processes"
)]
fn spawn(cmd: &Command) -> Result<Child, PlatformError> {
    let mut process = std::process::Command::new(&cmd.program);
    process.args(&cmd.args);
    if let Some(cwd) = &cmd.cwd {
        process.current_dir(cwd);
    }
    if cmd.env_clear {
        process.env_clear();
    }
    process.envs(cmd.env.iter().map(|(k, v)| (k, v)));
    process
        .stdin(if cmd.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // A group of its own, so one signal reaches every descendant and the
        // terminal's Ctrl-C does not reach the child behind our back.
        .process_group(0);
    let one_at_a_time = SPAWN.lock().unwrap_or_else(PoisonError::into_inner);
    let child = process.spawn();
    drop(one_at_a_time);
    child.map_err(|err| {
        let mut e = io_error(&format!("starting {}", cmd.program), &err);
        if err.kind() == std::io::ErrorKind::NotFound {
            e.code = PlatformErrorCode::NotFound;
        }
        e
    })
}

fn kill_group(group: Option<nix::unistd::Pid>) {
    if let Some(group) = group {
        // ESRCH means the group is already gone, which is the goal.
        if let Err(errno) = killpg(group, Signal::SIGKILL) {
            tracing::debug!(%errno, "killpg after timeout");
        }
    }
}

/// One pipe's output: the bytes read so far, and a signal sent when the
/// pipe has closed.
struct Reader {
    bytes: Arc<Mutex<Vec<u8>>>,
    closed: Receiver<()>,
}

/// Reads `pipe` on its own thread, keeping each chunk as it arrives, so the
/// output is there even if the pipe never closes.
fn reader<R: Read + Send + 'static>(mut pipe: R) -> Reader {
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let (tx, closed) = mpsc::channel();
    let sink = Arc::clone(&bytes);
    thread::spawn(move || {
        let mut chunk = [0_u8; 8192];
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => sink
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .extend_from_slice(chunk.get(..n).unwrap_or_default()),
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        // The receiver is gone when `drain` stopped waiting; nothing to tell.
        tx.send(()).ok();
    });
    Reader { bytes, closed }
}

/// The output of one pipe once it has closed, or, at `deadline`, what was
/// read by then.
fn drain(reader: Option<Reader>, deadline: Option<Instant>, program: &str, pipe: &str) -> Vec<u8> {
    let Some(reader) = reader else {
        return Vec::new();
    };
    let wait = deadline.map_or(DRAIN, |d| {
        d.saturating_duration_since(SystemClock.monotonic())
    });
    if reader.closed.recv_timeout(wait).is_err() {
        tracing::warn!(
            program,
            pipe,
            "the pipe was still open {DRAIN:?} after the process ended; returning the output read so far"
        );
    }
    std::mem::take(&mut *reader.bytes.lock().unwrap_or_else(PoisonError::into_inner))
}

fn exit_code(status: ExitStatus) -> i32 {
    status
        .code()
        .or_else(|| status.signal().map(|signal| 128 + signal))
        .unwrap_or(-1)
}
