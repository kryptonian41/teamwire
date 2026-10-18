//! Process liveness and environment reads.

use std::ffi::OsString;
use std::time::Duration;

use nix::errno::Errno;
use nix::sys::signal::kill;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use teamwire_core::{Command, Pid, PlatformError, PlatformErrorCode, Timestamp};

use crate::run::run_command;

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

/// When the process with this pid started, to the second, read from the
/// kernel (`sysinfo`). `None` when no such process exists, or the kernel
/// reports no start time (zero).
pub(crate) fn process_started(pid: Pid) -> Option<Timestamp> {
    nix_pid(pid)?;
    let target = sysinfo::Pid::from_u32(pid.get());
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[target]),
        true,
        ProcessRefreshKind::nothing(),
    );
    let secs = system.process(target)?.start_time();
    if secs == 0 {
        return None;
    }
    Timestamp::from_unix_millis(i64::try_from(secs).ok()?.checked_mul(1000)?)
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

/// The parent pid and program name of the process with this pid, read with
/// `ps` because `sysinfo` reports a program name cut to 15 bytes on Linux.
/// `None` when `ps` fails, times out, or finds no such process.
pub(crate) fn parent_of(pid: Pid) -> Option<(Pid, String)> {
    let cmd = Command::new("ps")
        .args(["-o", "ppid=,comm=", "-p"])
        .arg(pid.get().to_string());
    let out = run_command(&cmd, Duration::from_secs(2)).ok()?;
    if out.exit_code != 0 {
        return None;
    }
    parse_ps_parent(&String::from_utf8_lossy(&out.stdout))
}

/// `  123 /usr/bin/tmux` becomes `(123, "tmux")`.
fn parse_ps_parent(text: &str) -> Option<(Pid, String)> {
    let (ppid, comm) = text.trim().split_once(char::is_whitespace)?;
    let name = comm.trim().rsplit('/').next()?;
    Some((Pid::new(ppid.parse().ok()?), name.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_of_this_process_is_the_real_parent() {
        let me = Pid::new(std::process::id());
        let (parent, name) = parent_of(me).unwrap();
        assert_eq!(parent.get(), std::os::unix::process::parent_id());
        assert!(!name.is_empty());
    }

    #[test]
    fn ps_output_keeps_the_last_path_part() {
        let (pid, name) = parse_ps_parent("  4242 /opt/homebrew/bin/tmux\n").unwrap();
        assert_eq!((pid.get(), name.as_str()), (4242, "tmux"));
        assert!(parse_ps_parent("").is_none());
    }
}
