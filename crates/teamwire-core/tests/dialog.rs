//! The permission-dialog types, the `Harness` defaults for a harness with no
//! permission hook, and the text the lead reads (ADR 0052).

use std::time::Duration;

use teamwire_core::{
    CallerSession, DENIED_MARKER, DialogKind, DialogState, DialogVerdict, Harness, HarnessCaps,
    HarnessError, HarnessErrorCode, HarnessId, Host, LaunchSpec, Liveness, MemberLaunch,
    OtelConfig, PermissionAnswer, PermissionDecision, PermissionRequest, Pid, Place, Role,
    ScreenState, SessionId, StartupOutcome, TeamName, Timestamp, TrustPolicy, TurnEnd, Ulid,
    clock_time, decision_for, shown,
};

/// A harness that overrides none of the dialog methods.
#[derive(Debug)]
struct Plain;

impl Harness for Plain {
    fn id(&self) -> HarnessId {
        HarnessId::from_static("plain")
    }
    fn capabilities(&self) -> HarnessCaps {
        HarnessCaps {
            turn_end_hook: false,
            in_turn_hook: false,
            mcp_client: false,
            choose_session_id: false,
            can_push: false,
            session_env_via_config: false,
            reads_watch_lines: true,
            watch_start: teamwire_core::harness::WatchStart::MonitoredCommand,
            load_skill: "",
            reads_input_box: false,
            runs_afk: false,
        }
    }
    fn launch_spec(&self, _m: &MemberLaunch) -> Result<LaunchSpec, HarnessError> {
        Err(HarnessError::unsupported())
    }
    fn resume_spec(&self, _m: &MemberLaunch, _s: &SessionId) -> Result<LaunchSpec, HarnessError> {
        Err(HarnessError::unsupported())
    }
    fn caller_session(&self, _get: &dyn Fn(&str) -> Option<String>) -> Option<CallerSession> {
        None
    }
    fn processes_for_session(
        &self,
        _s: &SessionId,
        _t: Duration,
    ) -> Result<Vec<Pid>, HarnessError> {
        Ok(Vec::new())
    }
    fn is_alive(&self, _s: &SessionId, _t: Duration) -> Result<Liveness, HarnessError> {
        Ok(Liveness::Unknown)
    }
    fn parse_turn_end(&self, _stdin: &[u8]) -> Result<TurnEnd, HarnessError> {
        Err(HarnessError::unsupported())
    }
    fn hold_reply(&self, _note: &str) -> Vec<u8> {
        Vec::new()
    }
    fn screen_state(&self, _tail: &str) -> ScreenState {
        ScreenState::Unknown
    }
    fn answer_startup(
        &self,
        _host: &dyn Host,
        _place: &Place,
        _policy: TrustPolicy,
    ) -> Result<StartupOutcome, HarnessError> {
        Ok(StartupOutcome::NoDialog)
    }
    fn otel_env(&self, _t: &TeamName, _r: &Role, _b: &OtelConfig) -> Vec<(String, String)> {
        Vec::new()
    }
}

#[expect(clippy::unwrap_used, reason = "a failure fails the test")]
fn request(summary: &str) -> PermissionRequest {
    PermissionRequest {
        session: SessionId::parse("f07ea8cf-a9c1-41d3-9162-d68dbdefef83").unwrap(),
        subagent: false,
        tool: "Bash".to_owned(),
        summary: summary.to_owned(),
        input: Some(serde_json::json!({"command": summary})),
        why_not: None,
    }
}

