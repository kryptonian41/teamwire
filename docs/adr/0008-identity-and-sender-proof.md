# ADR 0008: Who is speaking — member tokens and lead proof

Status: accepted, 2026-09-29.

## Context

`team-msg.py` refuses `--from X` unless the process is in X's recorded pane,
and refuses `--from lead` inside any member's pane (specmsg A7). The check is
skipped when `HERDR_PANE_ID` is unset, so any process outside a pane can speak
as anyone (A8, L9). The Stop hook finds a member by pane id or session id, so a
different session in a member's old pane can be taken for it (L11). harness R9
asks for identity passed at spawn, not inferred from the host, because tmux and
headless runs have no `HERDR_PANE_ID`.

## Decision

1. **Every member gets a random 128-bit token at launch.** The store keeps
   only its SHA-256. The token and `TEAMWIRE_TEAM`, `TEAMWIRE_ROLE`,
   `TEAMWIRE_HOME` reach the member through the first identity channel that
   the member's adapters offer, in this order:

   | Channel | Capability | v1 adapters |
   | --- | --- | --- |
   | The harness's per-session configuration file | harness `session_env_via_config` | Claude Code: the `env` key of the per-member `settings.json` passed with `--settings` |
   | The host sets the environment when it starts the place | host `sets_env_at_spawn` | tmux (`new-window -e`); herdr cannot (`herdr agent start --help`, 2026-09-29: only `--kind`, `--pane`, `--timeout` and agent args) |
   | Neither | | Launch refuses the member with `no_identity_channel` before creating anything |

   The host's own shell environment is never assumed to reach the agent:
   herdr types the start command into a shell it started itself, so
   teamwire's environment does not reach it. The lead checked on 2026-09-29
   that a Claude session started with `--settings <file>` whose file holds
   `{"env": {...}}` shows the value both in the Bash tool and in a Stop hook
   defined in the same file.

   The configuration files that hold the token (`members/<role>/settings.json`
   and `members/<role>/mcp.json`) are written with mode 0600 under
   `$TEAMWIRE_HOME/teams/<team>/`, never in the member's worktree. The MCP
   server also gets the token in its argv from `mcp.json`, because whether a
   stdio MCP server inherits the session's environment is untested (harness
   Q4). The hook commands in `settings.json` carry `--team` and `--role` in
   their argv as well.
2. **A member's call is accepted only with a token whose hash matches its
   role.** `--from` defaults to `TEAMWIRE_ROLE`. A `--from` naming another
   role, or a missing or wrong token, is refused with `refused.identity`.
3. **The lead proves itself by its harness session.** `--from lead` is
   accepted only when the calling process has no `TEAMWIRE_TOKEN` and the
   harness reports a current session id (Claude: `CLAUDE_CODE_SESSION_ID`)
   equal to the lead session id recorded at launch. When no session id can be
   read, the call is refused (DEL-10). A new lead session takes over a team
   with `teamwire lead --adopt <team>`, run from that session. It prints the
   old and new session ids and records the change as an event.
4. **Hooks match by two facts.** The turn-end hook acts only when the hook
   input's `session_id` equals the member's recorded session id and
   `TEAMWIRE_ROLE` names that member (REP-11).
5. **Tokens never appear in logs, events, errors or exports** (SEC-1). The
   same filter hides `CLAUDE_CODE_MESSAGING_TOKEN` and any variable whose name
   contains `TOKEN`, `KEY` or `SECRET`.

## Consequences

- The check works the same under herdr, tmux or no host.
- The threat this stops is a confused agent, not a hostile one. Every member
  runs as the same OS user with bypassed permissions. It could read another
  member's `mcp.json` or the database itself. The check makes the easy mistake
  (speaking as another member or as the lead) impossible without deliberate
  effort. The architecture says this plainly.
- The user typing `teamwire send --from lead` in a plain terminal is refused.
  The user speaks through the lead session, or adopts the team from a new one.
- A lead resumed with `claude --resume` keeps its session id and needs no
  adoption.
