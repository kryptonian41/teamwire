//! A team member as the store records it.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::message::MessageType;
use crate::names::{HarnessId, HostId, Place, Role, SessionId};
use crate::strings::string_enum;
use crate::timestamp::Timestamp;
use crate::who::Actor;

string_enum! {
    pub enum MemberKind {
        Implementer => "implementer",
        Reviewer => "reviewer",
        Investigator => "investigator",
        Closer => "closer",
        /// Reads the lead's plan and briefs before any implementer starts
        /// (ADR 0034).
        PlanReviewer => "plan_reviewer",
        /// Reads the reports of one set of readers, checks each finding in
        /// the code, and writes the one report the next step acts on.
        Reconciler => "reconciler",
    }
}

string_enum! {
    pub enum Mode {
        Worktree => "worktree",
        Pane => "pane",
    }
}

string_enum! {
    pub enum MemberState {
        Creating => "creating",
        Started => "started",
        Failed => "failed",
        Removed => "removed",
    }
}

impl MemberKind {
    /// Reviewers, plan reviewers, reconcilers and closers report in their
    /// report file; the lead acts on the report (MSG-3). This answers for
    /// the kind alone: `Member::is_quiet` also reads the member's `quiet`
    /// setting.
    pub fn is_quiet(self) -> bool {
        matches!(
            self,
            Self::Reviewer | Self::Closer | Self::PlanReviewer | Self::Reconciler
        )
    }

    /// Whether a member of this kind may send a message of this type. Only the
    /// lead instructs, so a member never sends a reprompt or a note (MSG-1).
    pub fn may_send(self, kind: MessageType) -> bool {
        !self.is_quiet()
            && matches!(
                kind,
                MessageType::Notice
                    | MessageType::Finding
                    | MessageType::Question
                    | MessageType::Answer
            )
    }
}

/// One row of the `members` table. The token hash stays in the store: a
/// `Member` is printed and passed around, and must never carry it.
///
/// Its JSON is written by hand (see `Serialize` below), so that the `quiet`
/// key says whether the member is quiet in effect.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Member {
    pub role: Role,
    pub title: String,
    pub kind: MemberKind,
    pub mode: Mode,
    pub harness: HarnessId,
    pub host: HostId,
    pub place: Option<Place>,
    pub session_id: Option<SessionId>,
    pub cwd: PathBuf,
    pub repo: Option<PathBuf>,
    pub branch: Option<String>,
    pub base: Option<String>,
    pub base_sha: Option<String>,
    pub check: Option<String>,
    pub owns: Vec<String>,
    pub deadline_s: u64,
    pub state: MemberState,
    pub generation: u32,
    pub gen_started_at: Timestamp,
    /// The generation the report check last held this member in, if any.
    pub gate_told_generation: Option<u32>,
    pub removed_at: Option<Timestamp>,
    pub removed_note: Option<String>,
    /// What the spec's `quiet` said: true makes a member of any kind quiet.
    /// A kind that is quiet by nature stores false here, so read
    /// `is_quiet`, never this field, to decide.
    #[serde(default)]
    pub quiet: bool,
    /// The set this member belongs to: members with the same label in one
    /// team do the same judging job on the same brief.
    #[serde(default)]
    pub set: Option<String>,
}

impl Member {
    /// Whether the member sends no message and hears only its own lead
    /// (MSG-3): its kind is quiet, or its spec set `quiet`. Every rule that
    /// decides silence asks this, never the kind alone.
    pub fn is_quiet(&self) -> bool {
        self.quiet || self.kind.is_quiet()
    }

    /// Whether this member may send a message of this type: what its kind
    /// may send, and nothing when it is quiet.
    pub fn may_send(&self, kind: MessageType) -> bool {
        !self.is_quiet() && self.kind.may_send(kind)
    }

    /// Whether this member may be shown a message from `sender`, which is
    /// stored in this member's own team: a quiet member hears only its own
    /// lead (MSG-3). Every read that shows a member a message asks this as
    /// well as the send check, because a member can become quiet after a
    /// message to it was stored: a relaunch after a failed start may set
    /// `quiet` or a quiet kind.
    pub fn hears(&self, sender: &Actor) -> bool {
        !self.is_quiet() || *sender == Actor::Lead
    }
}

/// The fields of a `Member` as its JSON names them, with `quiet` in effect.
/// A reader of the JSON never needs to know which kinds are quiet.
#[derive(Serialize)]
struct MemberJson<'a> {
    role: &'a Role,
    title: &'a str,
    kind: MemberKind,
    mode: Mode,
    harness: &'a HarnessId,
    host: &'a HostId,
    place: &'a Option<Place>,
    session_id: &'a Option<SessionId>,
    cwd: &'a PathBuf,
    repo: &'a Option<PathBuf>,
    branch: &'a Option<String>,
    base: &'a Option<String>,
    base_sha: &'a Option<String>,
    check: &'a Option<String>,
    owns: &'a [String],
    deadline_s: u64,
    state: MemberState,
    generation: u32,
    gen_started_at: Timestamp,
    gate_told_generation: Option<u32>,
    removed_at: Option<Timestamp>,
    removed_note: &'a Option<String>,
    quiet: bool,
    set: &'a Option<String>,
}