#[test]
fn a_harness_without_a_permission_hook_cannot_tell_and_cannot_answer() {
    let h = Plain;
    assert_eq!(
        h.dialog_kind("Do you want to proceed?"),
        DialogKind::CannotTell
    );
    assert_eq!(h.dialog_kind(""), DialogKind::CannotTell);
    let parsed = h.parse_permission_request(br#"{"tool_name":"Bash"}"#);
    assert_eq!(parsed.unwrap_err().code, HarnessErrorCode::Unsupported);
    for verdict in DialogVerdict::ALL {
        let decision = PermissionDecision {
            verdict: *verdict,
            message: None,
        };
        let reply = h.permission_reply(&decision);
        assert_eq!(reply.unwrap_err().code, HarnessErrorCode::Unsupported);
    }
}

#[test]
fn dialog_strings_round_trip() {
    let kinds: Vec<&str> = DialogKind::ALL.iter().map(|k| k.as_str()).collect();
    assert_eq!(
        kinds,
        [
            "none",
            "permission",
            "trust",
            "login",
            "update",
            "unknown",
            "cannot_tell"
        ]
    );
    let states: Vec<&str> = DialogState::ALL.iter().map(|k| k.as_str()).collect();
    assert_eq!(
        states,
        ["pending", "answered", "taken", "expired", "unanswerable"]
    );
    for k in DialogKind::ALL {
        assert_eq!(DialogKind::parse(k.as_str()), Ok(*k));
    }
    for s in DialogState::ALL {
        assert_eq!(DialogState::parse(s.as_str()), Ok(*s));
    }
    for v in DialogVerdict::ALL {
        assert_eq!(DialogVerdict::parse(v.as_str()), Ok(*v));
    }
    let answer = PermissionAnswer {
        verdict: DialogVerdict::Deny,
        reason: "no".to_owned(),
    };
    let json = serde_json::to_string(&answer).unwrap();
    assert_eq!(json, r#"{"verdict":"deny","reason":"no"}"#);
    let req = request("rmdir /usr");
    let back: PermissionRequest =
        serde_json::from_str(&serde_json::to_string(&req).unwrap()).unwrap();
    assert_eq!(back, req);
    assert!(req.answerable());
}

#[test]
fn shown_escapes_control_and_bidi_characters() {
    assert_eq!(shown("rm -rf a\nb\tc"), "rm -rf a\nb\tc");
    assert_eq!(shown("a\u{1b}[2Jb"), "a\\u{1b}[2Jb");
    assert_eq!(shown("x\u{202e}y\u{200b}z"), "x\\u{202e}y\\u{200b}z");
    assert_eq!(shown("bell\u{7}\r"), "bell\\u{7}\\u{d}");
    assert_eq!(shown("del\u{7f}"), "del\\u{7f}");
    assert_eq!(shown("tag\u{e0041}"), "tag\\u{e0041}");
    assert_eq!(shown("plain é ✓"), "plain é ✓");
}

#[test]
fn a_denial_quotes_the_call_and_the_reason() {
    let id = Ulid::from_parts(Timestamp::from_unix_millis(1_700_000_000_000).unwrap(), 7);
    let req = request("rmdir /usr");
    let deny = decision_for(
        &id,
        &req,
        &PermissionAnswer {
            verdict: DialogVerdict::Deny,
            reason: "removes a system folder".to_owned(),
        },
    );
    assert_eq!(deny.verdict, DialogVerdict::Deny);
    let expected = format!(
        "DIALOG DENIED: the lead refused this Bash call (dialog {id}):\n\
         rmdir /usr\n\
         Reason: removes a system folder\n\
         Do not run this command another way, such as in a script, another shell or another tool. Change your approach, or ask your lead."
    );
    assert_eq!(deny.message.as_deref(), Some(expected.as_str()));
    assert!(expected.starts_with(DENIED_MARKER));
    let allow = decision_for(
        &id,
        &req,
        &PermissionAnswer {
            verdict: DialogVerdict::Approve,
            reason: "fine".to_owned(),
        },
    );
    assert_eq!(
        allow,
        PermissionDecision {
            verdict: DialogVerdict::Approve,
            message: None
        }
    );
}

#[test]
fn clock_time_is_the_utc_time_of_day() {
    let at = Timestamp::parse("2026-11-18T09:05:44.123Z").unwrap();
    assert_eq!(clock_time(at), "09:05:44Z");
}
