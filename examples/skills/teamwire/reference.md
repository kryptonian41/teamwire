# teamwire reference

This file holds the machine setup, the architecture notes, the watch lines and the other commands that `SKILL.md` points to.

## Setting up a machine

teamwire never edits `~/.claude/settings.json` or `~/.claude.json`. Only
`teamwire install` and `teamwire uninstall` change Claude Code's setup,
through `claude plugin`, plus `claude mcp remove` and deleting an older
install's skill folder. Install writes one plugin, `teamwire@teamwire`, to
`$TEAMWIRE_HOME/claude-plugin/` and registers it at user scope. It carries
this skill, the lead's MCP server and the lead's Stop hook.

1. Install the binary (`cargo install --locked --path crates/teamwire-cli`)
   and run `teamwire doctor`, which changes nothing. Until step 3 it prints a
   `FAIL` line saying the teamwire plugin is not installed, and one for an
   older install's skill or MCP entry if there is one. Step 3 clears both.
   Fix any other `FAIL` line, such as `.worktrees/` not ignored or a binary
   missing from `PATH`.
2. Run the live checks by hand (`docs/test-strategy.md`, section 6). The
   Claude adapter's check
   (`crates/teamwire-harness-claude/examples/live_check.rs`) proves the per-run
   `--settings` hooks and `--mcp-config` work with the installed Claude Code:
   `cargo run -p teamwire-harness-claude --example live_check --locked`. Then
   run the host's check: inside a herdr pane,
   `cargo run -p teamwire-host-herdr --example live_check --locked`, or for
   tmux, `cargo run -p teamwire-host-tmux --example tmux_live_check --locked`.
3. Run `teamwire install`. It registers the plugin, and removes an older
   install's skill and MCP server entry. Start a new Claude Code session,
   because an open session does not load the plugin.
4. Run a throwaway team in a scratch repository, and call `team_status` on it
   from the lead's session. A reply that is not refused shows the lead's MCP
   server sees `CLAUDE_CODE_SESSION_ID`, so the lead's calls are accepted.
   Until then, use the CLI.
5. Check that `teamwire doctor` reports the teamwire plugin as `ok`.

After a new `cargo install`, run `teamwire install` again. `teamwire uninstall`
removes the plugin, its marketplace, `$TEAMWIRE_HOME/claude-plugin/` and any
skill or MCP entry of an older install, and leaves team state in
`$TEAMWIRE_HOME`.

## How it fits together

```
lead (you)
  │  teamwire launch ── setup ── brief ──►  member in its own tab
  │                                               │
  │                                    member calls `teamwire done`
  │                                               │
  │                                      Stop hook delivers the result
  │  ◄──── typed into your pane (wakes you) ──────┘
```

## Watch lines

`DIED <role>` (the session is gone with no completion), `BLOCKED <role>` (at
a dialog), `LATE <role> <age>` (past its deadline with nothing recorded),
`SILENT <role>` (quiet with no completion, so ask it) and
`TEAM-STALLED <roles>` (every remaining member stuck) report trouble to act
on. `STORE-ERROR <code>` means the watch could not read or write the team's
database, so run `teamwire doctor`. The other lines:

| Line | Meaning |
| --- | --- |
| `WATCHING n members` | The watch started. |
| `DECLARED <role>` | A completion is recorded while its turn still runs. The report lands at turn end. |
| `COMPLETED <role> (<status>) <summary> Result: <path>` | Your host cannot type results into your pane, so the watch reports them. Read the path. |
| `UNDELIVERED <role> <reason> Result: <path>` | The hook did not type the result into your pane. This line is your only notice. Read the path, then run `teamwire received <team> <role>`, so `status` stops listing the result. |
| `UNCONFIRMED <role> ...` | Typing the result stalled. It may or may not be on your screen. Once you have the result, run `teamwire received <team> <role>`. |
| `QUESTION-UNDELIVERED <id> from <role>` or `QUESTION-UNCONFIRMED <id> from <role>` | A question to you could not be typed, or its typing stalled. `teamwire log` shows an excerpt, and the team's `comms/` directory holds the full text. |
| `UNREAD <id> -> <role> <age>` | A message over 300 characters has waited unread 10 minutes since it was sent or resent, and the member has not reported `done`, `blocked` or `failed` since. `resend` refuses it, because the session already took it. Send a short `note` telling the member to read its inbox. |
| `UNANSWERED <id> from <role> <age>` | A question to you has waited 10 minutes. Answer with `--type answer --re <id>`. |
| `TEAM-CLOSED` | The team was closed before `TEAM-COMPLETE`, and the watch exited. If you did not close it, another session did, so run `teamwire status` before you act. |
| `TEAM-COMPLETE` | Every current member has stopped. `n/m done` counts only status `done` and names the others. `m` counts every member still in the team, including one that reported `failed`, and leaves out members refused or failed at launch and removed members. A later wave, a reviewer or the closer may still be due. |

## Other commands

| Command | Does |
| --- | --- |
| `teamwire decisions [<team>] [--role R] [--ref REF] [--grep TEXT] [--json]` | The "Decisions I made" blocks of members' reports, for one team, a ticket ref, or a text search across running teams and archives. Reads only. |
| `teamwire export <team> [--out FILE]` | Every record as JSON lines. |
| `teamwire metrics [<team>] [--since DURATION] [--json]` | Local metrics from the event rows, including tokens per member. |
| `teamwire doctor` | Read-only checks of the state, the disk, the tools and the plugin. Exit 0 when all pass, 2 on any `warn` or `fail` line. |
| `teamwire version` | The binary's version and the schema version it writes. |

When a member sits at a dialog the launcher does not answer, read its pane
with the host's own tools, and do not retry in a loop. When a member is idle
for 30 minutes with no completion, run `teamwire status` and read its pane:
it may be running the full suite, or stuck on a failing command from its
brief. On a database problem, such as damage or a newer schema, run
`teamwire doctor`, then `teamwire verify <team> --deep`. `--repair-files`
rewrites `comms/` copies that are missing or differ from the database.
