# ADR 0012: `teamwire install` writes the lead setup into Claude Code's user configuration

Status: accepted, 2026-10-10. Supersedes STO-12, SEC-3 and SEC-4 of
`docs/requirements.md` where they forbid every write to `~/.claude/` and
`~/.claude.json` and every skill or MCP server registered outside the
repository, and the sentence of the migration steps in
`docs/ARCHITECTURE.md` that says teamwire never registers the lead's MCP
server.

## Context

The lead needs two things in Claude Code's user configuration: the
`teamwire` skill, and an MCP server entry that runs `teamwire mcp --lead`.
Until now the user copied both by hand from `examples/`. teamwire never wrote
under `~/.claude/`, because the user's live Python team tooling ran from
there and a teamwire bug could break it.

The Python tooling has been retired. Copying by hand made the installed skill
fall out of step with the binary, and a hand-written MCP entry that runs
`teamwire` from `PATH` failed in sessions where `~/.cargo/bin` was not on
`PATH`.

## Decision

1. teamwire writes the **lead setup** (`CONTEXT.md`) only when the user runs
   `teamwire install` or `teamwire uninstall`. No other command, hook or
   server writes under `~/.claude/` or to `~/.claude.json`. Launch still
   passes each member's hooks and MCP server per run, and never registers
   them (ADR 0004, point 6).
2. The skill is compiled into the binary from
   `examples/skills/teamwire/SKILL.md`, so each binary installs the skill
   that matches its own commands. The installed copy carries a SHA-256 hash
   of its text in its frontmatter (`metadata.teamwire-sha256`). A copy whose
   text still matches its hash was written by teamwire and was not edited
   since.
3. teamwire never edits `.claude.json` itself. Claude Code rewrites that file
   from every running session, so a second writer could lose its changes or
   ours. teamwire reads the file and changes the MCP entry only through
   `claude mcp remove` and `claude mcp add --scope user`.
4. The MCP entry runs the absolute path of the binary that ran the install.
   It adds `--home DIR` only when the state directory is not the default.
   An install from inside a `target/` directory is refused, because it would
   register a development build.
5. A skill the user edited, or an MCP entry named `teamwire` that does not run
   `teamwire [--home DIR] mcp --lead`, is refused unless `--force` is given.
6. Every check runs before the first write. A failure after a write exits 2,
   says what was left behind, and asks for the same command again. Nothing
   is rolled back, because a rollback is one more write that can fail.

## Considered options

- Edit `.claude.json` directly. Rejected for the reason in point 3.
- Keep a copy of the installed skill in `$TEAMWIRE_HOME` to detect edits.
  Rejected because that check breaks when the state directory is moved or
  deleted. The hash travels with the file.
- Register the MCP entry as `teamwire` on `PATH`. Rejected for the `PATH`
  failure described above.

## Consequences

- `teamwire doctor` fails when the lead setup is missing, out of date or not
  teamwire's, and names `teamwire install` as the fix.
- After `cargo install`, the user runs `teamwire install` again to update the
  skill and the MCP entry.
- Tests point Claude's directory at a temporary one with
  `TEAMWIRE_CLAUDE_DIR` and use the fake `claude` binary. They never write
  under the real `~/.claude/`.
