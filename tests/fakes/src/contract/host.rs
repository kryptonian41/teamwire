//! `host_contract`: what every `Host` must do.

use std::thread::sleep;
use std::time::Duration;

use teamwire_core::{
    Address, Host, HostErrorCode, Key, Place, PlaceState, SpawnRequest, SplitRequest, one_line,
};

use super::{fail, ok, require};

/// How a caller lets `host_contract` make places on its host.
pub struct HostSetup<'a> {
    pub host: &'a dyn Host,
    /// Builds the request for one new place. The request must start a
    /// program that keeps running until the place is closed (a shell, `cat`,
    /// `fake-claude`); the suite supplies the name.
    pub request: &'a dyn Fn(&Address) -> SpawnRequest,
    /// The timeout passed to every host call.
    pub timeout: Duration,
    /// True when the program in the place echoes what is typed into it, so
    /// the suite may look for its text in `read_tail`.
    pub echoes_input: bool,
    /// True when `live_agent_names` lists a place this host started. A host
    /// with no notion of a named agent (tmux) sets this false.
    pub lists_agent_names: bool,
    /// True when a second `spawn` under a name a live place holds is refused
    /// with `BadArgument` and a detail starting `agent_name_taken:`. Every
    /// adapter does this; a host that cannot sets it false.
    pub refuses_live_names: bool,
    /// How many times to look at the screen, 50 ms apart, before giving up.
    pub screen_attempts: u32,
}

impl std::fmt::Debug for HostSetup<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostSetup")
            .field("host", &self.host.id())
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl<'a> HostSetup<'a> {
    pub fn new(host: &'a dyn Host, request: &'a dyn Fn(&Address) -> SpawnRequest) -> Self {
        Self {
            host,
            request,
            timeout: Duration::from_secs(10),
            echoes_input: false,
            lists_agent_names: true,
            refuses_live_names: true,
            screen_attempts: 100,
        }
    }
}

fn address(role: &str) -> Address {
    ok(
        "parsing the suite's own address",
        Address::parse(&format!("tw_contract_{role}")),
    )
}

/// Runs every rule on one host. Leaves no place behind.
pub fn host_contract(setup: &HostSetup<'_>) {
    let host = setup.host;
    let t = setup.timeout;

    require("the host id is not empty", !host.id().as_str().is_empty());
    require(
        "capabilities() gives the same answer twice",
        host.capabilities() == host.capabilities(),
    );
    if let Some(var) = host.place_env_var() {
        require(
            "place_env_var is a plain variable name",
            !var.is_empty() && !var.contains('='),
        );
    }

    let name = address("a1");
    let place = ok("spawn", host.spawn(&(setup.request)(&name), t));
    require(
        "a spawned place belongs to this host",
        place.host == host.id(),
    );

    if setup.refuses_live_names {
        match host.spawn(&(setup.request)(&name), t) {
            Err(e)
                if e.code == HostErrorCode::BadArgument
                    && e.detail.starts_with("agent_name_taken:") => {}
            other => fail(
                "a second spawn under a live name is bad_argument with agent_name_taken:",
                other,
            ),
        }
    }

    let listed = ok("list", host.list(t));
    require("list() holds a place just spawned", listed.contains(&place));
    if setup.lists_agent_names {
        let names = ok("live_agent_names", host.live_agent_names(t));
        require(
            "live_agent_names() holds the name just spawned",
            names.iter().any(|n| n == name.as_str()),
        );
    }

    let caps = host.capabilities();
    let state = ok("state", host.state(&place, t));
    if caps.reports_state {
        require(
            "a host that reports state gives idle, working or blocked for a live place",
            matches!(
                state,
                PlaceState::Idle | PlaceState::Working | PlaceState::Blocked
            ),
        );
    }
    ok("occupant", host.occupant(&place, t));
    ok("read_tail", host.read_tail(&place, 20, t));

    if caps.can_type {
        let ping = "contract ping 7";
        ok("submit", host.submit(&place, &one_line(ping, 80), t));
        ok(
            "send_keys",
            host.send_keys(&place, &[Key::Char('x'), Key::Escape], t),
        );
        if setup.echoes_input {
            require(
                "text submitted to an echoing program shows in read_tail",
                screen_shows(setup, &place, ping),
            );
        }
    }

    unknown_place_rules(setup);
    split_rules(setup, &place);

    match host.close(&place, &address("someone_else"), t) {
        Err(e) if e.code == HostErrorCode::NotFound => {}
        other => fail(
            "close of a place that holds another agent's name is not_found",
            other,
        ),
    }
    require(
        "close with another agent's name leaves the place",
        ok("list", host.list(t)).contains(&place),
    );

    ok("close", host.close(&place, &name, t));
    require(
        "list() no longer holds a closed place",
        !ok("list", host.list(t)).contains(&place),
    );
    match host.state(&place, t) {
        Ok(PlaceState::Gone) => {}
        Err(e) if e.code == HostErrorCode::NotFound => {}
        other => fail("state of a closed place is gone or not_found", other),
    }
    if setup.lists_agent_names {
        let names = ok("live_agent_names", host.live_agent_names(t));
        require(
            "live_agent_names() drops the name of a closed place",
            !names.iter().any(|n| n == name.as_str()),
        );
    }
}

