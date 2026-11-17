//! `Host::split_run` and `Host::close_pane`: `pane split` beside a pane,
//! `pane rename` for its label, then `pane run` of a short line that runs
//! the command from a private file.
//!
//! herdr types the `pane run` line into the new pane's shell. `herdr --skill`
//! says: "On macOS, `pane run` and `agent start` reject a command that
//! exceeds the terminal's current canonical line-input limit before queuing
//! it", and that limit is 1,024 bytes. A lead's first prompt alone comes
//! close to it. So the command goes into a file, in a new folder only the
//! current user can enter, and the typed line runs that file. The file
//! removes its folder before it starts the command.
//!
//! `pane run` queues the line and says nothing about a busy pane, so nothing
//! here retries it or waits for the new pane's shell.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use teamwire_core::{Host, HostError, HostErrorCode, LaunchSpec, Place, SplitRequest};

use crate::Herdr;
use crate::shapes::split_pane;
use crate::spawn::{CLEANUP_TIMEOUT, LAYOUT_TIMEOUT, budget};

/// Numbers the run folders of this process.
static RUN_FOLDERS: AtomicU64 = AtomicU64::new(0);

/// The `--env` values travel in herdr's own arguments, which any local user
/// can read with `ps`, so a caller passes no secret in `req.env`.
pub(crate) fn split_run(
    host: &Herdr,
    beside: &Place,
    req: &SplitRequest,
    timeout: Duration,
) -> Result<Place, HostError> {
    if req.argv.is_empty() {
        return Err(HostError::new(
            HostErrorCode::BadArgument,
            "split_run needs a program to run",
        ));
    }
    let deadline = host.clock.monotonic() + timeout;
    let folder = write_run_file(host, &req.argv)?;
    let result = split_in(host, beside, req, &folder, deadline);
    // The run file removes its folder once the shell reads it, so after a
    // failure nothing will.
    if result.is_err() {
        drop(std::fs::remove_dir_all(&folder));
    }
    result
}

fn split_in(
    host: &Herdr,
    beside: &Place,
    req: &SplitRequest,
    folder: &std::path::Path,
    deadline: Instant,
) -> Result<Place, HostError> {
    let cwd = req.cwd.display().to_string();
    let mut args: Vec<String> = [
        "pane",
        "split",
        &beside.id,
        "--direction",
        "right",
        "--cwd",
        &cwd,
        "--no-focus",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    for (key, value) in &req.env {
        args.push("--env".to_owned());
        args.push(format!("{key}={value}"));
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let pane = split_pane(host.call(&refs, budget(host, deadline, LAYOUT_TIMEOUT)?)?)?;
    let place = Place {
        host: host.id(),
        id: pane,
    };
    let run_file = folder.join("run").display().to_string();
    let line = format!("sh {}", LaunchSpec::shell_word(&run_file));
    let rest = (|| {
        host.call(
            &["pane", "rename", &place.id, &req.label],
            budget(host, deadline, LAYOUT_TIMEOUT)?,
        )?;
        host.call_text(
            &["pane", "run", &place.id, &line],
            budget(host, deadline, LAYOUT_TIMEOUT)?,
        )
    })();
    if let Err(e) = rest {
        // Nothing else knows this pane, so leaving it would leave an empty
        // pane in the user's tab.
        drop(host.call(&["pane", "close", &place.id], CLEANUP_TIMEOUT));
        return Err(e);
    }
    Ok(place)
}

/// Writes the run file into a new folder only the current user can enter,
/// and returns the folder. The file removes the folder, then replaces the
/// shell with the command.
fn write_run_file(host: &Herdr, argv: &[String]) -> Result<PathBuf, HostError> {
    use std::os::unix::fs::DirBuilderExt;
    let io = |what: &str, e: &dyn std::fmt::Display| {
        HostError::new(HostErrorCode::Other, format!("{what}: {e}"))
    };
    let folder = std::env::temp_dir().join(format!(
        "teamwire-herdr-run-{}-{}",
        std::process::id(),
        RUN_FOLDERS.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&folder)
        .map_err(|e| io("cannot make the run folder", &e))?;
    let text = format!(
        "rm -rf {}\nexec {}\n",
        LaunchSpec::shell_word(&folder.display().to_string()),
        LaunchSpec::shell_line(argv)
    );
    if let Err(e) = host
        .platform
        .write_atomic(&folder.join("run"), text.as_bytes())
    {
        drop(std::fs::remove_dir_all(&folder));
        return Err(io("cannot write the run file", &e));
    }
    Ok(folder)
}

/// `pane close`. A pane herdr no longer has counts as closed.
pub(crate) fn close_pane(host: &Herdr, place: &Place, timeout: Duration) -> Result<(), HostError> {
    match host.call(&["pane", "close", &place.id], timeout) {
        Ok(_) => Ok(()),
        Err(e) if e.code == HostErrorCode::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}
