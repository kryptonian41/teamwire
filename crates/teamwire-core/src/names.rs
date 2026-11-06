//! Names and ids. Every name is checked when it is built, so a `Role` or an
//! `Address` in hand always obeys its rule.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::strings::checked_name;
use crate::timestamp::Timestamp;

/// A name that broke its rule. `rule` says which one, in words.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("invalid {kind} {value:?}: {rule}")]
pub struct NameError {
    pub kind: &'static str,
    pub value: String,
    pub rule: &'static str,
}

fn is_lower_start(text: &str) -> bool {
    text.chars().next().is_some_and(|c| c.is_ascii_lowercase())
}

/// `^[a-z][a-z0-9]{0,11}$`.
fn check_team(text: &str) -> Result<(), &'static str> {
    if !is_lower_start(text) {
        return Err("must start with a lower-case letter a-z");
    }
    if text.len() > TeamName::MAX_LEN {
        return Err("must be at most 12 characters");
    }
    if !text
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    {
        return Err("may hold only a-z and 0-9 (no hyphen, no underscore)");
    }
    Ok(())
}

/// `^[a-z][a-z0-9_]{0,15}$`, and never `lead`.
fn check_role(text: &str) -> Result<(), &'static str> {
    if !is_lower_start(text) {
        return Err("must start with a lower-case letter a-z");
    }
    if text.len() > Role::MAX_LEN {
        return Err("must be at most 16 characters");
    }
    if !text
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err("may hold only a-z, 0-9 and _ (no hyphen)");
    }
    if text == "lead" {
        return Err("\"lead\" is reserved for the team's lead");
    }
    Ok(())
}

/// `tw_<team>_<role>`, at most 32 characters. A team name has no
/// underscore, so the first `_` after `tw_` ends the team part.
fn check_address(text: &str) -> Result<(), &'static str> {
    if text.len() > Address::MAX_LEN {
        return Err("must be at most 32 characters");
    }
    let Some(rest) = text.strip_prefix(Address::PREFIX) else {
        return Err("must start with tw_");
    };
    let Some((team, role)) = rest.split_once('_') else {
        return Err("must be tw_<team>_<role>");
    };
    check_team(team).map_err(|_| "the team part is not a valid team name")?;
    check_role(role).map_err(|_| "the role part is not a valid role")?;
    Ok(())
}

/// `^[a-z][a-z0-9-]{0,31}$`: the id of a harness or host adapter.
fn check_adapter_id(text: &str) -> Result<(), &'static str> {
    if !is_lower_start(text) {
        return Err("must start with a lower-case letter a-z");
    }
    if text.len() > 32 {
        return Err("must be at most 32 characters");
    }
    if !text
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("may hold only a-z, 0-9 and -");
    }
    Ok(())
}

/// A lower-case letter, then 3 to 9 digits: `m014`.
fn check_display_id(text: &str) -> Result<(), &'static str> {
    let mut chars = text.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_lowercase()) {
        return Err("must start with a lower-case letter a-z");
    }
    let digits = chars.as_str();
    if !(3..=9).contains(&digits.len()) || !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err("must be a letter and 3 to 9 digits, such as m014");
    }
    Ok(())
}

/// Opaque, but one printable token of at most 128 characters.
fn check_session_id(text: &str) -> Result<(), &'static str> {
    if text.is_empty() {
        return Err("must not be empty");
    }
    if text.chars().count() > 128 {
        return Err("must be at most 128 characters");
    }
    if text.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("may not hold spaces or control characters");
    }
    Ok(())
}

checked_name! {
    /// `^[a-z][a-z0-9]{0,11}$`: no hyphen, no underscore. Checked by `TeamName::parse`.
    pub struct TeamName, "team name", check_team
}

impl TeamName {
    pub const MAX_LEN: usize = 12;
}

checked_name! {
    /// `^[a-z][a-z0-9_]{0,15}$`: no hyphen. `"lead"` is reserved and refused.
    pub struct Role, "role", check_role
}

impl Role {
    pub const MAX_LEN: usize = 16;
}

checked_name! {
    /// `tw_<team>_<role>`, at most 32 characters: the member's name in hosts and
    /// harnesses. It never contains a hyphen, so it never equals a `<team>-<role>`
    /// name that another tool gives its agents (INV-32).
    pub struct Address, "address", check_address
}

impl Address {
    pub const MAX_LEN: usize = 32;
    const PREFIX: &'static str = "tw_";

    /// The address of `role` in `team`. Always valid: the longest team and
    /// role make exactly 32 characters.
    pub fn of(team: &TeamName, role: &Role) -> Self {
        Self(format!(
            "{}{}_{}",
            Self::PREFIX,
            team.as_str(),
            role.as_str()
        ))
    }

