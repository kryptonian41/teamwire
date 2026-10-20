# teamwire

teamwire runs a team of coding agents for one lead session and gets their
results back with no human present. This glossary names the concepts the
docs, errors and commands share.

## Language

**Lead setup**:
The teamwire plugin that `teamwire install` registers with the lead's
harness (ADR 0018, ADR 0023). The plugin carries the `teamwire` skill, the
`afk` skill (Claude Code only), `reference.md`, the lead's MCP server and the
lead's Stop hook.
_Avoid_: lead tools (those are the lead's MCP tools, such as `team_status`), install, integration
