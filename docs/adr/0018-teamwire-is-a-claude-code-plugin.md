# ADR 0018: teamwire installs into Claude Code as a plugin with a lead Stop hook

Status: accepted, 2026-10-16. Supersedes points 1 and 2 of ADR 0012, point 4
of ADR 0017 for one plugin id, the sentence of ADR 0017 point 3 that says an
empty map writes no key, and the text of STO-12, SEC-3 and SEC-4 of
`docs/requirements.md`. Points 3 to 6 of ADR 0012 still hold.

## Context

The lead's watch runs under Claude Code's `Monitor` tool, which stops any
command after 30 minutes at most. A watch can also die on its own. Nothing
tells the lead that its watch is gone: the lead has no hook, `status` does
not show the watch, and an `UNDELIVERED` watch line is the lead's only notice
of a failed delivery. A survey of firstmate (2026-10-16) found the same gap
closed there with a Stop hook in the supervisor's session, after measured
blind spells of 30 and 40 minutes.

ADR 0012 let `teamwire install` write two things, the skill and the MCP
entry, and SEC-3 forbade any hook in the lead's session. A Claude Code
plugin carries skills, MCP servers and hooks as one unit, is installed and
removed with `claude plugin`, and can be switched off per session with
`enabledPlugins`.

## Decision

1. **One plugin.** `teamwire install` writes a local marketplace named
   `teamwire` to `$TEAMWIRE_HOME/claude-plugin/`, holding one plugin named
   `teamwire` at `$TEAMWIRE_HOME/claude-plugin/teamwire/`. Its id is
   `teamwire@teamwire`. It carries:
   - the skill `teamwire` (compiled into the binary as before, stamped with
     its SHA-256 as ADR 0012 point 2 describes);
   - the MCP server `teamwire`, running
     `<absolute binary> [--home DIR] mcp --lead`;
   - a Stop hook running `<absolute binary> [--home DIR] hook claude-lead-stop`.
   Claude Code names the lead's tools `mcp__plugin_teamwire_teamwire__<tool>`
   and the skill `teamwire:teamwire`.
2. **Registered only through `claude plugin`.** Install and uninstall change
   Claude Code's configuration only by running
   `claude plugin marketplace add|remove --scope user`,
   `claude plugin marketplace update` (which takes no scope),
   `claude plugin install|update|enable|uninstall --scope user`, and
   `claude mcp remove` for the migration below. Deleting the old skill folder
   in point 3 is the one file teamwire removes under `~/.claude/` itself. teamwire still never writes
   `~/.claude/settings.json`, `~/.claude.json` or Claude's plugin directory
   itself. The rules of ADR 0012 points 4 to 6 apply to the plugin: absolute
   binary path, no install from `target/`, every check before the first write.
3. **Migration.** Install removes the lead setup of ADR 0012: the stamped
   skill under `~/.claude/skills/teamwire/` and the user-scope MCP entry named
   `teamwire`. An edited skill or a foreign entry is refused unless `--force`
   is given, as before. Uninstall removes the plugin and the marketplace.
4. **Never in a member.** Launch writes `"teamwire@teamwire": false` into
   every member's `enabledPlugins`, whatever the spec says, so a member never
   loads the lead skill, the lead's MCP server or the lead's Stop hook. This
   is the one plugin id in Rust. The lead Stop hook also returns at once when
   `TEAMWIRE_TEAM` is set in its environment.
5. **What the lead Stop hook does.** It reads the hook input and returns
   nothing, exit 0, unless all of these hold for at least one team under the
   home: the team is open, its lead session is the hook's session, at least
   one member has started and has no completion for its current generation,
   and no live watch lease exists (`watch_lease_is_dead`). Then it holds the
   stop once, through the same hold reply the member hook uses, with one line
   per such team naming the command `teamwire watch <team>` and saying to run
   it under `Monitor`. It returns nothing when the stop is already a
   continuation (`stop_hook_active`) or comes from a subagent. It never starts
   a watch, never types into a pane and never retries. Every error returns
   nothing. It opens team stores read-only and writes nothing, because it
   runs on every stop in every session where the plugin is on. It records no
   event: a new event type needs a schema migration, and the hold is visible
   in the lead's transcript.
6. **The member hook says so too.** When a member's Stop hook types a result
   line to the lead and the team's watch lease is dead, the line ends with
   `(no watch is running: teamwire watch <team>)`. This reaches a lead that is
   idle, which the lead Stop hook cannot.

## Considered options

- Start the watch from the hook (firstmate's `asyncRewake` restart chain).
  Rejected: it needs a ledger, generation claims and a fail-open to stay
  alive, and Claude Code drops the wake when the hook times out.
- A hook only, with the skill and MCP entry kept as in ADR 0012. Rejected:
  two install paths, and members would still load the lead skill.
- Write the hook into `~/.claude/settings.json`. Rejected for the reason in
  ADR 0012 point 3: Claude Code rewrites its own files.

## Consequences

- The lead's skill is `/teamwire:teamwire`, and the skill's tool names change.
- After `cargo install`, the user runs `teamwire install` again, as before.
- `teamwire doctor` checks the plugin instead of the skill file and the MCP
  entry, and fails on a leftover ADR 0012 setup.
- Tests use the fake `claude` binary and `TEAMWIRE_CLAUDE_DIR`; they never run
  the real `claude plugin`.
