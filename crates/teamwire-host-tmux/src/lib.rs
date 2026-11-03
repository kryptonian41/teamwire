//! The `Host` trait for tmux: `new-window` to spawn, the member's environment
//! passed through a private file and never through argv, `load-buffer` plus
//! `paste-buffer -p` plus `send-keys Enter` to type, `capture-pane` for the screen, the pane's process tree for its
//! occupant.
//!
//! Never: type into a pane before the screen check of ADR 0006 point 3a
//! (tmux does not guard dialogs); run tmux except through `Platform::run`
//! with a timeout.
//!
//! Invariants: every call after `spawn` names its pane by its `%` id, never a
//! session or window name; a place is `%<n>@<socket path>` and its calls go to
//! that server; `list` and `live_agent_names` report only panes marked with
//! `@teamwire_name`; a pane whose program exited holds no live name, and
//! `spawn` refuses a live one; each method splits the caller's timeout.

mod call;
mod keys;
mod pane;
mod tree;

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use teamwire_core::{
    Address, Host, HostCaps, HostError, HostErrorCode, HostId, Key, LaunchSpec, Occupant, OneLine,
    Pid, Place, PlaceState, Platform, SpawnRequest, SubmitAck,
};

use call::{Failure, Socket, call};
use pane::Pane;

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
/// A place is a pane: its `%<n>` id, which is also the value of `TMUX_PANE`
/// inside the pane, and the socket path of its server (`pane` module).
/// Errors: a pane, or the whole server, that is
/// not there is `NotFound`; a timeout is `Timeout`; any other tmux failure
/// is `Other` with tmux's message in the detail.
pub struct Tmux {
    platform: Arc<dyn Platform>,
    /// `-L <name>`, for a test's private server.
    socket_name: Option<String>,
    /// `-S <path>`, the server the calling tmux client belongs to.
    socket_path: Option<String>,
}

impl Tmux {
    pub const ID: &'static str = "tmux";

    /// tmux's commands run through `platform`, on the server the process
    /// inherits (`TMUX`, or the default socket).
    pub fn new(platform: Arc<dyn Platform>) -> Self {
        Self {
            platform,
            socket_name: None,
            socket_path: None,
        }
    }

    /// Runs every tmux command on the named socket (`tmux -L <name>`), so a
    /// test never touches the user's server.
    #[must_use]
    pub fn with_socket(mut self, name: impl Into<String>) -> Self {
        self.socket_name = Some(name.into());
        self
    }

    /// Runs commands for a bare pane id on the server the `TMUX` value names
    /// (its first comma field is the socket path), and records that server in
    /// every place `spawn` and `list` return. A value with no path changes
    /// nothing.
    #[must_use]
    pub fn with_client(mut self, tmux_env: &str) -> Self {
        self.socket_path = pane::socket_of_client(tmux_env).map(str::to_owned);
        self
    }