impl Serialize for Member {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        MemberJson {
            role: &self.role,
            title: &self.title,
            kind: self.kind,
            mode: self.mode,
            harness: &self.harness,
            host: &self.host,
            place: &self.place,
            session_id: &self.session_id,
            cwd: &self.cwd,
            repo: &self.repo,
            branch: &self.branch,
            base: &self.base,
            base_sha: &self.base_sha,
            check: &self.check,
            owns: &self.owns,
            deadline_s: self.deadline_s,
            state: self.state,
            generation: self.generation,
            gen_started_at: self.gen_started_at,
            gate_told_generation: self.gate_told_generation,
            removed_at: self.removed_at,
            removed_note: &self.removed_note,
            quiet: self.is_quiet(),
            set: &self.set,
        }
        .serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn may_send_pins_every_kind_and_type() {
        for kind in MemberKind::ALL {
            for ty in MessageType::ALL {
                let want = matches!(kind, MemberKind::Implementer | MemberKind::Investigator)
                    && matches!(
                        ty,
                        MessageType::Notice
                            | MessageType::Finding
                            | MessageType::Question
                            | MessageType::Answer
                    );
                assert_eq!(kind.may_send(*ty), want, "{kind} {ty}");
            }
            assert_eq!(
                kind.is_quiet(),
                matches!(
                    kind,
                    MemberKind::Reviewer
                        | MemberKind::Closer
                        | MemberKind::PlanReviewer
                        | MemberKind::Reconciler
                )
            );
        }
    }

    fn member(kind: MemberKind, quiet: bool) -> Member {
        Member {
            role: Role::parse("rec").unwrap(),
            title: "Reconcile".to_owned(),
            kind,
            mode: Mode::Pane,
            harness: HarnessId::from_static("pi"),
            host: HostId::from_static("herdr"),
            place: Some(Place {
                host: HostId::from_static("herdr"),
                id: "w1:p2".to_owned(),
            }),
            session_id: Some(SessionId::parse("sess-1").unwrap()),
            cwd: PathBuf::from("/r"),
            repo: Some(PathBuf::from("/r")),
            branch: Some("alpha/rec".to_owned()),
            base: Some("main".to_owned()),
            base_sha: Some("abc123".to_owned()),
            check: Some("cargo test".to_owned()),
            owns: vec!["src/".to_owned()],
            deadline_s: 1800,
            state: MemberState::Started,
            generation: 2,
            gen_started_at: Timestamp::from_unix_millis(1_790_000_000_000).unwrap(),
            gate_told_generation: Some(1),
            removed_at: Some(Timestamp::from_unix_millis(1_790_000_100_000).unwrap()),
            removed_note: Some("done".to_owned()),
            quiet,
            set: Some("rev1".to_owned()),
        }
    }

    #[test]
    fn a_member_with_quiet_set_is_quiet_and_sends_nothing_whatever_its_kind() {
        for kind in MemberKind::ALL {
            let quiet = member(*kind, true);
            assert!(quiet.is_quiet(), "{kind}");
            let plain = member(*kind, false);
            assert_eq!(plain.is_quiet(), kind.is_quiet(), "{kind}");
            for ty in MessageType::ALL {
                assert!(!quiet.may_send(*ty), "{kind} {ty}");
                assert_eq!(plain.may_send(*ty), kind.may_send(*ty), "{kind} {ty}");
            }
        }
    }

    #[test]
    fn a_quiet_member_hears_only_its_own_lead() {
        let senders = [
            Actor::Lead,
            Actor::User,
            Actor::parse("api").unwrap(),
            Actor::parse("lead@beta").unwrap(),
            Actor::parse("db@beta").unwrap(),
        ];
        for kind in MemberKind::ALL {
            for stored in [false, true] {
                let m = member(*kind, stored);
                for sender in &senders {
                    let want = !m.is_quiet() || *sender == Actor::Lead;
                    assert_eq!(
                        m.hears(sender),
                        want,
                        "{kind} quiet={stored} from {sender:?}"
                    );
                }
            }
        }
        assert!(!member(MemberKind::Investigator, true).hears(&Actor::parse("api").unwrap()));
        assert!(member(MemberKind::Investigator, false).hears(&Actor::parse("api").unwrap()));
        assert!(member(MemberKind::Reviewer, false).hears(&Actor::Lead));
    }

    /// A reviewer stores `quiet` false, yet its JSON says true: a reader of
    /// the JSON never needs the list of quiet kinds.
    #[test]
    fn a_members_json_says_whether_it_is_quiet_in_effect() {
        for (kind, stored, shown) in [
            (MemberKind::Reviewer, false, true),
            (MemberKind::Reconciler, false, true),
            (MemberKind::Investigator, true, true),
            (MemberKind::Investigator, false, false),
            (MemberKind::Implementer, false, false),
        ] {
            let json = serde_json::to_value(member(kind, stored)).unwrap();
            assert_eq!(
                json.get("quiet"),
                Some(&serde_json::json!(shown)),
                "{kind} {stored}"
            );
            assert_eq!(json.get("set"), Some(&serde_json::json!("rev1")), "{kind}");
            assert_eq!(json.get("kind"), Some(&serde_json::json!(kind.as_str())));
        }
    }

    /// The JSON is written field by field, so a field missing from it would
    /// not come back. Every field here holds a value that is not its default.
    #[test]
    fn a_members_json_reads_back_as_the_same_member() {
        let m = member(MemberKind::Investigator, true);
        let back: Member = serde_json::from_value(serde_json::to_value(&m).unwrap()).unwrap();
        assert_eq!(back, m);
    }
}
