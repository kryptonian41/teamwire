//! Who sends and who receives. A same-team party is `lead` or a role; an
//! agent in another team is `<lead|role>@<team>`. Neither a role nor a team
//! name can hold `@`, so every string form has exactly one reading.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::names::{NameError, Role, TeamName};

/// An agent in another team: `lead@<team>` or `<role>@<team>`. It is how a
/// team's store names a party whose rows live in a different team's store.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Remote {
    team: TeamName,
    /// `None` is that team's lead.
    role: Option<Role>,
    text: String,
}

impl Remote {
    /// `role@team` is at most 16 + 1 + 12 characters.
    pub const MAX_LEN: usize = Role::MAX_LEN + 1 + TeamName::MAX_LEN;

    pub fn lead_of(team: &TeamName) -> Self {
        Self {
            team: team.clone(),
            role: None,
            text: format!("lead@{team}"),
        }
    }

    pub fn member_of(team: &TeamName, role: &Role) -> Self {
        Self {
            team: team.clone(),
            role: Some(role.clone()),
            text: format!("{role}@{team}"),
        }
    }

    pub fn parse(text: &str) -> Result<Self, NameError> {
        let refuse = |rule| NameError {
            kind: "team-qualified name",
            value: text.to_owned(),
            rule,
        };
        let (who, team) = text
            .split_once('@')
            .ok_or_else(|| refuse("must be <lead|role>@<team>"))?;
        let team = TeamName::parse(team).map_err(|e| refuse(e.rule))?;
        if who == "lead" {
            Ok(Self::lead_of(&team))
        } else {
            let role = Role::parse(who).map_err(|e| refuse(e.rule))?;
            Ok(Self::member_of(&team, &role))
        }
    }

    pub fn team(&self) -> &TeamName {
        &self.team
    }

    /// The member's role, or `None` for the team's lead.
    pub fn role(&self) -> Option<&Role> {
        self.role.as_ref()
    }

    /// The same party as its own team's store names it.
    pub fn local(&self) -> Recipient {
        match &self.role {
            None => Recipient::Lead,
            Some(role) => Recipient::Member(role.clone()),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }
}

/// Who is acting. Proven before any rule runs (ADR 0008, ADR 0040). Its
/// string form is `lead`, `user` or the role. A role can never be `lead`,
/// and launch refuses a role named `user`, so the form is unambiguous.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Actor {
    Lead,
    /// The human user, proven by the user key (ADR 0040). It sends only in
    /// its own team, and nobody sends to it, so `Recipient` has no such
    /// variant.
    User,
    Member(Role),
    /// A sender in another team. It appears only on a message stored in the
    /// recipient's team; a proof never yields it, because a caller is always
    /// proven in its own team.
    Other(Remote),
}

/// Who a message is for. The same string form as `Actor`, without `user`:
/// the text `user` reads as a member's role here.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Recipient {
    Lead,
    Member(Role),
    Other(Remote),
}

impl Actor {
    /// This actor as another team's store names it, when `own` is the team
    /// it acts in. A sender that is already remote keeps its name. `None`
    /// for the user, who sends only in its own team.
    pub fn seen_from_outside(&self, own: &TeamName) -> Option<Remote> {
        match self {
            Self::Lead => Some(Remote::lead_of(own)),
            Self::User => None,
            Self::Member(role) => Some(Remote::member_of(own, role)),
            Self::Other(remote) => Some(remote.clone()),
        }
    }

    pub fn parse(text: &str) -> Result<Self, NameError> {
        if text == "user" {
            return Ok(Self::User);
        }
        Recipient::parse(text).map(|r| match r {
            Recipient::Lead => Self::Lead,
            Recipient::Member(role) => Self::Member(role),
            Recipient::Other(remote) => Self::Other(remote),
        })
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Lead => "lead",
            Self::User => "user",
            Self::Member(role) => role.as_str(),
            Self::Other(remote) => remote.as_str(),
        }
    }
}

impl Recipient {
    pub fn parse(text: &str) -> Result<Self, NameError> {
        if text == "lead" {
            Ok(Self::Lead)
        } else if text.contains('@') {
            Remote::parse(text).map(Self::Other)
        } else {
            Role::parse(text).map(Self::Member)
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Lead => "lead",
            Self::Member(role) => role.as_str(),
            Self::Other(remote) => remote.as_str(),
        }
    }

    /// The team the recipient sits in, or `None` for the sender's own team.
    pub fn other_team(&self) -> Option<&TeamName> {
        match self {
            Self::Lead | Self::Member(_) => None,
            Self::Other(remote) => Some(remote.team()),
        }
    }

    /// A name that points back at `own` is the same party as its local
    /// name, so every check and every row sees one spelling.
    #[must_use]
    pub fn local_to(self, own: &TeamName) -> Self {
        match self {
            Self::Other(remote) if remote.team() == own => remote.local(),
            other @ (Self::Lead | Self::Member(_) | Self::Other(_)) => other,
        }
    }
}

macro_rules! party {
    ($name:ident) => {
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<$name, D::Error> {
                let text = <std::borrow::Cow<'de, str> as Deserialize>::deserialize(d)?;
                $name::parse(&text).map_err(<D::Error as serde::de::Error>::custom)
            }
        }
    };
}

party!(Actor);
party!(Recipient);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_for_the_own_team_becomes_local() {
        let tw = TeamName::parse("tw").unwrap();
        let api = Recipient::parse("api@tw").unwrap();
        assert_eq!(api.local_to(&tw).as_str(), "api");
        let other = Recipient::parse("lead@beta").unwrap();
        assert_eq!(other.clone().local_to(&tw), other);
    }

    #[test]
    fn an_actor_seen_from_outside_carries_its_team() {
        let tw = TeamName::parse("tw").unwrap();
        assert_eq!(
            Actor::Lead.seen_from_outside(&tw).unwrap().as_str(),
            "lead@tw"
        );
        let api = Actor::parse("api").unwrap();
        assert_eq!(api.seen_from_outside(&tw).unwrap().as_str(), "api@tw");
        assert_eq!(Actor::User.seen_from_outside(&tw), None);
    }

    #[test]
    fn user_is_the_user_as_a_sender_and_a_role_as_a_recipient() {
        let user = Actor::parse("user").unwrap();
        assert_eq!(user, Actor::User);
        assert_eq!(user.as_str(), "user");
        let json = serde_json::to_string(&user).unwrap();
        assert_eq!(json, "\"user\"");
        assert_eq!(serde_json::from_str::<Actor>(&json).unwrap(), Actor::User);
        assert!(matches!(
            Recipient::parse("user").unwrap(),
            Recipient::Member(_)
        ));
    }
}
