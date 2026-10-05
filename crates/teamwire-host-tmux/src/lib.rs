//! The `Host` trait for tmux: `new-window -e` to spawn with the member's
//! environment, `load-buffer` plus `paste-buffer -p` plus `send-keys Enter`
//! to type, `capture-pane` for the screen, the pane's process tree for its
//! occupant.
//!
//! Never: type into a pane before the screen check of ADR 0006 point 3a
//! (tmux does not guard dialogs); run tmux except through `Platform::run`
//! with a timeout.
//!
//! Invariants: every call after `spawn` names its pane by its `%` id and
//! nothing else, so a session or window name is never taken for a place;
//! `spawn` and `list` leave the choice of session to tmux; `list` and
//! `live_agent_names` report only panes this adapter started, marked with
//! the pane option `@teamwire_name`, never the user's own panes.

mod call;
mod keys;
mod tree;

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use teamwire_core::{
    Host, HostCaps, HostError, HostErrorCode, HostId, Key, Occupant, OneLine, Pid, Place,
    PlaceState, Platform, SpawnRequest, SubmitAck,
};

use call::{Failure, call};

/// The session `spawn` creates when the tmux server has none.
const SESSION: &str = "teamwire";
/// The pane option that marks a pane this adapter started, holding the
/// member's name.
const NAME_OPTION: &str = "@teamwire_name";
/// The size of a session `spawn` creates. A detached session has no client
/// to size it, and Claude's dialogs read badly at 80 columns.
const SESSION_SIZE: [&str; 4] = ["-x", "200", "-y", "50"];
/// The wait between the paste and the Enter, so the program has taken the
/// pasted text in before it sees the key.
const PASTE_SETTLE: Duration = Duration::from_millis(100);

/// Numbers the paste buffers of this process.
static BUFFER_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The tmux host adapter (id `tmux`).
///
/// A place is a pane, named by its `%<n>` id, which is also the value of
/// `TMUX_PANE` inside the pane. Errors: a pane, or the whole server, that is
/// not there is `NotFound`; a timeout is `Timeout`; any other tmux failure
/// is `Other` with tmux's message in the detail.
pub struct Tmux {
    platform: Arc<dyn Platform>,
    socket: Option<String>,
}

impl Tmux {
    pub const ID: &'static str = "tmux";

    /// tmux's commands run through `platform`, on the server the process
    /// inherits (`TMUX`, or the default socket).
    pub fn new(platform: Arc<dyn Platform>) -> Self {
        Self {
            platform,
            socket: None,
        }
    }

    /// Runs every tmux command on the named socket (`tmux -L <name>`), so a
    /// test never touches the user's server.
    #[must_use]
    pub fn with_socket(mut self, name: impl Into<String>) -> Self {
        self.socket = Some(name.into());
        self
    }

    /// The platform tmux's commands run through.
    pub fn platform(&self) -> &dyn Platform {
        self.platform.as_ref()
    }

    fn run(&self, args: &[&str], timeout: Duration) -> Result<String, Failure> {
        call(
            self.platform.as_ref(),
            self.socket.as_deref(),
            args,
            None,
            timeout,
        )
    }

    /// The pane id of `place`, or `NotFound` when the id is not a pane id.
    /// tmux would also resolve a session or window name, and a place must
    /// never be one.
    fn pane(place: &Place) -> Result<&str, HostError> {
        if place.id.starts_with('%') && place.id.len() > 1 {
            Ok(&place.id)
        } else {
            Err(HostError::new(
                HostErrorCode::NotFound,
                format!("{} is not a tmux pane id", place.id),
            ))
        }
    }

    /// `(pane id, member name)` for every pane this adapter started.
    fn marked_panes(&self, timeout: Duration) -> Result<Vec<(String, String)>, HostError> {
        let format = format!("#{{pane_id}} #{{{NAME_OPTION}}}");
        let listing = match self.run(&["list-panes", "-a", "-F", &format], timeout) {
            Ok(text) => text,
            Err(Failure::NoServer(_)) => return Ok(Vec::new()),
            Err(f) => return Err(f.into()),
        };
        Ok(listing
            .lines()
            .filter_map(|line| line.split_once(' '))
            .filter(|(_, name)| !name.trim().is_empty())
            .map(|(id, name)| (id.to_owned(), name.trim().to_owned()))
            .collect())
    }