    /// The server a call goes to when its place names none.
    fn own_server(&self) -> Option<Socket<'_>> {
        self.socket_path
            .as_deref()
            .map(Socket::Path)
            .or_else(|| self.socket_name.as_deref().map(Socket::Name))
    }

    fn run(&self, args: &[&str], timeout: Duration) -> Result<String, Failure> {
        call(
            self.platform.as_ref(),
            self.own_server(),
            args,
            None,
            timeout,
        )
    }

    /// Runs tmux on the server that holds `pane`.
    fn run_at(&self, pane: &Pane<'_>, args: &[&str], timeout: Duration) -> Result<String, Failure> {
        call(
            self.platform.as_ref(),
            pane.server().or_else(|| self.own_server()),
            args,
            None,
            timeout,
        )
    }

    /// The pane of `place`, or `NotFound` when the id is not a pane id.
    fn pane(place: &Place) -> Result<Pane<'_>, HostError> {
        Pane::parse(&place.id).ok_or_else(|| {
            HostError::new(
                HostErrorCode::NotFound,
                format!("{} is not a tmux pane id", place.id),
            )
        })
    }

    /// The last `lines` screen lines of `place`, with `extra` capture flags.
    fn capture(
        &self,
        place: &Place,
        lines: u16,
        extra: &[&str],
        timeout: Duration,
    ) -> Result<String, HostError> {
        let pane = Self::pane(place)?;
        let from = format!("-{lines}");
        let mut args = vec!["capture-pane", "-p", "-J"];
        args.extend_from_slice(extra);
        args.extend(["-t", pane.id, "-S", &from]);
        let text = self.run_at(&pane, &args, timeout)?;
        Ok(last_lines(&text, usize::from(lines)))
    }

    /// `(place id, member name, program has exited)` for every pane this
    /// adapter started.
    fn marked_panes(&self, timeout: Duration) -> Result<Vec<(String, String, bool)>, HostError> {
        let format = format!("#{{pane_id}}\t#{{socket_path}}\t#{{pane_dead}}\t#{{{NAME_OPTION}}}");
        let listing = match self.run(&["list-panes", "-a", "-F", &format], timeout) {
            Ok(text) => text,
            Err(Failure::NoServer(_)) => return Ok(Vec::new()),
            Err(f) => return Err(f.into()),
        };
        Ok(listing
            .lines()
            .filter_map(|line| {
                let mut fields = line.splitn(4, '\t');
                Some((
                    fields.next()?,
                    fields.next()?,
                    fields.next()? == "1",
                    fields.next()?.trim(),
                ))
            })
            .filter(|(_, _, _, name)| !name.is_empty())
            .map(|(id, socket, dead, name)| (pane::place_id(id, socket), name.to_owned(), dead))
            .collect())
    }

    /// The new pane's place id, and the environment folder its program has
    /// not yet read and removed (none without an environment).
    fn spawn_pane(
        &self,
        req: &SpawnRequest,
        timeout: Duration,
    ) -> Result<(String, Option<std::path::PathBuf>), HostError> {
        if req.argv.is_empty() {
            return Err(HostError::new(
                HostErrorCode::BadArgument,
                "spawn needs a program to run",
            ));
        }
        let cwd = req.cwd.display().to_string();
        let mut shared: Vec<String> = [
            "-d",
            "-P",
            "-F",
            "#{pane_id} #{socket_path}",
            "-n",
            &req.title,
            "-c",
            &cwd,
        ]
        .iter()
        .map(|s| keys::escape_arg(s))
        .collect();
        for (key, _) in &req.env {
            if !is_env_name(key) {
                return Err(HostError::new(
                    HostErrorCode::BadArgument,
                    format!("{key:?} is not an environment variable name"),
                ));
            }
        }
        // `new-window -e` would put the values in tmux's argv, which any local
        // user can read with `ps`. They go through a private file instead.
        let env_dir = if req.env.is_empty() {
            None
        } else {
            Some(self.write_env_file(&req.env)?)
        };
        shared.push("--".to_owned());
        match &env_dir {
            None => shared.extend(req.argv.iter().map(|a| keys::escape_arg(a))),
            Some(dir) => {
                shared.extend(env_wrapper(dir).iter().map(|a| keys::escape_arg(a)));
                shared.extend(req.argv.iter().map(|a| keys::escape_arg(a)));
            }
        }

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

        let result = (|| {
            let mut out = run_owned(&window);
            if matches!(out, Err(Failure::NoServer(_))) {
                out = run_owned(&session);
                // A second spawn started the server between our two calls.
                if matches!(&out, Err(Failure::Host(e)) if e.detail.contains("duplicate session")) {
                    out = run_owned(&window);
                }
            }
            let printed = out?;
            let (id, socket) = printed
                .trim()
                .split_once(' ')
                .unwrap_or((printed.trim(), ""));
            if !id.starts_with('%') {
                return Err(HostError::new(
                    HostErrorCode::Other,
                    format!("tmux printed no pane id: {}", call::clip(&printed)),
                ));
            }
            Ok(pane::place_id(id, socket))
        })();
        // The wrapper removes the directory once it has read it, so a leftover
        // directory means the program never started.
        if let (Err(_), Some(dir)) = (&result, &env_dir) {
            drop(std::fs::remove_dir_all(dir));
        }
        Ok((result?, env_dir))
    }

    /// Writes `export KEY='value'` lines to a file in a new directory only the
    /// current user can enter, and returns the directory.
    fn write_env_file(&self, env: &[(String, String)]) -> Result<std::path::PathBuf, HostError> {
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        let io = |what: &str, e: &dyn fmt::Display| {
            HostError::new(HostErrorCode::Other, format!("{what}: {e}"))
        };
        let dir = std::env::temp_dir().join(format!(
            "teamwire-tmux-env-{}-{}",
            std::process::id(),
            BUFFER_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&dir)
            .map_err(|e| io("cannot make the environment directory", &e))?;
        let mut text = String::new();
        for (k, v) in env {
            text.push_str("export ");
            text.push_str(k);
            text.push('=');
            text.push_str(&LaunchSpec::shell_word(v));
            text.push('\n');
        }
        // `write_atomic` keeps the mode of the file it replaces and gives a new
        // file 0644. Make the file first at 0600 so the values are private.
        let file = dir.join("env");
        #[expect(
            clippy::disallowed_methods,
            reason = "creates the file at 0600 before write_atomic fills it"
        )]
        let written = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&file)
            .map_err(|e| io("cannot make the environment file", &e))
            .and_then(|_| {
                self.platform
                    .write_atomic(&file, text.as_bytes())
                    .map_err(|e| io("cannot write the environment file", &e))
            });
        if let Err(e) = written {
            drop(std::fs::remove_dir_all(&dir));
            return Err(e);
        }
        Ok(dir)
    }
}

