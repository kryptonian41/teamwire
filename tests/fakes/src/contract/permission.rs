//! The permission-dialog contract (ADR 0052): what every harness's
//! `dialog_kind`, `parse_permission_request` and `permission_reply` must
//! do. A harness either has a permission hook, and then its reply allows one
//! call or denies it with the lead's text and never grants anything lasting,
//! or it has none and says so with `Unsupported`.

use serde_json::Value;
use teamwire_core::{
    DialogKind, DialogVerdict, Harness, HarnessErrorCode, PermissionAnswer, PermissionDecision,
    Timestamp, Ulid, decision_for,
};

use super::{fail, ok, require};

/// What the suite needs from one harness.
pub struct PermissionSetup {
    pub harness: Box<dyn Harness>,
    /// A screen at the input prompt.
    pub prompt_screen: String,
    /// Screens and the kind each must give.
    pub named_screens: Vec<(String, DialogKind)>,
    /// Hook input for an answerable call; `None` for a harness with no hook.
    pub answerable_stdin: Option<Vec<u8>>,
    /// Hook input for a tool teamwire never answers.
    pub other_tool_stdin: Option<Vec<u8>>,
}

impl std::fmt::Debug for PermissionSetup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PermissionSetup")
            .field("harness", &self.harness.id())
            .field("named_screens", &self.named_screens.len())
            .finish_non_exhaustive()
    }
}

/// Keys a reply must never hold: each would grant more than this one call,
/// change the call, or stop the member's turn.
const FORBIDDEN_KEYS: [&str; 3] = ["updatedPermissions", "updatedInput", "interrupt"];

pub fn permission_contract(setup: &PermissionSetup) {
    let h = setup.harness.as_ref();
    for screen in [setup.prompt_screen.as_str(), ""] {
        let kind = h.dialog_kind(screen);
        require(
            &format!("a screen with no dialog gives none or cannot_tell, got {kind}"),
            matches!(kind, DialogKind::NoDialog | DialogKind::CannotTell),
        );
    }
    for (screen, want) in &setup.named_screens {
        let got = h.dialog_kind(screen);
        if got != *want {
            fail(
                &format!("dialog_kind must name this screen {want}, got {got}"),
                screen,
            );
        }
    }
    match &setup.answerable_stdin {
        Some(stdin) => with_hook(h, stdin, setup.other_tool_stdin.as_deref()),
        None => without_hook(h),
    }
}

fn with_hook(h: &dyn Harness, stdin: &[u8], other: Option<&[u8]>) {
    let request = ok(
        "parse_permission_request",
        h.parse_permission_request(stdin),
    );
    require("the answerable input is answerable", request.answerable());
    require("a request carries the tool name", !request.tool.is_empty());
    let id = Ulid::from_parts(
        Timestamp::from_unix_millis(1_790_000_000_000).unwrap_or_else(|| fail("a time", "")),
        1,
    );
    for verdict in DialogVerdict::ALL {
        let answer = PermissionAnswer {
            verdict: *verdict,
            reason: "the contract's reason".to_owned(),
        };
        let decision = decision_for(&id, &request, &answer);
        let bytes = ok("permission_reply", h.permission_reply(&decision));
        let reply: Value = ok("a reply is JSON", serde_json::from_slice(&bytes));
        if let Some(key) = forbidden_key(&reply) {
            fail(&format!("a reply never holds {key}"), reply);
        }
        if let Some(message) = &decision.message
            && !holds_string(&reply, message)
        {
            fail("the deny reply holds the decision's whole message", reply);
        }
    }
    if let Some(other) = other {
        let request = ok(
            "parse_permission_request",
            h.parse_permission_request(other),
        );
        require(
            "a tool teamwire never answers is not answerable",
            !request.answerable(),
        );
    }
    let junk = h.parse_permission_request(b"not json at all");
    match junk {
        Err(e) if e.code == HarnessErrorCode::BadInput => {}
        other => fail("junk hook input gives bad_input", other),
    }
}

fn without_hook(h: &dyn Harness) {
    let parsed = h.parse_permission_request(br#"{"tool_name":"Bash"}"#);
    match parsed {
        Err(e) if e.code == HarnessErrorCode::Unsupported => {}
        other => fail("a harness with no hook cannot parse a request", other),
    }
    for verdict in DialogVerdict::ALL {
        let decision = PermissionDecision {
            verdict: *verdict,
            message: None,
        };
        match h.permission_reply(&decision) {
            Err(e) if e.code == HarnessErrorCode::Unsupported => {}
            other => fail("a harness with no hook has no reply", other),
        }
    }
}

/// The first forbidden key anywhere in `v`.
fn forbidden_key(v: &Value) -> Option<&'static str> {
    match v {
        Value::Object(map) => FORBIDDEN_KEYS
            .into_iter()
            .find(|k| map.contains_key(*k))
            .or_else(|| map.values().find_map(forbidden_key)),
        Value::Array(items) => items.iter().find_map(forbidden_key),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => None,
    }
}

/// Whether some string anywhere in `v` is exactly `text`.
fn holds_string(v: &Value, text: &str) -> bool {
    match v {
        Value::String(s) => s == text,
        Value::Object(map) => map.values().any(|x| holds_string(x, text)),
        Value::Array(items) => items.iter().any(|x| holds_string(x, text)),
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}
