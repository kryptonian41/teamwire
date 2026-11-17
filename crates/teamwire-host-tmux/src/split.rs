//! `Host::split_run` and `Host::close_pane`: `split-window` beside a pane, on
//! that pane's server, with the environment passed through a private file as
//! `spawn` passes it (ADR 0013), then `select-pane -T` for the label. The new
//! pane gets no `@teamwire_name` mark, so `list` and `live_agent_names` leave
//! it out.

use std::time::Duration;

use teamwire_core::{Host, HostError, HostErrorCode, Place, SplitRequest};

use crate::call::{self, Failure};
use crate::spawn::{env_wrapper, is_env_name};
use crate::{Tmux, keys, pane, share};

impl Tmux {
    pub(crate) fn split_beside(
        &self,
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
        if let Some((key, _)) = req.env.iter().find(|(k, _)| !is_env_name(k)) {
            return Err(HostError::new(
                HostErrorCode::BadArgument,
                format!("{key:?} is not an environment variable name"),
            ));
        }
        let old = Self::pane(beside)?;
        // The split, the title, and the kill after a failed title.
        let each = share(timeout, 3);
        let env_dir = if req.env.is_empty() {
            None
        } else {
            Some(self.write_env_file(&req.env)?)
        };
        let cwd = req.cwd.display().to_string();
        let mut args: Vec<String> = [
            "split-window",
            "-h",
            "-d",
            "-P",
            "-F",
            "#{pane_id} #{socket_path}",
            "-t",
            old.id,
            "-c",
            &cwd,
            "--",
        ]
        .iter()
        .map(|s| keys::escape_arg(s))
        .collect();
        if let Some(dir) = &env_dir {
            args.extend(env_wrapper(dir).iter().map(|a| keys::escape_arg(a)));
        }
        args.extend(req.argv.iter().map(|a| keys::escape_arg(a)));
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();

        let result = (|| {
            let printed = self.run_at(&old, &refs, each)?;
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
            let place = Place {
                host: self.id(),
                id: pane::place_id(id, socket),
            };
            let new = Self::pane(&place)?;
            if let Err(e) =
                self.run_at(&new, &["select-pane", "-t", new.id, "-T", &req.label], each)
            {
                // Nothing else knows this pane, so leaving it would leave the
                // program running in the user's window.
                drop(self.run_at(&new, &["kill-pane", "-t", new.id], each));
                return Err(e.into());
            }
            Ok(place)
        })();
        // The wrapper removes the folder once it has read it, so a leftover
        // folder means the program never started.
        if let (Err(_), Some(dir)) = (&result, &env_dir) {
            drop(std::fs::remove_dir_all(dir));
        }
        result
    }

    /// `kill-pane`. A pane, or a whole server, that is gone counts as closed.
    pub(crate) fn close_split(&self, place: &Place, timeout: Duration) -> Result<(), HostError> {
        let pane = Self::pane(place)?;
        match self.run_at(&pane, &["kill-pane", "-t", pane.id], timeout) {
            Ok(_) | Err(Failure::NoServer(_) | Failure::NoTarget(_)) => Ok(()),
            Err(f) => Err(f.into()),
        }
    }
}
