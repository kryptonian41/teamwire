//! `Platform::run`: one command in its own process group, with a timeout.

use std::io::{Read, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Child, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use nix::sys::signal::{Signal, killpg};
use teamwire_core::{Clock, Command, PlatformError, PlatformErrorCode, RunOutput};

use crate::{SystemClock, io_error};

/// How often the wait loop looks at the child. Short enough that a fast
/// command does not wait long, long enough not to burn a core.
const POLL: Duration = Duration::from_millis(10);

/// How long to wait for the output readers after the child has ended. A
/// process that left its group can keep the pipes open forever; the output
/// read so far is still returned.
const DRAIN: Duration = Duration::from_secs(2);

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

    let deadline = clock.monotonic() + timeout;
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
        if now >= deadline {
            kill_group(group);
            // Reap the direct child so it does not stay a zombie.
            drop(child.wait());
            break None;
        }
        thread::sleep(POLL.min(deadline - now));
    };
    drop(stdin_done);

    let stdout = drain(stdout);
    let stderr = drain(stderr);
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
    process.spawn().map_err(|err| {
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

fn reader<R: Read + Send + 'static>(mut pipe: R) -> Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = Vec::new();
        drop(pipe.read_to_end(&mut buf));
        drop(tx.send(buf));
    });
    rx
}

fn drain(rx: Option<Receiver<Vec<u8>>>) -> Vec<u8> {
    rx.and_then(|rx| rx.recv_timeout(DRAIN).ok())
        .unwrap_or_default()
}

fn exit_code(status: ExitStatus) -> i32 {
    status
        .code()
        .or_else(|| status.signal().map(|signal| 128 + signal))
        .unwrap_or(-1)
}
