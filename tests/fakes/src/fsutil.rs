//! Small file helpers the fake binaries share. They stand in for
//! `Platform::write_atomic`, which a test double must not depend on.

use std::io::Write as _;
use std::path::{Path, PathBuf};

/// Replaces `path` with `bytes`, all at once: a reader sees the old file or
/// the new one, never half of it. Fails when the parent directory is missing.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temp)
        .and_then(|mut f| f.write_all(bytes).and_then(|()| f.sync_all()))
        .and_then(|()| rename(&temp, path));
    if written.is_err() {
        std::fs::remove_file(&temp).ok();
    }
    written
}

#[expect(
    clippy::disallowed_methods,
    reason = "the fake binaries cannot call Platform::write_atomic, so they rename a temporary file themselves"
)]
fn rename(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::rename(from, to)
}

/// Appends `bytes` to `path`, creating it if needed. One `write` call, so
/// concurrent appenders of whole lines do not interleave inside a line.
pub(crate) fn append(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)?
        .write_all(bytes)
}

/// The file's text, or `None` when it is missing or unreadable.
pub(crate) fn read_text(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// True when a process with this id exists (`kill -0`).
#[expect(
    clippy::disallowed_methods,
    reason = "the fake binaries check a child's liveness with kill(1); this crate may start processes"
)]
pub(crate) fn pid_alive(pid: u32) -> bool {
    // kill(1) with pid 0 means the whole process group.
    pid != 0
        && std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
}

/// Sends SIGTERM to a process. Best effort: an error means it was gone.
#[expect(
    clippy::disallowed_methods,
    reason = "the fake binaries end a child with kill(1); this crate may start processes"
)]
pub(crate) fn terminate(pid: u32) {
    if pid == 0 {
        return;
    }
    std::process::Command::new("kill")
        .arg(pid.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok();
}

/// Elapsed real time, for the fake binaries' waits. They run as separate
/// processes, so the `Clock` trait has no place in them.
#[derive(Debug)]
pub(crate) struct Stopwatch(std::time::Instant);

impl Stopwatch {
    #[expect(
        clippy::disallowed_methods,
        reason = "a fake binary waits on real time; no Clock is injected into a separate process"
    )]
    pub(crate) fn start() -> Self {
        Self(std::time::Instant::now())
    }

    pub(crate) fn elapsed(&self) -> std::time::Duration {
        self.0.elapsed()
    }
}

/// A `Path` under `dir` made of `parts`.
pub(crate) fn join(dir: &Path, parts: &[&str]) -> PathBuf {
    parts.iter().fold(dir.to_path_buf(), |p, part| p.join(part))
}
