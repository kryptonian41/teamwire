//! The dialogs Codex can show before its composer appears. Each is answered
//! by pressing keys that move the `›` pointer to a chosen numbered option.
//!
//! Never answers the hooks review: its "Trust all and continue" would trust
//! the hooks in the user's own `~/.codex` too (the analogue of INV-25).

use std::time::Duration;

use teamwire_core::screen::{STARTUP_HOST_TIMEOUT, StartupPoll};
use teamwire_core::{
    DialogKind, HarnessError, HarnessErrorCode, Host, Key, Place, ScreenState, StartupOutcome,
    TrustPolicy,
};

use crate::screen::{
    APPROVAL_MARKERS, CWD_MARKER, HOOKS_REVIEW_MARKER, MIGRATION_MARKER, TRUST_MARKER,
    UPDATE_MARKER, classify, has_any, is_selected_option, last_lines, last_pointer_text,
    pointer_rest, squash,
};

/// The sign-in choices (`onboarding/auth.rs`).
const LOGIN_MARKERS: [&str; 3] = [
    "Sign in with ChatGPT",
    "Sign in with Device Code",
    "Finish signing in via your browser",
];

/// The timeout of each key teamwire sends to a starting member.
const HOST_TIMEOUT: Duration = STARTUP_HOST_TIMEOUT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dialog {
    Login,
    HooksReview,
    Trust,
    /// "Update available": skip this version.
    Update,
    /// "Codex just got an upgrade": refused, never answered.
    Migration,
    /// The resume picker: continue in the session's folder.
    Cwd,
}

impl Dialog {
    /// The numbered option to choose, by its text.
    fn choice(self) -> Option<&'static str> {
        match self {
            Self::Trust => Some("Trust and continue"),
            Self::Update => Some("Skip"),
            Self::Cwd => Some("Use session directory"),
            Self::Login | Self::HooksReview | Self::Migration => None,
        }
    }
}

fn dialog_on(screen: &str) -> Option<Dialog> {
    let last = last_lines(screen);
    let drawn: Vec<&str> = last
        .iter()
        .copied()
        .filter(|l| !l.trim_start().starts_with('•'))
        .collect();
    let text = squash(&drawn.join("\n"));
    if has_any(&text, &LOGIN_MARKERS) {
        return Some(Dialog::Login);
    }
    // A numbered choice with the pointer on it is what separates a dialog
    // from the same words in the model's reply.
    if !drawn.iter().any(|l| is_selected_option(l)) {
        return None;
    }
    [
        (HOOKS_REVIEW_MARKER, Dialog::HooksReview),
        (TRUST_MARKER, Dialog::Trust),
        (UPDATE_MARKER, Dialog::Update),
        (MIGRATION_MARKER, Dialog::Migration),
        (CWD_MARKER, Dialog::Cwd),
    ]
    .into_iter()
    .find(|(marker, _)| text.contains(marker))
    .map(|(_, dialog)| dialog)
}

/// What kind of dialog `screen` shows (ADR 0052): a startup dialog by its
/// marker, else an approval overlay. Codex runs its members with approvals
/// off, so an approval overlay is not expected, but it is named if it comes.
pub(crate) fn dialog_kind(screen: &str) -> DialogKind {
    match dialog_on(screen) {
        Some(Dialog::Login) => DialogKind::Login,
        Some(Dialog::Trust | Dialog::HooksReview) => DialogKind::Trust,
        Some(Dialog::Update | Dialog::Migration) => DialogKind::Update,
        Some(Dialog::Cwd) => DialogKind::Unknown,
        None if classify(screen) == ScreenState::Dialog => {
            if has_any(&squash(screen), &APPROVAL_MARKERS) {
                DialogKind::Permission
            } else {
                DialogKind::Unknown
            }
        }
        None => DialogKind::NoDialog,
    }
}

