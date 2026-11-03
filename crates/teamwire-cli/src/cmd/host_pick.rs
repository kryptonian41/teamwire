//! The lead's host, as `doctor` and wiring pick it: herdr or tmux from the
//! environment, and the nearest of them among this process's parents when
//! both are set. Free of doctor's types, so the doctor tests compile this
//! file as it is.

use teamwire_core::Pid;

/// The host teamwire starts members in: herdr inside a herdr pane
/// (`HERDR_ENV=1`), else tmux inside a tmux session (`TMUX` set), else none.
/// Only the tools of the host in use are required.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Host {
    Herdr,
    Tmux,
    None,
}

impl Host {
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Herdr => "herdr",
            Self::Tmux => "tmux",
            Self::None => "none",
        }
    }

    pub(crate) fn from_id(id: &str) -> Self {
        match id {
            "herdr" => Self::Herdr,
            "tmux" => Self::Tmux,
            _ => Self::None,
        }
    }

    pub(crate) fn detect(herdr_env: Option<&str>, tmux: Option<&str>) -> Self {
        if herdr_env == Some("1") {
            Self::Herdr
        } else if tmux.is_some_and(|t| !t.is_empty()) {
            Self::Tmux
        } else {
            Self::None
        }
    }
}

/// The most parents `nearest_host` looks at. A loop in the chain stops here.
pub(crate) const MAX_PARENT_STEPS: usize = 64;

/// The first of `start` and its parents, nearest first, whose program is
/// `tmux` or `herdr`. `parent_of` gives a pid's parent and its own program
/// name; a failed lookup ends the walk.
pub(crate) fn nearest_host(
    start: Pid,
    parent_of: &dyn Fn(Pid) -> Option<(Pid, String)>,
) -> Option<&'static str> {
    let mut pid = start;
    for _ in 0..MAX_PARENT_STEPS {
        let (parent, name) = parent_of(pid)?;
        // Linux names the tmux server "tmux: server".
        match name
            .split_whitespace()
            .next()
            .map(|n| n.trim_end_matches(':'))
        {
            Some("tmux") => return Some("tmux"),
            Some("herdr") => return Some("herdr"),
            _ => {}
        }
        pid = parent;
    }
    None
}

/// The lead's host and the reason for the pick, for `doctor`.
pub(crate) struct HostPick {
    pub(crate) id: &'static str,
    pub(crate) why: String,
}

/// `herdr` inside a herdr pane (`HERDR_ENV=1`), else `tmux` inside a tmux
/// client (`TMUX` set), otherwise `none`. A tmux server started from a herdr
/// pane copies `HERDR_ENV` into its windows, and herdr started inside tmux
/// carries `TMUX`, so when both are set the nearest `tmux` or `herdr`
/// among this process's parents decides; with none found, herdr wins.
pub(crate) fn pick_host(
    herdr_env: Option<&str>,
    tmux: Option<&str>,
    parent_of: &dyn Fn(Pid) -> Option<(Pid, String)>,
) -> HostPick {
    // The rule when the parents cannot tell: herdr first.
    let first = Host::detect(herdr_env, tmux).id();
    let both = herdr_env == Some("1") && tmux.is_some_and(|t| !t.is_empty());
    let (id, why) = match (both, first) {
        (false, "herdr") => (first, "HERDR_ENV is 1".to_owned()),
        (false, "tmux") => (first, "TMUX is set".to_owned()),
        (false, _) => (first, "neither HERDR_ENV nor TMUX is set".to_owned()),
        (true, _) => match nearest_host(Pid::new(std::process::id()), parent_of) {
            Some(id) => (
                id,
                format!("both HERDR_ENV and TMUX are set; the nearest parent is {id}"),
            ),
            None => (
                first,
                "both HERDR_ENV and TMUX are set; no parent is tmux or herdr, so herdr first"
                    .to_owned(),
            ),
        },
    };
    HostPick { id, why }
}
