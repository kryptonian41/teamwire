//! A place as a pane id plus, when known, the tmux server that holds it.
//!
//! A new place is written `%<n>@<socket path>`, so a command run outside the
//! lead's tmux (a shell with no `TMUX`) still reaches the team's server. A
//! bare `%<n>` is what older teams stored and what `TMUX_PANE` holds: it goes
//! to the adapter's own server.

use crate::call::Socket;

/// The two parts of a place id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pane<'a> {
    /// `%<n>`.
    pub(crate) id: &'a str,
    /// The socket path the place was recorded with.
    pub(crate) socket: Option<&'a str>,
}

impl<'a> Pane<'a> {
    /// Splits a place id. `None` when it does not start with a `%` pane id:
    /// tmux would also resolve a session or window name, and a place must
    /// never be one.
    pub(crate) fn parse(place_id: &'a str) -> Option<Self> {
        let (id, socket) = match place_id.split_once('@') {
            Some((id, path)) => (id, Some(path).filter(|p| !p.is_empty())),
            None => (place_id, None),
        };
        (id.starts_with('%') && id.len() > 1).then_some(Self { id, socket })
    }

    /// The server this pane names, if its place recorded one.
    pub(crate) fn server(&self) -> Option<Socket<'a>> {
        self.socket.map(Socket::Path)
    }
}

/// The place id for pane `id` on the server at `socket_path`. A server that
/// reports no path leaves the bare pane id.
pub(crate) fn place_id(id: &str, socket_path: &str) -> String {
    let path = socket_path.trim();
    if path.is_empty() {
        id.to_owned()
    } else {
        format!("{id}@{path}")
    }
}

/// The server a `TMUX` value names: its first comma field, the socket path.
pub(crate) fn socket_of_client(tmux_env: &str) -> Option<&str> {
    tmux_env.split(',').next().filter(|p| !p.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_pane_id_has_no_server() {
        let pane = Pane::parse("%12").unwrap();
        assert_eq!((pane.id, pane.socket), ("%12", None));
    }

    #[test]
    fn a_recorded_server_travels_with_the_pane() {
        let pane = Pane::parse("%3@/tmp/tmux-501/tw team").unwrap();
        assert_eq!(
            (pane.id, pane.socket),
            ("%3", Some("/tmp/tmux-501/tw team"))
        );
        assert_eq!(
            place_id("%3", "/tmp/tmux-501/tw team\n"),
            "%3@/tmp/tmux-501/tw team"
        );
    }

    #[test]
    fn a_name_is_not_a_pane() {
        assert!(Pane::parse("main").is_none());
        assert!(Pane::parse("%").is_none());
        assert!(Pane::parse("@/tmp/x").is_none());
    }

    #[test]
    fn no_path_leaves_the_bare_id() {
        assert_eq!(place_id("%3", " \n"), "%3");
    }

    #[test]
    fn the_client_value_names_its_server() {
        assert_eq!(
            socket_of_client("/private/tmp/tmux-501/twsmoke,95189,0"),
            Some("/private/tmp/tmux-501/twsmoke")
        );
        assert_eq!(socket_of_client(""), None);
    }
}
