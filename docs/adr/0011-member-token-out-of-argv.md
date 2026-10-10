# ADR 0011: A member's MCP server reads its token from its environment

Status: accepted, 2026-10-10. Supersedes the sentence of ADR 0008, decision
1, that puts the token in the MCP server's argv.

## Context

ADR 0008 put the member's token in the argv of its MCP server, in
`members/<role>/mcp.json`, because nobody had tested whether a stdio MCP
server inherits the session's environment. The files are mode 0600, but argv
is not private. Every process on the machine can read it with `ps`. A review
on 2026-10-10 saw three live member tokens that way.

An `mcp.json` server entry has its own `env` key, and Claude Code sets those
variables when it starts the server. That is the documented way to give a
stdio server its settings. The live check of 2026-10-10 showed both facts
with Claude Code: the server inherits the session's `settings.json`
environment, and a variable in the entry's `env` key takes precedence over
the inherited one.

## Decision

1. Launch writes the token into the `env` entry of the member's server in
   `mcp.json`, as `TEAMWIRE_TOKEN`. The argv keeps `--home`, `--team` and
   `--role`, and no longer carries `--token`.
2. `teamwire mcp --team T --role R` reads the token from `TEAMWIRE_TOKEN`
   when `--token` is absent. It still accepts `--token`, so an `mcp.json`
   written by an older launch keeps working until its team closes.
3. The Claude adapter's live check
   (`crates/teamwire-harness-claude/examples/live_check.rs`) gives
   `settings.json` a different token from `mcp.json`. It passes only when
   the server started from `mcp.json` sees the `mcp.json` token and its argv
   does not carry one.

## Consequences

- `ps` no longer shows a member's token.
- The token is still in `mcp.json` and in `settings.json`, both mode 0600.
  The threat model of ADR 0008 is unchanged: a member running as the same OS
  user can still read those files on purpose.
- A member server started with neither `--token` nor `TEAMWIRE_TOKEN` exits
  with a usage error and serves nothing.