    fn spawn_pane(&self, req: &SpawnRequest, timeout: Duration) -> Result<String, HostError> {
        if req.argv.is_empty() {
            return Err(HostError::new(
                HostErrorCode::BadArgument,
                "spawn needs a program to run",
            ));
        }
        let cwd = req.cwd.display().to_string();
        let mut shared: Vec<String> =
            ["-d", "-P", "-F", "#{pane_id}", "-n", &req.title, "-c", &cwd]
                .iter()
                .map(|s| keys::escape_arg(s))
                .collect();
        for (key, value) in &req.env {
            if key.is_empty() || key.contains('=') {
                return Err(HostError::new(
                    HostErrorCode::BadArgument,
                    format!("{key:?} is not an environment variable name"),
                ));
            }
            shared.push("-e".to_owned());
            shared.push(keys::escape_arg(&format!("{key}={value}")));
        }
        shared.push("--".to_owned());
        shared.extend(req.argv.iter().map(|a| keys::escape_arg(a)));

        let with = |head: &[&str]| -> Vec<String> {
            head.iter()
                .map(|s| (*s).to_owned())
                .chain(shared.iter().cloned())
                .collect()
        };
        let window = with(&["new-window"]);
        let mut session = vec!["new-session", "-s", SESSION];
        session.extend(SESSION_SIZE);
        let session = with(&session);
        let run_owned = |args: &[String]| {
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            self.run(&refs, timeout)
        };

        let mut out = run_owned(&window);
        if matches!(out, Err(Failure::NoServer(_))) {
            out = run_owned(&session);
            // A second spawn started the server between our two calls.
            if matches!(&out, Err(Failure::Host(e)) if e.detail.contains("duplicate session")) {
                out = run_owned(&window);
            }
        }
        let id = out?.trim().to_owned();
        if !id.starts_with('%') {
            return Err(HostError::new(
                HostErrorCode::Other,
                format!("tmux printed no pane id: {}", call::clip(&id)),
            ));
        }
        Ok(id)
    }
}

impl fmt::Debug for Tmux {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tmux").finish_non_exhaustive()
    }
}

impl Host for Tmux {
    fn id(&self) -> HostId {
        HostId::from_static(Self::ID)
    }

    fn capabilities(&self) -> HostCaps {
        HostCaps {
            can_type: true,
            reports_state: false,
            knows_session_of_place: false,
            submit_ack: SubmitAck::None,
            sets_env_at_spawn: true,
            guards_dialogs: false,
        }
    }

    fn spawn(&self, req: &SpawnRequest, timeout: Duration) -> Result<Place, HostError> {
        let id = self.spawn_pane(req, timeout)?;
        let place = Place {
            host: self.id(),
            id,
        };
        // A pane that cannot be marked would never be listed or cleaned up.
        if let Err(e) = self.run(
            &[
                "set-option",
                "-p",
                "-t",
                &place.id,
                NAME_OPTION,
                req.name.as_str(),
            ],
            timeout,
        ) {
            drop(self.run(&["kill-pane", "-t", &place.id], timeout));
            return Err(e.into());
        }
        Ok(place)
    }

