//! `harness_contract`: what every `Harness` must do.

use std::time::Duration;

use teamwire_core::{
    Harness, HarnessErrorCode, HostErrorCode, IdentityChannel, Liveness, MemberLaunch, OtelConfig,
    ScreenState, SessionId, is_secret_name, one_line,
};

use super::{fail, ok, require};

/// How a caller lets `harness_contract` reach one harness. The caller builds
/// the harness with its files and directories in a temporary directory.
pub struct HarnessSetup<'a> {
    pub harness: &'a dyn Harness,
    /// A member to build a start for. Its `identity` must hold at least one
    /// secret-named value, such as `TEAMWIRE_TOKEN`.
    pub member: MemberLaunch,
    /// One valid turn-end hook input, and the session it names.
    pub turn_end_stdin: Vec<u8>,
    pub turn_end_session: SessionId,
    /// Text the harness's screen shows at its input prompt, and while a
    /// dialog is open.
    pub prompt_screen: String,
    pub dialog_screen: String,
    /// A session the harness knows is running, if the caller can make one.
    pub live_session: Option<SessionId>,
    /// A session id no harness has ever heard of.
    pub unknown_session: SessionId,
    pub timeout: Duration,
}

impl std::fmt::Debug for HarnessSetup<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HarnessSetup")
            .field("harness", &self.harness.id())
            .field("member", &self.member)
            .finish_non_exhaustive()
    }
}

/// Runs every rule on one harness.
pub fn harness_contract(setup: &HarnessSetup<'_>) {
    let h = setup.harness;
    let t = setup.timeout;
    let caps = h.capabilities();

    require("the harness id is not empty", !h.id().as_str().is_empty());
    require(
        "capabilities() gives the same answer twice",
        h.capabilities() == caps,
    );

    launch_rules(setup);

    let end = ok("parse_turn_end", h.parse_turn_end(&setup.turn_end_stdin));
    require(
        "parse_turn_end names the session in the input",
        end.session == setup.turn_end_session,
    );
    for junk in [&b"not json"[..], b""] {
        match h.parse_turn_end(junk) {
            Err(e) if e.code == HarnessErrorCode::BadInput => {}
            other => fail("parse_turn_end of junk is bad_input", other),
        }
    }

    let note = "report the progress line first";
    let reply = String::from_utf8_lossy(&h.hold_reply(note)).into_owned();
    require("hold_reply carries the note", reply.contains(note));

    require(
        "the prompt screen is an input prompt",
        h.screen_state(&setup.prompt_screen) == ScreenState::InputPrompt,
    );
    require(
        "the dialog screen is a dialog",
        h.screen_state(&setup.dialog_screen) == ScreenState::Dialog,
    );
    require(
        "an empty screen is unknown",
        h.screen_state("") == ScreenState::Unknown,
    );

    if let Some(live) = &setup.live_session {
        require(
            "a running session is alive",
            matches!(ok("is_alive", h.is_alive(live, t)), Liveness::Alive(_)),
        );
        require(
            "a running session has a process",
            !ok("processes_for_session", h.processes_for_session(live, t)).is_empty(),
        );
    }
    match h.is_alive(&setup.unknown_session, t) {
        Ok(Liveness::Dead | Liveness::Unknown) => {}
        Err(e) if e.code == HarnessErrorCode::NotFound => {}
        other => fail("an unknown session is never alive", other),
    }
    match h.processes_for_session(&setup.unknown_session, t) {
        Ok(pids) => require("an unknown session has no process", pids.is_empty()),
        Err(e) if e.code == HarnessErrorCode::NotFound => {}
        other => fail(
            "processes_for_session of an unknown session is empty or not_found",
            other,
        ),
    }

    let base = OtelConfig {
        endpoint: Some("http://127.0.0.1:4318".to_owned()),
        headers: Vec::new(),
    };
    let vars = h.otel_env(&setup.member.team, &setup.member.role, &base);
    let mut names: Vec<&String> = vars.iter().map(|(k, _)| k).collect();
    names.sort();
    let before = names.len();
    names.dedup();
    require("otel_env names each variable once", names.len() == before);
    require(
        "otel_env names are not empty",
        vars.iter().all(|(k, _)| !k.is_empty()),
    );

    // Whatever the environment says, reading the caller's session must not panic.
    let _caller = h.caller_session(&|_| None);

    if !caps.can_push {
        let line = one_line("hello", 20);
        match h.push(&setup.member.session_id, &line, t) {
            Err(e) if e.code == HostErrorCode::Unsupported => {}
            other => fail("push without can_push is unsupported", other),
        }
    }
}

fn launch_rules(setup: &HarnessSetup<'_>) {
    let h = setup.harness;
    let m = &setup.member;
    let caps = h.capabilities();
    let spec = ok("launch_spec", h.launch_spec(m));

    require("launch_spec gives an argv", !spec.argv.is_empty());
    require(
        "launch_spec writes only under the member's directory",
        spec.files.iter().all(|f| f.path.starts_with(&m.member_dir)),
    );
    if caps.choose_session_id {
        require(
            "a harness that chooses session ids puts the id in argv",
            spec.argv.iter().any(|a| a.contains(m.session_id.as_str())),
        );
    }

    let secrets: Vec<&(String, String)> = m
        .identity
        .iter()
        .filter(|(k, _)| is_secret_name(k))
        .collect();
    require(
        "the setup's member has a secret identity value",
        !secrets.is_empty(),
    );
    match spec.identity_channel {
        IdentityChannel::HarnessConfig => {
            require(
                "a harness with session_env_via_config uses the config channel",
                caps.session_env_via_config,
            );
            for (k, v) in &m.identity {
                require(
                    &format!("identity {k} is in a launch file"),
                    spec.files.iter().any(|f| contains(&f.bytes, v)),
                );
            }
            for f in spec
                .files
                .iter()
                .filter(|f| secrets.iter().any(|(_, v)| contains(&f.bytes, v)))
            {
                require(
                    &format!(
                        "{} holds a secret, so it is private to its owner",
                        f.path.display()
                    ),
                    f.mode.trailing_zeros() >= 6,
                );
            }
        }
        IdentityChannel::HostEnv => {
            require(
                "a harness without session_env_via_config uses the host channel",
                !caps.session_env_via_config,
            );
            for pair in &m.identity {
                require(
                    &format!("identity {} is in spec.env", pair.0),
                    spec.env.contains(pair),
                );
            }
        }
    }
    no_secret_in_argv("launch_spec", &spec.argv, &secrets);
    let shown = format!("{spec:?}");
    for (k, v) in &secrets {
        require(
            &format!("Debug of a launch spec hides {k}"),
            !shown.contains(v.as_str()),
        );
    }

    let resumed = ok("resume_spec", h.resume_spec(m, &m.session_id));
    no_secret_in_argv("resume_spec", &resumed.argv, &secrets);
    require("resume_spec gives an argv", !resumed.argv.is_empty());
    require(
        "resume_spec names the session to resume",
        resumed
            .argv
            .iter()
            .any(|a| a.contains(m.session_id.as_str())),
    );
}

/// Any user on the machine can read a process's argv, so a secret never goes
/// there; it travels in a private file or in the host's environment.
fn no_secret_in_argv(call: &str, argv: &[String], secrets: &[&(String, String)]) {
    for (k, v) in secrets {
        require(
            &format!("{call} keeps the value of {k} out of argv"),
            !argv.iter().any(|a| a.contains(v.as_str())),
        );
    }
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|w| w == needle.as_bytes())
}