/// One share of `timeout` for each of `calls` tmux calls that may run one
/// after the other, so their sum never passes the caller's `timeout`.
fn share(timeout: Duration, calls: usize) -> Duration {
    timeout / u32::try_from(calls.max(1)).unwrap_or(u32::MAX)
}

/// A name `sh` accepts in `export NAME=...`.
fn is_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The words that load the environment file in `dir`, remove `dir`, and then
/// run the program that follows them.
fn env_wrapper(dir: &std::path::Path) -> Vec<String> {
    let dir = dir.display().to_string();
    vec![
        "sh".to_owned(),
        "-c".to_owned(),
        "env=\"$1/env\"; . \"$env\"; rm -rf \"$1\"; shift; exec \"$@\"".to_owned(),
        "teamwire-env".to_owned(),
        dir,
    ]
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
        // The calls in a spawn: the name check, up to three to make the pane,
        // the mark, and the kill after a failed mark.
        let each = share(timeout, 6);
        // shortcut: check then act, so two spawns of one name at the same
        // instant could both pass; launch spawns one member at a time.
        if self
            .live_agent_names(each)?
            .iter()
            .any(|n| n == req.name.as_str())
        {
            return Err(HostError::new(
                HostErrorCode::BadArgument,
                format!(
                    "agent_name_taken: an agent named {} is already live",
                    req.name
                ),
            ));
        }
        let (id, env_dir) = self.spawn_pane(req, each)?;
        let place = Place {
            host: self.id(),
            id,
        };
        // A pane that cannot be marked would never be listed or cleaned up.
        let pane = Self::pane(&place)?;
        if let Err(e) = self.run_at(
            &pane,
            &[
                "set-option",
                "-p",
                "-t",
                pane.id,
                NAME_OPTION,
                req.name.as_str(),
            ],
            each,
        ) {
            drop(self.run_at(&pane, &["kill-pane", "-t", pane.id], each));
            // The killed program may never have read the environment folder.
            if let Some(dir) = &env_dir {
                drop(std::fs::remove_dir_all(dir));
            }
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
        // Load, paste and then either the delete after a failed paste or the
        // Enter: three calls at most.
        let each = share(timeout, 3);
        // The buffer lives on the pane's server, so load, paste and delete
        // all go there.
        call(
            self.platform.as_ref(),
            pane.server().or_else(|| self.own_server()),
            &["load-buffer", "-b", &buffer, "-"],
            Some(text.as_str().as_bytes()),
            each,
        )?;
        // `-p` sends the text as a bracketed paste where the program asked
        // for one; `-d` drops the buffer afterwards.
        if let Err(e) = self.run_at(
            &pane,
            &["paste-buffer", "-p", "-d", "-b", &buffer, "-t", pane.id],
            each,
        ) {
            drop(self.run_at(&pane, &["delete-buffer", "-b", &buffer], each));
            return Err(e.into());
        }
        std::thread::sleep(PASTE_SETTLE);
        self.run_at(&pane, &["send-keys", "-t", pane.id, "Enter"], each)?;
        Ok(())
    }

    fn state(&self, place: &Place, timeout: Duration) -> Result<PlaceState, HostError> {
        let Ok(pane) = Self::pane(place) else {
            return Ok(PlaceState::Gone);
        };
        match self.run_at(
            &pane,
            &["display-message", "-p", "-t", pane.id, "#{pane_dead}"],
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
        // Two commands run one after the other (tmux, then `ps`), so each
        // gets half: the whole call stays inside the caller's `timeout`.
        let each = timeout / 2;
        let text = self.run_at(
            &pane,
            &["display-message", "-p", "-t", pane.id, "#{pane_pid}"],
            each,
        )?;
        // tmux 3.x prints nothing and exits 0 for a pane that is not there.
        let Ok(root) = text.trim().parse::<u32>() else {
            return Err(HostError::new(
                HostErrorCode::NotFound,
                format!("tmux has no pane {}", place.id),
            ));
        };
        let pids: Vec<Pid> = tree::tree_of(self.platform.as_ref(), Pid::new(root), each)?;
        Ok(Occupant {
            session: None,
            pids,
        })
    }

    fn read_tail(&self, place: &Place, lines: u16, timeout: Duration) -> Result<String, HostError> {
        self.capture(place, lines, &[], timeout)
    }

    fn read_tail_styled(
        &self,
        place: &Place,
        lines: u16,
        timeout: Duration,
    ) -> Result<String, HostError> {
        // `-e` keeps the SGR codes, so dim placeholder text can be told apart.
        self.capture(place, lines, &["-e"], timeout)
    }

    fn send_keys(&self, place: &Place, keys: &[Key], timeout: Duration) -> Result<(), HostError> {
        let pane = Self::pane(place)?;
        let each = share(timeout, keys.len());
        for key in keys {
            let tail = keys::send_args(*key);
            let mut args = vec!["send-keys", "-t", pane.id];
            args.extend(tail.iter().map(String::as_str));
            self.run_at(&pane, &args, each)?;
        }
        Ok(())
    }

    fn close(&self, place: &Place, expected: &Address, timeout: Duration) -> Result<(), HostError> {
        let pane = Self::pane(place)?;
        // After a tmux server restart the pane ids start again at `%0`, so a
        // recorded id can name another member's pane. Only the pane this
        // adapter marked with the expected name is killed.
        let format = format!("#{{{NAME_OPTION}}}");
        // Reading the mark and killing the pane: two calls.
        let each = share(timeout, 2);
        let mark = self.run_at(
            &pane,
            &["display-message", "-p", "-t", pane.id, &format],
            each,
        )?;
        if mark.trim() != expected.as_str() {
            return Err(HostError::new(
                HostErrorCode::NotFound,
                format!("{} is not {expected}'s pane", pane.id),
            ));
        }
        self.run_at(&pane, &["kill-pane", "-t", pane.id], each)?;
        Ok(())
    }

    fn list(&self, timeout: Duration) -> Result<Vec<Place>, HostError> {
        let host = self.id();
        Ok(self
            .marked_panes(timeout)?
            .into_iter()
            .map(|(id, _, _)| Place {
                host: host.clone(),
                id,
            })
            .collect())
    }

    fn live_agent_names(&self, timeout: Duration) -> Result<Vec<String>, HostError> {
        Ok(self
            .marked_panes(timeout)?
            .into_iter()
            .filter(|(_, _, dead)| !dead)
            .map(|(_, name, _)| name)
            .collect())
    }

    fn place_env_var(&self) -> Option<&'static str> {
        Some("TMUX_PANE")
    }

    /// `TMUX_PANE` alone names the pane on the default server. The server
    /// comes from `TMUX`, so a shell outside tmux still reaches it.
    fn place_from_env(&self, read: &dyn Fn(&str) -> Option<String>) -> Option<String> {
        let pane = read("TMUX_PANE").filter(|p| !p.is_empty())?;
        let socket = read("TMUX").and_then(|t| pane::socket_of_client(&t).map(str::to_owned));
        Some(pane::place_id(&pane, socket.as_deref().unwrap_or_default()))
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
    use teamwire_core::{Key, RunOutput};
    use teamwire_fakes::FakePlatform;

    fn ok(stdout: &str) -> RunOutput {
        RunOutput {
            exit_code: 0,
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
            timed_out: false,
        }
    }

    #[test]
    fn occupant_keeps_both_commands_inside_the_callers_timeout() {
        let platform = Arc::new(FakePlatform::new());
        platform.queue_run(Ok(ok("10\n")));
        platform.queue_run(Ok(ok("10 1\n11 10\n")));
        let tmux = Tmux::new(platform.clone());
        let place = Place {
            host: tmux.id(),
            id: "%1".to_owned(),
        };
        let got = tmux.occupant(&place, Duration::from_secs(8)).unwrap();
        assert_eq!(got.pids.len(), 2);
        let runs = platform.runs();
        let total: Duration = runs.iter().map(|(_, t)| *t).sum();
        assert_eq!(runs.len(), 2, "one tmux call and one process listing");
        assert!(total <= Duration::from_secs(8), "{runs:?}");
    }

    fn place_in(tmux: &Tmux) -> Place {
        Place {
            host: tmux.id(),
            id: "%1".to_owned(),
        }
    }

    /// The sum of the timeouts of every command the platform ran.
    fn total_timeout(platform: &FakePlatform) -> Duration {
        platform.runs().iter().map(|(_, t)| *t).sum()
    }

    #[test]
    fn submit_keeps_its_three_calls_inside_the_callers_timeout() {
        let platform = Arc::new(FakePlatform::new());
        for _ in 0..3 {
            platform.queue_run(Ok(ok("")));
        }
        let tmux = Tmux::new(platform.clone());
        let text = teamwire_core::one_line("hello", 20);
        tmux.submit(&place_in(&tmux), &text, Duration::from_secs(5))
            .unwrap();
        assert_eq!(platform.runs().len(), 3, "{:?}", platform.runs());
        assert!(total_timeout(&platform) <= Duration::from_secs(5));
    }

    #[test]
    fn send_keys_keeps_all_its_keys_inside_the_callers_timeout() {
        let platform = Arc::new(FakePlatform::new());
        for _ in 0..3 {
            platform.queue_run(Ok(ok("")));
        }
        let tmux = Tmux::new(platform.clone());
        let keys = [Key::Char('x'), Key::Escape, Key::Enter];
        tmux.send_keys(&place_in(&tmux), &keys, Duration::from_secs(5))
            .unwrap();
        assert_eq!(platform.runs().len(), 3, "{:?}", platform.runs());
        assert!(total_timeout(&platform) <= Duration::from_secs(5));
    }

    #[test]
    fn close_keeps_its_two_calls_inside_the_callers_timeout() {
        let platform = Arc::new(FakePlatform::new());
        platform.queue_run(Ok(ok("tw_demo_api\n")));
        platform.queue_run(Ok(ok("")));
        let tmux = Tmux::new(platform.clone());
        let mine = Address::parse("tw_demo_api").unwrap();
        tmux.close(&place_in(&tmux), &mine, Duration::from_secs(5))
            .unwrap();
        assert_eq!(platform.runs().len(), 2, "{:?}", platform.runs());
        assert!(total_timeout(&platform) <= Duration::from_secs(5));
    }

    fn spawn_request(env: Vec<(String, String)>) -> SpawnRequest {
        SpawnRequest {
            name: Address::parse("tw_demo_api").unwrap(),
            title: "api".to_owned(),
            cwd: std::env::temp_dir(),
            argv: vec!["cat".to_owned()],
            env,
        }
    }

    #[test]
    fn spawn_keeps_its_calls_inside_the_callers_timeout_even_when_it_starts_the_server() {
        let platform = Arc::new(FakePlatform::new());
        // The longest path: no server for the name check, none for the first
        // `new-window`, a new session, then the mark.
        let no_server = |msg: &str| {
            Ok(RunOutput {
                exit_code: 1,
                stdout: Vec::new(),
                stderr: msg.as_bytes().to_vec(),
                timed_out: false,
            })
        };
        platform.queue_run(no_server("no server running on /tmp/tmux-1/default"));
        platform.queue_run(no_server("no server running on /tmp/tmux-1/default"));
        platform.queue_run(Ok(ok("%1 /tmp/tmux-1/default\n")));
        platform.queue_run(Ok(ok("")));
        let tmux = Tmux::new(platform.clone());
        tmux.spawn(&spawn_request(Vec::new()), Duration::from_secs(6))
            .unwrap();
        assert_eq!(platform.runs().len(), 4, "{:?}", platform.runs());
        assert!(total_timeout(&platform) <= Duration::from_secs(6));
    }

    #[test]
    fn a_failed_mark_removes_the_environment_folder() {
        let platform = Arc::new(FakePlatform::new());
        platform.queue_run(Ok(ok("")));
        platform.queue_run(Ok(ok("%1 /tmp/tmux-1/default\n")));
        // The mark fails, then the pane is killed.
        platform.queue_run(Ok(RunOutput {
            exit_code: 1,
            stdout: Vec::new(),
            stderr: b"boom".to_vec(),
            timed_out: false,
        }));
        platform.queue_run(Ok(ok("")));
        let tmux = Tmux::new(platform.clone());
        let req = spawn_request(vec![("SECRET".to_owned(), "x".to_owned())]);
        tmux.spawn(&req, Duration::from_secs(6)).unwrap_err();
        // The wrapper in the pane's argv names the folder it would have read.
        let runs = platform.runs();
        let dir = runs
            .iter()
            .flat_map(|(c, _)| c.args.iter())
            .find(|a| a.contains("teamwire-tmux-env-"))
            .map(|a| a.trim_matches('\'').to_owned())
            .unwrap();
        assert!(!std::path::Path::new(&dir).exists(), "{dir} was left");
    }

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |var| {
            pairs
                .iter()
                .find(|(k, _)| *k == var)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    #[test]
    fn a_process_names_its_pane_with_the_server_from_its_tmux_value() {
        let tmux = Tmux::new(Arc::new(FakePlatform::new()));
        let read = env(&[
            ("TMUX_PANE", "%4"),
            ("TMUX", "/private/tmp/tmux-501/tw,95189,0"),
        ]);
        assert_eq!(
            tmux.place_from_env(&read).as_deref(),
            Some("%4@/private/tmp/tmux-501/tw")
        );
        // Without `TMUX` the pane is on the default server, as before.
        assert_eq!(
            tmux.place_from_env(&env(&[("TMUX_PANE", "%4")])).as_deref(),
            Some("%4")
        );
        assert_eq!(tmux.place_from_env(&env(&[("TMUX", "/x,1,0")])), None);
    }

    #[test]
    fn close_leaves_a_pane_marked_with_another_name() {
        let platform = Arc::new(FakePlatform::new());
        platform.queue_run(Ok(ok("tw_other_web\n")));
        let tmux = Tmux::new(platform.clone());
        let place = Place {
            host: tmux.id(),
            id: "%0".to_owned(),
        };
        let mine = Address::parse("tw_demo_api").unwrap();
        let err = tmux
            .close(&place, &mine, Duration::from_secs(5))
            .unwrap_err();
        assert_eq!(err.code, HostErrorCode::NotFound);
        let runs = platform.runs();
        assert_eq!(runs.len(), 1, "only the mark was read: {runs:?}");
        assert!(
            !runs
                .iter()
                .any(|(c, _)| c.args.iter().any(|a| a == "kill-pane")),
            "{runs:?}"
        );
    }

    #[test]
    fn last_lines_drops_padding_then_takes_the_end() {
        let screen = "a\nb\nc\nd\n\n\n\n";
        assert_eq!(last_lines(screen, 2), "c\nd");
        assert_eq!(last_lines(screen, 10), "a\nb\nc\nd");
        assert_eq!(last_lines("", 3), "");
    }
}
