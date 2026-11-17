//! `Harness::lead_start`: `claude --session-id <new> <carried flags>
//! <prompt>`. The old session's transcript wins over the old process's
//! arguments for the permission mode and the model, because a user can
//! change both in the middle of a session.

use std::path::Path;

use serde_json::Value;
use teamwire_core::{
    CarryRules, HarnessError, HarnessErrorCode, LeadStart, LeadStartSpec, SessionId, carry,
};

use crate::fill::{find_transcript, tail_lines};

/// The flags a new lead keeps, from `claude --help` (2026-11-17): the
/// permission flags, the model and its fallback, the effort, and four
/// switches that change nothing but the session's surface. The session flags
/// are replaced by the new session's own id.
const RULES: CarryRules<'static> = CarryRules {
    switches: &[
        DANGER,
        ALLOW_DANGER,
        "--chrome",
        "--no-chrome",
        "--ide",
        "--verbose",
    ],
    with_value: &[MODE, MODEL, "--effort", "--fallback-model"],
    keyed: &[],
    replaced: &[
        ("--session-id", true),
        ("--resume", true),
        ("-r", true),
        ("--continue", false),
        ("-c", false),
        ("--fork-session", false),
    ],
};

const DANGER: &str = "--dangerously-skip-permissions";
const ALLOW_DANGER: &str = "--allow-dangerously-skip-permissions";
const MODE: &str = "--permission-mode";
const MODEL: &str = "--model";

/// The values `claude --permission-mode` accepts. A transcript value outside
/// them would make `claude` exit at once, so it never wins.
const MODES: [&str; 6] = [
    "acceptEdits",
    "auto",
    "bypassPermissions",
    "manual",
    "dontAsk",
    "plan",
];

pub(crate) fn lead_start(
    claude_bin: &str,
    claude_dir: &Path,
    req: &LeadStart,
) -> Result<LeadStartSpec, HarnessError> {
    let new = req.new_session.as_ref().ok_or_else(|| {
        HarnessError::new(
            HarnessErrorCode::BadInput,
            "a Claude Code lead start needs the new session's id",
        )
    })?;
    let (mut carried, dropped) = carry(&req.old_args, &RULES);
    let (mode, model) = transcript_values(claude_dir, &req.old_session);
    if let Some(mode) = mode {
        carried = with_mode(carried, &mode);
    }
    if let Some(model) = model {
        carried = with_model(carried, &model);
    }
    let mut argv = vec![
        claude_bin.to_owned(),
        "--session-id".to_owned(),
        new.as_str().to_owned(),
    ];
    argv.extend(carried);
    argv.push(req.prompt.clone());
    Ok(LeadStartSpec { argv, dropped })
}

/// The newest valid `permissionMode` and the newest model of a main-chain
/// assistant line, from the end of the old session's transcript.
fn transcript_values(claude_dir: &Path, session: &SessionId) -> (Option<String>, Option<String>) {
    let Some(tail) = find_transcript(claude_dir, session).and_then(|f| tail_lines(&f)) else {
        return (None, None);
    };
    let mut mode = None;
    let mut model = None;
    for line in tail.lines().rev() {
        if mode.is_some() && model.is_some() {
            break;
        }
        let Ok(doc) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if doc.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        if mode.is_none() {
            mode = doc
                .get("permissionMode")
                .and_then(Value::as_str)
                .filter(|m| MODES.contains(m))
                .map(str::to_owned);
        }
        // An API error is written as an assistant line with model
        // `<synthetic>`: it names no model the session ran on.
        if model.is_none()
            && doc.get("type").and_then(Value::as_str) == Some("assistant")
            && doc.get("isApiErrorMessage").and_then(Value::as_bool) != Some(true)
        {
            model = doc
                .pointer("/message/model")
                .and_then(Value::as_str)
                .filter(|m| !m.is_empty() && *m != "<synthetic>")
                .map(str::to_owned);
        }
    }
    (mode, model)
}

/// `carried` with the transcript's permission mode in place of the old
/// permission flags. A lead started with the bypass flag keeps the right to
/// switch to bypass again.
fn with_mode(carried: Vec<String>, mode: &str) -> Vec<String> {
    let had_danger = carried.iter().any(|a| a == DANGER);
    let mut out = without_flag(carried, MODE);
    out.retain(|a| a != DANGER);
    if had_danger && mode != "bypassPermissions" && !out.iter().any(|a| a == ALLOW_DANGER) {
        out.push(ALLOW_DANGER.to_owned());
    }
    out.extend([MODE.to_owned(), mode.to_owned()]);
    out
}

/// `carried` with the transcript's model. An old `--model` value that starts
/// with that model is kept, so a suffix such as `[1m]` stays.
fn with_model(carried: Vec<String>, model: &str) -> Vec<String> {
    let old = value_of(&carried, MODEL);
    if old.as_deref().is_some_and(|v| v.starts_with(model)) {
        return carried;
    }
    let mut out = without_flag(carried, MODEL);
    out.extend([MODEL.to_owned(), model.to_owned()]);
    out
}

/// The value of `flag` in `args`, as `--flag value` or `--flag=value`.
fn value_of(args: &[String], flag: &str) -> Option<String> {
    let mut words = args.iter();
    while let Some(word) = words.next() {
        if word == flag {
            return words.next().cloned();
        }
        if let Some(value) = word.strip_prefix(flag).and_then(|r| r.strip_prefix('=')) {
            return Some(value.to_owned());
        }
    }
    None
}

/// `args` without `flag` and its value, in either form.
fn without_flag(args: Vec<String>, flag: &str) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut words = args.into_iter();
    while let Some(word) = words.next() {
        if word == flag {
            words.next();
        } else if !word
            .strip_prefix(flag)
            .is_some_and(|rest| rest.starts_with('='))
        {
            out.push(word);
        }
    }
    out
}
