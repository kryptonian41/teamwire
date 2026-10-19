//! The input of each tool: the shapes `tools/list` publishes as JSON schema
//! and the handlers read.

use rmcp::model::JsonObject;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
#[expect(
    dead_code,
    reason = "`team` is read from the raw arguments before the store opens; the field puts it in the schema"
)]
pub(crate) struct SendInput {
    /// The team. The lead names it; a member's own team is implied.
    pub(crate) team: Option<String>,
    /// Roles or "lead" in this team; "role@team" or "lead@team" in another team.
    pub(crate) to: Vec<String>,
    /// notice, finding, question, answer; the lead may also send reprompt and note,
    /// to its own team only. A lead sends no notice.
    #[serde(rename = "type")]
    pub(crate) kind: String,
    pub(crate) text: String,
    /// Required for a notice: the commit the notice is about.
    pub(crate) commit: Option<String>,
    /// Required for an answer: the display id of the question.
    pub(crate) re: Option<String>,
    /// An idempotency key: the same key never makes a second message.
    pub(crate) key: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[expect(
    dead_code,
    reason = "`team` is read from the raw arguments before the store opens; the field puts it in the schema"
)]
pub(crate) struct ResendInput {
    pub(crate) team: Option<String>,
    /// The display id of the message, such as m014.
    pub(crate) id: String,
    /// Roles to retry; default is every resendable recipient.
    pub(crate) to: Option<Vec<String>>,
}

#[derive(Deserialize, JsonSchema)]
#[expect(
    dead_code,
    reason = "`team` is read from the raw arguments before the store opens; the field puts it in the schema"
)]
pub(crate) struct InboxInput {
    pub(crate) team: Option<String>,
    /// Also return read messages, and mark nothing.
    #[serde(default)]
    pub(crate) all: bool,
}

#[derive(Deserialize, JsonSchema)]
#[expect(
    dead_code,
    reason = "`team` is read from the raw arguments before the store opens; the field puts it in the schema"
)]
pub(crate) struct AckInput {
    pub(crate) team: Option<String>,
    /// Display ids, such as m014.
    pub(crate) ids: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
#[expect(
    dead_code,
    reason = "`team` is read from the raw arguments before the store opens; the field puts it in the schema"
)]
pub(crate) struct DoneInput {
    pub(crate) team: Option<String>,
    /// done, blocked or failed.
    pub(crate) status: String,
    /// One line, up to 2000 characters.
    pub(crate) summary: String,
}

#[derive(Deserialize, JsonSchema)]
#[expect(
    dead_code,
    reason = "`team` is read from the raw arguments before the store opens; the field puts it in the schema"
)]
pub(crate) struct TeamOnly {
    /// The team. The lead names it; a member's own team is implied.
    pub(crate) team: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub(crate) struct MembersInput {
    /// The team. The lead names it; a member's own team is implied. Read from
    /// the raw arguments before the store opens; the field puts it in the schema.
    #[serde(rename = "team")]
    pub(crate) _team: Option<String>,
    /// Another team whose members to list, such as one found with `team_teams`.
    pub(crate) of: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[expect(
    dead_code,
    reason = "`team` is read from the raw arguments before the store opens; the field puts it in the schema"
)]
pub(crate) struct LogInput {
    pub(crate) team: Option<String>,
    /// Only messages to or from this role.
    pub(crate) role: Option<String>,
    /// At most this many rows; default 50.
    pub(crate) limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
#[expect(
    dead_code,
    reason = "`team` is read from the raw arguments before the store opens; the field puts it in the schema"
)]
pub(crate) struct DecisionsInput {
    /// The caller's team; the lead names it.
    pub(crate) team: Option<String>,
    /// The team to read, running or its newest archive. Default: the caller's own
    /// team, when `grep` is not given.
    pub(crate) of: Option<String>,
    /// Only this member.
    pub(crate) role: Option<String>,
    /// Only blocks whose text contains this, ignoring case. Reaches every running
    /// team and every archive unless `of` is given.
    pub(crate) grep: Option<String>,
    /// At most this many blocks; default 50. The result says `truncated` when more matched.
    pub(crate) limit: Option<usize>,
}

/// The input schema of a tool, for `tools/list`.
pub(crate) fn input_schema(tool: &str) -> Option<JsonObject> {
    fn of<T: JsonSchema>() -> Option<JsonObject> {
        schemars::schema_for!(T).as_object().cloned()
    }
    match tool {
        "team_send" => of::<SendInput>(),
        "team_resend" => of::<ResendInput>(),
        "team_inbox" => of::<InboxInput>(),
        "team_ack" => of::<AckInput>(),
        "team_done" => of::<DoneInput>(),
        "team_status" | "team_teams" => of::<TeamOnly>(),
        "team_members" => of::<MembersInput>(),
        "team_log" => of::<LogInput>(),
        "team_decisions" => of::<DecisionsInput>(),
        _ => None,
    }
}
