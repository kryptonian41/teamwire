//! What `Harness::lead_start` takes and gives: the command line that starts
//! a fresh lead session in place of a lead whose context is full (ADR 0051),
//! and the one rule every harness uses to carry the old lead's flags.
//!
//! A harness carries a flag only when it names it in its `CarryRules`, an
//! allow list, so a flag it does not know is never passed to the new lead.

use serde::{Deserialize, Serialize};

use crate::names::SessionId;

/// What a harness needs to build a fresh lead's command line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeadStart {
    /// The lead session whose context is full.
    pub old_session: SessionId,
    /// The old lead process's arguments, `argv[0]` first, or empty when they
    /// cannot be read.
    pub old_args: Vec<String>,
    /// The new session's id, which teamwire chooses.
    pub new_session: SessionId,
    /// The first prompt of the new session.
    pub prompt: String,
}

/// A fresh lead's command line, the old flags it leaves out, and what the
/// harness has to say about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeadStartSpec {
    /// The program and its arguments; the prompt is the last one.
    pub argv: Vec<String>,
    /// Each old flag that was not carried, by name only. A value is never
    /// shown, because it can hold a secret (`pi --api-key`).
    pub dropped: Vec<String>,
    /// One line each, printed as a `warning:` line by `teamwire lead
    /// --handoff` and its dry run, such as a permission mode from the old
    /// transcript that the new lead cannot be started with. Never a secret.
    pub warnings: Vec<String>,
}

/// Which old flags a harness carries to a new lead.
#[derive(Debug, Clone, Copy)]
pub struct CarryRules<'a> {
    /// Flags with no value, such as `--verbose`.
    pub switches: &'a [&'a str],
    /// Flags that take one value, as the next word or after `=`.
    pub with_value: &'a [&'a str],
    /// Flags the new start sets itself, such as the session to resume, and
    /// whether each takes a value. They are neither carried nor dropped.
    pub replaced: &'a [(&'a str, bool)],
}

/// The old flags `rules` carries, in their order, and the names of the
/// flags it drops. `old_args[0]` is the program. A flag's value is the next
/// word, unless that word is itself a flag. A word that is no flag is the
/// value of a dropped flag, a subcommand or an old prompt: it is never
/// carried, and never shown. Everything after `--` is the old prompt.
pub fn carry(old_args: &[String], rules: &CarryRules<'_>) -> (Vec<String>, Vec<String>) {
    let mut carried = Vec::new();
    let mut dropped = Vec::new();
    let mut words = old_args.iter().skip(1).peekable();
    while let Some(word) = words.next() {
        if word == "--" {
            break;
        }
        let (name, inline) = match word.split_once('=') {
            Some((name, value)) if name.starts_with('-') => (name, Some(value.to_owned())),
            _ => (word.as_str(), None),
        };
        let takes = |flag: &&str| *flag == name;
        let has_value = rules.with_value.iter().any(takes)
            || rules.replaced.iter().any(|(flag, v)| *flag == name && *v);
        let next = if has_value && inline.is_none() {
            words.next_if(|w| !w.starts_with('-')).cloned()
        } else {
            None
        };
        let value = inline.clone().or_else(|| next.clone());
        if rules.replaced.iter().any(|(flag, _)| *flag == name) {
            continue;
        }
        let allowed = if rules.switches.iter().any(takes) {
            inline.is_none()
        } else if rules.with_value.iter().any(takes) {
            value.is_some()
        } else {
            false
        };
        if allowed {
            carried.push(word.clone());
            carried.extend(next);
        } else if name.starts_with('-') {
            dropped.push(name.to_owned());
        }
    }
    (carried, dropped)
}