fn unknown_place_rules(setup: &HostSetup<'_>) {
    let (host, t) = (setup.host, setup.timeout);
    let missing = Place {
        host: host.id(),
        id: "contract-no-such-place".to_owned(),
    };
    let not_found = |call: &str, code: Option<HostErrorCode>| {
        require(
            &format!("{call} on a place that does not exist is not_found"),
            code == Some(HostErrorCode::NotFound),
        );
    };
    not_found("occupant", host.occupant(&missing, t).err().map(|e| e.code));
    not_found(
        "read_tail",
        host.read_tail(&missing, 5, t).err().map(|e| e.code),
    );
    not_found(
        "close",
        host.close(&missing, &address("a1"), t)
            .err()
            .map(|e| e.code),
    );
    not_found(
        "submit",
        host.submit(&missing, &one_line("hello", 20), t)
            .err()
            .map(|e| e.code),
    );
    match host.state(&missing, t) {
        Ok(PlaceState::Gone) => {}
        other => not_found("state", other.err().map(|e| e.code)),
    }
}

/// `split_run` and `close_pane`, beside the live place `beside`. The split
/// runs the program of the setup's own request, so it keeps running until
/// its pane is closed.
fn split_rules(setup: &HostSetup<'_>, beside: &Place) {
    let (host, t) = (setup.host, setup.timeout);
    let base = (setup.request)(&address("s1"));
    let req = SplitRequest {
        label: "lead \u{b7} contract".to_owned(),
        cwd: base.cwd,
        argv: base.argv,
        env: base.env,
    };
    if !host.capabilities().splits_pane {
        match host.split_run(beside, &req, t) {
            Err(e) if e.code == HostErrorCode::Unsupported => {}
            other => fail(
                "split_run on a host that cannot split panes is unsupported",
                other,
            ),
        }
        return;
    }
    let missing = Place {
        host: host.id(),
        id: "contract-no-such-place".to_owned(),
    };
    require(
        "split_run beside a place that does not exist is not_found",
        host.split_run(&missing, &req, t).err().map(|e| e.code) == Some(HostErrorCode::NotFound),
    );
    let names = |what: &str| {
        let mut all = ok(what, host.live_agent_names(t));
        all.sort();
        all
    };
    let before = setup.lists_agent_names.then(|| names("live_agent_names"));
    let new = ok("split_run", host.split_run(beside, &req, t));
    require("a split pane belongs to this host", new.host == host.id());
    require("a split pane is a new place", new != *beside);
    require(
        "list() leaves out a split pane, which holds no agent name",
        !ok("list", host.list(t)).contains(&new),
    );
    if let Some(before) = before {
        let after = names("live_agent_names");
        if after != before {
            fail(
                "a split pane adds no name to live_agent_names()",
                (before, after),
            );
        }
    }
    ok("close_pane", host.close_pane(&new, t));
    ok(
        "close_pane of a pane that is already gone",
        host.close_pane(&new, t),
    );
    require(
        "close_pane leaves the place it split beside",
        ok("list", host.list(t)).contains(beside),
    );
}

fn screen_shows(setup: &HostSetup<'_>, place: &Place, text: &str) -> bool {
    for _ in 0..setup.screen_attempts {
        let tail = ok("read_tail", setup.host.read_tail(place, 50, setup.timeout));
        if tail.contains(text) {
            return true;
        }
        sleep(Duration::from_millis(50));
    }
    false
}
