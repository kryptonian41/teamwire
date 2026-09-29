//! Process liveness and environment reads.

use std::ffi::OsString;

use nix::errno::Errno;
use nix::sys::signal::kill;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use teamwire_core::{Pid, PlatformError, PlatformErrorCode};

fn nix_pid(pid: Pid) -> Option<nix::unistd::Pid> {
    // Pid 0 would signal our own process group, so it never counts as a process.
    i32::try_from(pid.get())
        .ok()
        .filter(|raw| *raw > 0)
        .map(nix::unistd::Pid::from_raw)
}

/// True when a process with this pid exists. A process of another user
/// answers `EPERM`, which still means it exists.
pub(crate) fn pid_alive(pid: Pid) -> bool {
    match nix_pid(pid) {
        Some(raw) => matches!(kill(raw, None), Ok(()) | Err(Errno::EPERM)),
        None => false,
    }
}

/// The value of `var` in the process's environment, read from the kernel
/// (`sysinfo`). `Ok(None)` means the variable is not set.
///
/// The kernel hides the environment of another user's process and, on
/// macOS, of Apple-signed system binaries such as `/bin/sh`. `sysinfo` shows
/// both as an empty environment. A real process nearly always has some
/// variable set, so an empty environment is reported as an error rather than
/// as "variable not set". A wrong `None` would let a caller resolve the wrong
/// lead process without any sign of trouble.
pub(crate) fn env_of(pid: Pid, var: &str) -> Result<Option<String>, PlatformError> {
    let target = sysinfo::Pid::from_u32(pid.get());
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[target]),
        true,
        ProcessRefreshKind::nothing().with_environ(UpdateKind::Always),
    );
    let Some(process) = system.process(target) else {
        return Err(PlatformError::new(
            PlatformErrorCode::NotFound,
            format!("no process {pid}"),
        ));
    };
    let environ = process.environ();
    if environ.is_empty() {
        return Err(PlatformError::new(
            PlatformErrorCode::PermissionDenied,
            format!("the environment of process {pid} is empty or not readable"),
        ));
    }
    Ok(environ.iter().find_map(|entry| value_of(entry, var)))
}

fn value_of(entry: &OsString, var: &str) -> Option<String> {
    let text = entry.to_string_lossy();
    let (key, value) = text.split_once('=')?;
    (key == var).then(|| value.to_owned())
}
