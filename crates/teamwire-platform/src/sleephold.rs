//! The sleep hold: an untimed `caffeinate -ims -w <lead pid>` that keeps the
//! Mac awake while the lead session lives (SLP-1). The flag reading follows
//! `caffeinate`'s own getopt rules, as the Python tooling's parser does, so
//! `-ims -w 5`, `-imsw5` and `-i -w5` are one hold.

use std::os::unix::process::CommandExt;
use std::process::Stdio;
use std::time::Duration;

use nix::sys::signal::{Signal, kill};
use teamwire_core::{
    Clock, Command, HoldInfo, Pid, PlatformError, PlatformErrorCode, ReleaseOutcome, SleepHold,
};

use crate::run::run_command;
use crate::{MacOs, SystemClock, io_error, process};

/// Bound on the `ps` calls (DEL-14).
const PS_TIMEOUT: Duration = Duration::from_secs(10);

/// How long `release` waits for a killed hold to exit, and how often it looks.
const EXIT_WAIT: Duration = Duration::from_secs(2);
const EXIT_POLL: Duration = Duration::from_millis(10);

/// What `parse_caffeinate` read from one command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Caffeinate {
    /// The value of `-w`, which is the pid the hold waits on.
    pub wait: Option<String>,
    /// True when `-t` is given: the hold expires, so it does not last as long
    /// as the session.
    pub timed: bool,
}

impl Caffeinate {
    /// An untimed hold that waits on `pid`.
    fn is_untimed_hold_on(&self, pid: Pid) -> bool {
        !self.timed && self.wait.as_deref() == Some(pid.to_string().as_str())
    }
}

/// `None` when the words are not a `caffeinate` command. Reading stops at the
/// first word that is not an option, because that word starts a wrapped
/// command with flags of its own.
pub(crate) fn parse_caffeinate(words: &[&str]) -> Option<Caffeinate> {
    let (program, args) = words.split_first()?;
    let name = program.rsplit('/').next().unwrap_or(program);
    if name != "caffeinate" {
        return None;
    }
    let mut out = Caffeinate {
        wait: None,
        timed: false,
    };
    let mut i = 0;
    while let Some(word) = args.get(i).filter(|w| is_option(w)) {
        for (offset, flag) in word.char_indices().skip(1) {
            if flag != 't' && flag != 'w' {
                continue;
            }
            // The value is glued to the flag (`-w5`) or is the next word.
            let mut value = word.get(offset + 1..).unwrap_or("").to_owned();
            if let (true, Some(next)) = (value.is_empty(), args.get(i + 1)) {
                i += 1;
                (*next).clone_into(&mut value);
            }
            if flag == 't' {
                out.timed = true;
            } else {
                out.wait = Some(value);
            }
            break;
        }
        i += 1;
    }
    Some(out)
}

fn is_option(word: &str) -> bool {
    let mut chars = word.chars();
    chars.next() == Some('-') && chars.next().is_some_and(|c| c.is_ascii_lowercase())
}

/// Pids of every untimed `caffeinate … -w <pid>` in `ps -eo pid=,args=`
/// output, in listing order.
pub(crate) fn holds_in(ps_text: &str, pid: Pid) -> Vec<u32> {
    ps_text
        .lines()
        .filter_map(|line| {
            let words: Vec<&str> = line.split_whitespace().collect();
            let (hold_pid, command) = words.split_first()?;
            let hold_pid = hold_pid.parse().ok()?;
            parse_caffeinate(command)
                .filter(|parsed| parsed.is_untimed_hold_on(pid))
                .map(|_| hold_pid)
        })
        .collect()
}

/// The text `program args…` printed. An `Err` means the listing could not be
/// read (not started, timed out, non-zero exit), which is not the same as an
/// empty listing.
fn ps(program: &str, args: &[&str]) -> Result<String, PlatformError> {
    let out = run_command(
        &Command::new(program).args(args.iter().copied()),
        PS_TIMEOUT,
    )?;
    if out.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    Err(PlatformError::new(
        PlatformErrorCode::Other,
        format!("{program} did not finish cleanly (exit {})", out.exit_code),
    ))
}

impl SleepHold for MacOs {
    /// The hold's `owned` flag is false: the process list cannot say who
    /// started it. The caller knows from the team records and sets it.
    fn find_existing(&self, lead_pid: Pid) -> Option<HoldInfo> {
        // The trait returns an Option, so a `ps` that cannot be read looks the
        // same as "no hold". The shape stays because launch and the fakes
        // depend on it.
        let listing = ps("ps", &["-eo", "pid=,args="]).ok()?;
        holds_in(&listing, lead_pid).first().map(|pid| HoldInfo {
            pid: Pid::new(*pid),
            owned: false,
        })
    }

    fn start(&self, lead_pid: Pid) -> Result<HoldInfo, PlatformError> {
        let child = spawn_caffeinate(lead_pid)?;
        let pid = Pid::new(child.id());
        // Reap the hold when it ends, so a released hold is gone rather than
        // a zombie that still answers `kill(pid, 0)`.
        std::thread::spawn(move || {
            let mut child = child;
            drop(child.wait());
        });
        Ok(HoldInfo { pid, owned: true })
    }

    fn release(&self, hold: &HoldInfo, lead_pid: Pid) -> Result<ReleaseOutcome, PlatformError> {
        release_with(*hold, lead_pid, "ps")
    }
}