    /// The tab name of a team's lead when `reopen` resumes it. No member can
    /// have this name, because the role `lead` is reserved, so `Address::parse`
    /// refuses it and nothing reads it back as a member.
    pub fn lead(team: &TeamName) -> Self {
        Self(format!("{}{}_lead", Self::PREFIX, team.as_str()))
    }

    /// The team part of `tw_<team>_<role>`. A team name has no underscore,
    /// so the first `_` after `tw_` ends it.
    pub fn team(&self) -> &str {
        self.parts().0
    }

    /// The role part of `tw_<team>_<role>`: `lead` for `Address::lead`.
    pub fn role(&self) -> &str {
        self.parts().1
    }

    fn parts(&self) -> (&str, &str) {
        let rest = self.0.strip_prefix(Self::PREFIX).unwrap_or(&self.0);
        rest.split_once('_').unwrap_or((rest, ""))
    }
}

checked_name! {
    /// Adapter ids are open strings, `^[a-z][a-z0-9-]{0,31}$`: "claude-code",
    /// "herdr", "tmux", "none", "fake-a". The core never matches on a value; it
    /// looks the id up in the `Registry` and reads the adapter's capability
    /// record.
    pub struct HarnessId, "harness id", check_adapter_id
}

checked_name! {
    /// A host adapter's id; the same rule as `HarnessId`.
    pub struct HostId, "host id", check_adapter_id
}

impl HarnessId {
    /// An adapter's own id, written as a constant in its source. The rule is
    /// checked in debug builds, so every test run proves the constant valid.
    pub fn from_static(text: &'static str) -> Self {
        debug_assert!(check_adapter_id(text).is_ok(), "bad harness id {text:?}");
        Self(text.to_owned())
    }
}

impl HostId {
    /// An adapter's own id, written as a constant in its source. The rule is
    /// checked in debug builds, so every test run proves the constant valid.
    pub fn from_static(text: &'static str) -> Self {
        debug_assert!(check_adapter_id(text).is_ok(), "bad host id {text:?}");
        Self(text.to_owned())
    }
}

checked_name! {
    /// "m014": shown to agents; unique per team.
    pub struct DisplayId, "display id", check_display_id
}

impl DisplayId {
    /// The display id of the `n`th message of a team: `m001`, `m014`, `m1234`.
    /// `None` past `m999999999`.
    pub fn message(n: u64) -> Option<Self> {
        Self::parse(&format!("m{n:03}")).ok()
    }
}

checked_name! {
    /// The harness's own session id, opaque.
    pub struct SessionId, "session id", check_session_id
}

/// Where a member runs inside a host: herdr `"w3T:p3"`, a tmux pane id.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Place {
    pub host: HostId,
    pub id: String,
}

/// An operating-system process id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Pid(u32);

impl Pid {
    pub fn new(pid: u32) -> Self {
        Self(pid)
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for Pid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identity of messages, completions, events and teams. The core never makes
/// one from the system clock or random source: the caller passes both in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ulid(ulid::Ulid);

impl Ulid {
    /// A ULID for `time`, with 80 random bits taken from `random`.
    pub fn from_parts(time: Timestamp, random: u128) -> Self {
        let millis = u64::try_from(time.unix_millis()).unwrap_or(0);
        Self(ulid::Ulid::from_parts(millis, random))
    }

    pub fn parse(text: &str) -> Result<Self, NameError> {
        ulid::Ulid::from_string(text)
            .map(Ulid)
            .map_err(|_| NameError {
                kind: "ulid",
                value: text.to_owned(),
                rule: "must be 26 Crockford base-32 characters",
            })
    }
}

impl fmt::Display for Ulid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for Ulid {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Ulid {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str> as Deserialize>::deserialize(d)?;
        Self::parse(&text).map_err(<D::Error as serde::de::Error>::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_is_built_from_team_and_role() {
        let team = TeamName::parse("tw").unwrap();
        let role = Role::parse("api_2").unwrap();
        assert_eq!(Address::of(&team, &role).as_str(), "tw_tw_api_2");
    }

    #[test]
    fn lead_address_is_the_one_no_member_can_have() {
        let team = TeamName::parse("alpha").unwrap();
        assert_eq!(Address::lead(&team).as_str(), "tw_alpha_lead");
        assert!(Address::parse("tw_alpha_lead").is_err());
    }

    #[test]
    fn an_address_splits_at_the_first_underscore_after_tw() {
        let member = Address::parse("tw_instr_plan_rev").unwrap();
        assert_eq!((member.team(), member.role()), ("instr", "plan_rev"));
        let lead = Address::lead(&TeamName::parse("instr").unwrap());
        assert_eq!((lead.team(), lead.role()), ("instr", "lead"));
    }

    #[test]
    fn display_id_pads_to_three_digits() {
        assert_eq!(DisplayId::message(14).unwrap().as_str(), "m014");
        assert_eq!(DisplayId::message(1234).unwrap().as_str(), "m1234");
    }
}