    fn submit(&self, place: &Place, text: &OneLine, timeout: Duration) -> Result<(), HostError> {
        let pane = Self::pane(place)?;
        let buffer = format!(
            "teamwire-{}-{}",
            std::process::id(),
            BUFFER_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        call(
            self.platform.as_ref(),
            self.socket.as_deref(),
            &["load-buffer", "-b", &buffer, "-"],
            Some(text.as_str().as_bytes()),
            timeout,
        )?;
        // `-p` sends the text as a bracketed paste where the program asked
        // for one; `-d` drops the buffer afterwards.
        if let Err(e) = self.run(
            &["paste-buffer", "-p", "-d", "-b", &buffer, "-t", pane],
            timeout,
        ) {
            drop(self.run(&["delete-buffer", "-b", &buffer], timeout));
            return Err(e.into());
        }
        std::thread::sleep(PASTE_SETTLE);
        self.run(&["send-keys", "-t", pane, "Enter"], timeout)?;
        Ok(())
    }

    fn state(&self, place: &Place, timeout: Duration) -> Result<PlaceState, HostError> {
        let Ok(pane) = Self::pane(place) else {
            return Ok(PlaceState::Gone);
        };
        match self.run(
            &["display-message", "-p", "-t", pane, "#{pane_dead}"],
            timeout,
        ) {
            Ok(text) => Ok(match text.trim() {
                "0" => PlaceState::Unknown,
                _ => PlaceState::Gone,
            }),
            Err(Failure::NoServer(_) | Failure::NoTarget(_)) => Ok(PlaceState::Gone),
            Err(f) => Err(f.into()),
        }
    }

    fn occupant(&self, place: &Place, timeout: Duration) -> Result<Occupant, HostError> {
        let pane = Self::pane(place)?;
        let text = self.run(
            &["display-message", "-p", "-t", pane, "#{pane_pid}"],
            timeout,
        )?;
        // tmux 3.x prints nothing and exits 0 for a pane that is not there.
        let Ok(root) = text.trim().parse::<u32>() else {
            return Err(HostError::new(
                HostErrorCode::NotFound,
                format!("tmux has no pane {}", place.id),
            ));
        };
        let pids: Vec<Pid> = tree::tree_of(self.platform.as_ref(), Pid::new(root), timeout)?;
        Ok(Occupant {
            session: None,
            pids,
        })
    }

    fn read_tail(&self, place: &Place, lines: u16, timeout: Duration) -> Result<String, HostError> {
        let pane = Self::pane(place)?;
        let from = format!("-{lines}");
        let text = self.run(
            &["capture-pane", "-p", "-J", "-t", pane, "-S", &from],
            timeout,
        )?;
        Ok(last_lines(&text, usize::from(lines)))
    }

    fn send_keys(&self, place: &Place, keys: &[Key], timeout: Duration) -> Result<(), HostError> {
        let pane = Self::pane(place)?;
        for key in keys {
            let tail = keys::send_args(*key);
            let mut args = vec!["send-keys", "-t", pane];
            args.extend(tail.iter().map(String::as_str));
            self.run(&args, timeout)?;
        }
        Ok(())
    }

    fn close(&self, place: &Place, timeout: Duration) -> Result<(), HostError> {
        let pane = Self::pane(place)?;
        // After a tmux server restart the pane ids start again at `%0`, so a
        // recorded id can name a pane that is not ours. Only a pane this
        // adapter marked is killed.
        let format = format!("#{{{NAME_OPTION}}}");
        let mark = self.run(&["display-message", "-p", "-t", pane, &format], timeout)?;
        if mark.trim().is_empty() {
            return Err(HostError::new(
                HostErrorCode::NotFound,
                format!("{pane} is not a pane teamwire started"),
            ));
        }
        self.run(&["kill-pane", "-t", pane], timeout)?;
        Ok(())
    }

    fn list(&self, timeout: Duration) -> Result<Vec<Place>, HostError> {
        let host = self.id();
        Ok(self
            .marked_panes(timeout)?
            .into_iter()
            .map(|(id, _)| Place {
                host: host.clone(),
                id,
            })
            .collect())
    }

    fn live_agent_names(&self, timeout: Duration) -> Result<Vec<String>, HostError> {
        Ok(self
            .marked_panes(timeout)?
            .into_iter()
            .map(|(_, name)| name)
            .collect())
    }

    fn place_env_var(&self) -> Option<&'static str> {
        Some("TMUX_PANE")
    }
}

/// The last `n` lines of `screen`, with the empty lines tmux pads a tall
/// pane with cut off the end first.
fn last_lines(screen: &str, n: usize) -> String {
    let all: Vec<&str> = screen.trim_end().lines().collect();
    let start = all.len().saturating_sub(n);
    all.iter()
        .skip(start)
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_lines_drops_padding_then_takes_the_end() {
        let screen = "a\nb\nc\nd\n\n\n\n";
        assert_eq!(last_lines(screen, 2), "c\nd");
        assert_eq!(last_lines(screen, 10), "a\nb\nc\nd");
        assert_eq!(last_lines("", 3), "");
    }
}
