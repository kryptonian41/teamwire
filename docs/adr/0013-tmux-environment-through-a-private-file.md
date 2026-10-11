# ADR 0013: The tmux adapter passes a member's environment through a private file

Status: accepted, 2026-10-11. Supersedes the `new-window -e` mechanism that
ADR 0004 (capability table, `sets_env_at_spawn`) and ADR 0008 (decision 1
table) name for tmux. The capability itself is unchanged: tmux still sets
the environment when it starts the place.

## Context

ADR 0004 and ADR 0008 say tmux passes a member's environment with
`new-window -e NAME=VALUE`. That puts every value, including the member's
`TEAMWIRE_TOKEN`, in tmux's argv. Any local user can read argv with `ps`.
ADR 0011 already moved the token out of the MCP server's argv for the same
reason. A review on 2026-10-11 found that the tmux path had the same hole.
No launch used it yet, because the Claude adapter passes identity through
`settings.json`.

## Decision

1. Before `new-window`, the adapter writes the variables as `export` lines
   to a file named `env` in a new directory under the system temp directory,
   created with mode 0700. The file is written with `Platform::write_atomic`.
2. The window runs a `sh -c` wrapper that sources the file, deletes the
   directory, and then `exec`s the member's program. The program's argv is
   unchanged.
3. When the spawn fails before the wrapper runs, the adapter deletes the
   directory itself.
4. Variable names are limited to `[A-Za-z_][A-Za-z0-9_]*`, and values are
   quoted for `sh`.

## Consequences

- `ps` never shows the member's environment values for a tmux member.
- The values sit on disk, readable only by the user, from the spawn until
  the wrapper starts. That is the same exposure as the 0600 `settings.json`
  and `mcp.json` files that ADR 0011 accepts.
- A leftover `teamwire-tmux-env-*` directory means a program never started.