/// `release`, with the program that reads the process list as a parameter so
/// a test can make that read fail.
fn release_with(
    hold: HoldInfo,
    lead_pid: Pid,
    ps_program: &str,
) -> Result<ReleaseOutcome, PlatformError> {
    if !hold.owned {
        return Ok(ReleaseOutcome::NotOwned);
    }
    if !process::pid_alive(hold.pid) {
        return Ok(ReleaseOutcome::AlreadyGone);
    }
    // Checked right before the kill: if the hold exited and the system
    // gave its pid to another process, that process must not be killed.
    let pid_text = hold.pid.to_string();
    let args = match ps(ps_program, &["-o", "args=", "-p", &pid_text]) {
        Ok(args) => args,
        // `ps -p` also fails when the process ended after the check above.
        Err(_) if !process::pid_alive(hold.pid) => return Ok(ReleaseOutcome::AlreadyGone),
        // Unknown is not gone: the hold may still be running.
        Err(err) => return Err(err),
    };
    let words: Vec<&str> = args.split_whitespace().collect();
    if words.is_empty() {
        return Ok(ReleaseOutcome::AlreadyGone);
    }
    let still_hold = parse_caffeinate(&words).is_some_and(|c| c.is_untimed_hold_on(lead_pid));
    if !still_hold {
        return Ok(ReleaseOutcome::NotAHold);
    }
    let raw = i32::try_from(hold.pid.get())
        .map(nix::unistd::Pid::from_raw)
        .map_err(|_| PlatformError::new(PlatformErrorCode::Other, "pid out of range"))?;
    match kill(raw, Signal::SIGTERM) {
        Ok(()) => wait_gone(hold.pid),
        Err(nix::errno::Errno::ESRCH) => Ok(ReleaseOutcome::AlreadyGone),
        Err(errno) => Err(io_error(
            &format!("releasing caffeinate {}", hold.pid),
            &std::io::Error::from(errno),
        )),
    }
}

/// SIGTERM is asynchronous. Return `Released` only once the process is gone,
/// so a `find_existing` right after `release` cannot see the hold. A hold
/// that ignores SIGTERM for `EXIT_WAIT` is an error, not a success.
fn wait_gone(pid: Pid) -> Result<ReleaseOutcome, PlatformError> {
    let clock = SystemClock;
    let deadline = clock.monotonic() + EXIT_WAIT;
    while process::pid_alive(pid) {
        if clock.monotonic() >= deadline {
            return Err(PlatformError::new(
                PlatformErrorCode::Other,
                format!("caffeinate {pid} was sent SIGTERM but is still running"),
            ));
        }
        std::thread::sleep(EXIT_POLL);
    }
    Ok(ReleaseOutcome::Released)
}

#[expect(
    clippy::disallowed_methods,
    reason = "teamwire-platform is the one crate that starts processes; the hold outlives any single run() call"
)]
fn spawn_caffeinate(lead_pid: Pid) -> Result<std::process::Child, PlatformError> {
    std::process::Command::new("caffeinate")
        .args(["-ims", "-w", &lead_pid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // Its own session: the hold must outlive the launcher and not receive
        // the terminal's signals.
        .process_group(0)
        .spawn()
        .map_err(|err| io_error("starting caffeinate", &err))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "the test needs a live process that Platform::run would wait for"
    )]
    fn release_reports_error_when_ps_fails() {
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let hold = HoldInfo {
            pid: Pid::new(child.id()),
            owned: true,
        };
        let outcome = release_with(hold, Pid::new(1), "/nonexistent/ps");
        let alive = process::pid_alive(hold.pid);
        drop(child.kill());
        drop(child.wait());
        assert!(outcome.is_err(), "{outcome:?}");
        assert!(alive, "a failed check must not kill the process");
    }

    const PS: &str = "  101 /usr/bin/caffeinate -ims -w 500\n  102 caffeinate -i -t 300 -w 500\n  103 caffeinate -imsw500\n  104 caffeinate -i -w5000\n  105 caffeinate -ims claude -w 500\n";

    #[test]
    fn only_untimed_unwrapped_holds_on_the_pid_count() {
        assert_eq!(holds_in(PS, Pid::new(500)), [101, 103]);
    }

    #[test]
    fn a_pid_that_is_a_prefix_or_suffix_does_not_match() {
        assert!(holds_in(PS, Pid::new(5)).is_empty());
        assert_eq!(holds_in(PS, Pid::new(5000)), [104]);
    }

    #[test]
    fn flag_spellings_are_the_same_hold() {
        for line in [
            "caffeinate -ims -w 5",
            "caffeinate -imsw5",
            "caffeinate -i -w5",
            "/usr/bin/caffeinate -i -m -w 5",
        ] {
            let words: Vec<&str> = line.split_whitespace().collect();
            let parsed = parse_caffeinate(&words).unwrap();
            assert!(parsed.is_untimed_hold_on(Pid::new(5)), "{line}");
        }
    }

    #[test]
    fn t_marks_the_hold_timed_and_other_programs_are_not_caffeinate() {
        let words = ["caffeinate", "-t", "10", "-w", "5"];
        assert!(parse_caffeinate(&words).unwrap().timed);
        assert_eq!(parse_caffeinate(&["sleep", "-w", "5"]), None);
        assert_eq!(parse_caffeinate(&[]), None);
        assert_eq!(parse_caffeinate(&["caffeinate"]).unwrap().wait, None);
    }

    #[test]
    fn garbage_lines_are_skipped() {
        assert!(holds_in("\nabc caffeinate -w 5\n7\n", Pid::new(5)).is_empty());
    }
}
