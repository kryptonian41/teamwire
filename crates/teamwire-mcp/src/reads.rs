//! The four tools that only read: `team_status`, `team_members`, `team_log`
//! and `team_teams`. The caller is proven in its own team first; a look at
//! another team opens that team's store read-only.

use rmcp::model::JsonObject;
use serde_json::json;
use teamwire_core::{Actor, TeamName};
use teamwire_msg::{LogFilter, roster, roster_of, teams};
use teamwire_store::Ctx;

use crate::failure::ToolFailure;
use crate::inputs::{LogInput, MembersInput};
use crate::tools::{Reply, parse, to_value};

/// Rows `team_log` returns when the caller gives no limit.
const DEFAULT_LOG_LIMIT: usize = 50;

pub(crate) fn run(
    ctx: &mut Ctx<'_>,
    team: &TeamName,
    actor: &Actor,
    tool: &str,
    args: JsonObject,
) -> Result<Reply, ToolFailure> {
    match tool {
        "team_status" => {
            let mut view = teamwire_watch::status_with(ctx, None);
            // The lead session id is what a member would need to speak as the
            // lead, so a member's view of the team leaves it out.
            if matches!(actor, Actor::Member(_)) {
                view.lead = None;
            }
            Ok(Reply {
                summary: format!("team {team}: {} member(s)", view.members.len()),
                value: to_value(&view)?,
            })
        }
        "team_members" => {
            let input: MembersInput = parse(args)?;
            let of = input
                .of
                .as_deref()
                .map(|t| TeamName::parse(t).map_err(|e| ToolFailure::invalid(format!("of: {e}"))))
                .transpose()?;
            let entries = match &of {
                Some(other) if other != team => roster_of(ctx, other)?,
                _ => roster(ctx)?,
            };
            Ok(Reply {
                summary: format!(
                    "team {}: {} member(s)",
                    of.as_ref().unwrap_or(team),
                    entries.len()
                ),
                value: json!({ "members": to_value(&entries)? }),
            })
        }
        "team_log" => {
            let input: LogInput = parse(args)?;
            let role = input
                .role
                .as_deref()
                .map(|r| Actor::parse(r).map_err(|e| ToolFailure::invalid(e.to_string())))
                .transpose()?;
            let rows = teamwire_msg::log(
                ctx,
                LogFilter {
                    viewer: actor.clone(),
                    role,
                    limit: Some(input.limit.unwrap_or(DEFAULT_LOG_LIMIT)),
                },
            )?;
            Ok(Reply {
                summary: format!("{} message(s)", rows.len()),
                value: json!({ "messages": to_value(&rows)? }),
            })
        }
        "team_teams" => {
            let entries = teams(ctx)?;
            Ok(Reply {
                summary: format!("{} team(s)", entries.len()),
                value: json!({ "teams": to_value(&entries)? }),
            })
        }
        other => Err(ToolFailure::invalid(format!("no tool named {other}"))),
    }
}