/// The number in front of the option named `want`: `2` for `2. Skip`.
fn option_number(screen: &str, want: &str) -> Option<usize> {
    screen.lines().find_map(|line| {
        let rest = pointer_rest(line).unwrap_or(line.trim());
        let (n, text) = rest.split_once(". ")?;
        let text = squash(text);
        (text == want || text.starts_with(&format!("{want} (")))
            .then(|| n.parse().ok())
            .flatten()
    })
}

/// The one key that moves toward `want`, or Enter when the pointer is on it.
fn key_toward(screen: &str, want: &str) -> Option<Key> {
    let target = option_number(screen, want)?;
    let at: usize = last_pointer_text(screen)?
        .split_once(". ")?
        .0
        .parse()
        .ok()?;
    Some(match at.cmp(&target) {
        std::cmp::Ordering::Equal => Key::Enter,
        std::cmp::Ordering::Less => Key::Down,
        std::cmp::Ordering::Greater => Key::Up,
    })
}

fn timeout() -> HarnessError {
    HarnessError::new(
        HarnessErrorCode::Timeout,
        "neither the composer nor a startup dialog appeared",
    )
}

/// Watches the member's screen until it shows the composer. Folder trust is
/// answered when `policy` allows. The update prompt is skipped and the resume
/// folder prompt keeps the session's folder. The hooks review is never
/// answered, and the model-upgrade prompt and a sign-in screen are errors.
pub(crate) fn answer_startup(
    host: &dyn Host,
    place: &Place,
    policy: TrustPolicy,
    poll: StartupPoll,
) -> Result<StartupOutcome, HarnessError> {
    // A key can be dropped by a slow start, so the same dialog is answered
    // again only after a long wait, which a late first key would not undo.
    let patience = (poll.steps / 3).max(1);
    let mut trusted = false;
    let mut sent: Option<(Dialog, u32, Option<String>)> = None;
    let mut left = poll.steps;
    while left > 0 {
        left -= 1;
        let Some(screen) = poll.read(host, place, &mut left)? else {
            std::thread::sleep(poll.step);
            continue;
        };
        match dialog_on(&screen) {
            Some(Dialog::Login) => {
                return Err(HarnessError::new(
                    HarnessErrorCode::Refused,
                    "Codex is not signed in; run `codex login` and start the team again",
                ));
            }
            // Declining it makes Codex write the acknowledgement into the
            // user's `~/.codex/config.toml` (`tui/src/app/startup_prompts.rs`
            // `PersistModelMigrationPromptAcknowledged`), and no `-c` value
            // hides it for a model we cannot name in advance.
            Some(Dialog::Migration) => {
                return Err(HarnessError::new(
                    HarnessErrorCode::Refused,
                    "Codex shows its \"Codex just got an upgrade\" prompt; open Codex once yourself and answer it, then start the team again",
                ));
            }
            Some(Dialog::HooksReview) => return Ok(StartupOutcome::RefusedEscalated),
            Some(Dialog::Trust) if policy == TrustPolicy::Refuse => {
                return Err(HarnessError::new(
                    HarnessErrorCode::Refused,
                    "the spec does not accept the folder-trust dialog",
                ));
            }
            Some(dialog) => {
                // A key that moved the pointer has taken effect, so the next
                // one goes at once. One that changed nothing may have been
                // dropped, and is sent again only after the long wait.
                let pointer = last_pointer_text(&screen);
                let waited = match &sent {
                    Some((d, n, at)) if *d == dialog && *at == pointer => *n,
                    _ => patience,
                };
                if waited >= patience
                    && let Some(want) = dialog.choice()
                    && let Some(key) = key_toward(&screen, want)
                {
                    host.send_keys(place, &[key], HOST_TIMEOUT)?;
                    trusted |= dialog == Dialog::Trust && key == Key::Enter;
                    sent = Some((dialog, 0, pointer));
                } else {
                    sent = Some((dialog, waited + 1, pointer));
                }
            }
            None if classify(&screen) == ScreenState::InputPrompt => {
                return Ok(if trusted {
                    StartupOutcome::AnsweredPlainTrust
                } else {
                    StartupOutcome::NoDialog
                });
            }
            None => {}
        }
        std::thread::sleep(poll.step);
    }
    Err(timeout())
}
