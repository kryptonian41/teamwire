//! `Host::spawn`'s tmux calls: the `new-window` (or `new-session`) that
//! makes the pane, and the private file that carries the member's
//! environment instead of tmux's argv. Outside a tmux client each team's
//! windows go to its own detached session `tw_<team>`.

use std::fmt;
use std::sync::atomic::Ordering;
use std::time::Duration;

use teamwire_core::{HostError, HostErrorCode, LaunchSpec, SpawnRequest};

use crate::call::{self, Failure};
use crate::{BUFFER_COUNTER, Tmux, keys, pane};

/// The size of a session `spawn` creates. A detached session has no client
/// to size it, and Claude's dialogs read badly at 80 columns.
const SESSION_SIZE: [&str; 4] = ["-x", "200", "-y", "50"];

impl Tmux {
    /// The new pane's place id, and the environment folder its program has
    /// not yet read and removed (none without an environment).
    pub(crate) fn spawn_pane(
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
        let window_name = req.window_name();
        let mut shared: Vec<String> = [
            "-d",
            "-P",
            "-F",
            "#{pane_id} #{socket_path}",
            "-n",
            &window_name,
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
        // Inside a tmux client (`with_client`), a window with no target opens
        // in the client's session. Outside one, it goes to the team's own
        // detached session. The `=` makes tmux match that name exactly: a bare
        // name matches by prefix, which would put team `in` into `tw_instr`.
        let session_name = req.session_name();
        let target = format!("={session_name}:");
        let window = if self.socket_path.is_some() {
            with(&["new-window"])
        } else {
            with(&["new-window", "-t", &target])
        };
        let mut session = vec!["new-session", "-s", &session_name];
        session.extend(SESSION_SIZE);
        let session = with(&session);
        let run_owned = |args: &[String]| {
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            self.run(&refs, timeout)
        };

        let result = (|| {
            let mut out = run_owned(&window);
            // No server, or no session of the team yet.
            if matches!(out, Err(Failure::NoServer(_) | Failure::NoTarget(_))) {
                out = run_owned(&session);
                // A second spawn made the session between our two calls.
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
    pub(crate) fn write_env_file(
        &self,
        env: &[(String, String)],
    ) -> Result<std::path::PathBuf, HostError> {
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

/// A name `sh` accepts in `export NAME=...`.
pub(crate) fn is_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The words that load the environment file in `dir`, remove `dir`, and then
/// run the program that follows them.
pub(crate) fn env_wrapper(dir: &std::path::Path) -> Vec<String> {
    let dir = dir.display().to_string();
    vec![
        "sh".to_owned(),
        "-c".to_owned(),
        "env=\"$1/env\"; . \"$env\"; rm -rf \"$1\"; shift; exec \"$@\"".to_owned(),
        "teamwire-env".to_owned(),
        dir,
    ]
}
