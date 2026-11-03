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

A pi member's `model` names its provider, as `provider/id` from
`teamwire models`. pi lists a provider whenever it finds any credential for
it, even when the account cannot call that provider. For example, an
`AWS_PROFILE` in the environment makes pi list the Amazon Bedrock models,
and every call to one fails when the account's policy denies Bedrock. A
model pattern without a provider can resolve to such a provider. Leave the
provider out of teamwire's list with `TEAMWIRE_PI_EXCLUDE_PROVIDERS`, a
comma-separated list of pi provider names. teamwire reads it from the
environment first, then from a line in `$TEAMWIRE_HOME/config.toml`:

```toml
TEAMWIRE_PI_EXCLUDE_PROVIDERS = "amazon-bedrock"
```

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

`DIED <role>` (the session is gone with no reported completion), `BLOCKED <role>` (at
a dialog), `LATE <role> <age>` (past its deadline with nothing recorded),
`SILENT <role>` (quiet with no completion, so ask it) and
`TEAM-STALLED <roles>` (every remaining member stuck) report trouble to act
on. `STORE-ERROR <code>` means the watch could not read or write the team's
database, so run `teamwire doctor`. The other lines:

| Line | Meaning |
| --- | --- |
| `WATCHING n members` | The watch started. |
| `DECLARED <role>` | A completion is recorded while its turn still runs. The report lands at turn end. |
| `UNREPORTED <role>: ...` | The member recorded done and its turn ended, but its turn-end hook never reported. Run `teamwire doctor`, then read the member's report. The team keeps waiting. |
| `COMPLETED <role> (<status>) <summary> Result: <path>` | Your host cannot type results into your pane, so the watch reports them. Read the path. When the result file is missing or out of date, the path is the report, `reports/<role>.md`. |
| `UNDELIVERED <role> <reason> Result: <path>` | The hook did not type the result into your pane. This line is your only notice. Read the path, then run `teamwire received <team> <role>`, so `status` stops listing the result. |
| `UNCONFIRMED <role> ...` | Typing the result stalled. It may or may not be on your screen. Once you have the result, run `teamwire received <team> <role>`. |
| `QUESTION-UNDELIVERED <id> from <role>` or `QUESTION-UNCONFIRMED <id> from <role>` | A question to you could not be typed, or its typing stalled. `teamwire log` shows an excerpt, and the team's `comms/` directory holds the full text. |
| `CONTEXT <role> <pct>% <n>k of <window>k (handoff at <n>k)` | The member's context passed its handoff point. Once per generation. Wait for its `handoff:` completion, then run `teamwire handoff <team> --role <role>`. |
| `UNREAD <id> -> <role> <age>` | A message over 300 characters has waited unread 10 minutes since it was sent or resent, and the member has not reported `done`, `blocked` or `failed` since. `resend` refuses it, because the session already took it. Send a short `note` telling the member to read its inbox. |
| `UNANSWERED <id> from <role> <age>` | A question to you has waited 10 minutes. Answer with `--type answer --re <id>`. |
| `TEAM-CLOSED` | The team was closed before `TEAM-COMPLETE`, and the watch exited. If you did not close it, another session did, so run `teamwire status` before you act. |
| `TEAM-COMPLETE` | Every current member has stopped. `n/m done` counts only status `done` and names the others. `m` counts every member still in the team, including one that reported `failed`, and leaves out members refused or failed at launch and removed members. A later wave, a reviewer or the closer may still be due. |

## Other commands

| Command | Does |
| --- | --- |
| `teamwire export <team> [--out FILE]` | Every record as JSON lines. |
| `teamwire metrics [<team>] [--since DURATION] [--json]` | Local metrics from the event rows, including tokens per member. |
| `teamwire handoff <team> --role R [--dry-run]` | Gives a member a fresh session in the same seat, after it wrote its handoff document and recorded `done` with `handoff:`. Refuses a busy member, one at a dialog, or one with uncommitted work, with exit 1 and nothing changed. A missing or old handoff document gives a `warning:` line. Exit 2 after the old session was closed and the new one failed: run `teamwire reopen <team> --role R`. Exit 6 when the store cannot be read. `teamwire handoff --guide` prints the guide a member follows. |
| `teamwire doctor` | Read-only checks of the state, the disk, the tools and the plugin. Exit 0 when all pass, 2 on any `warn` or `fail` line. |
| `teamwire version` | The binary's version and the schema version it writes. |

When a member's context runs high: `teamwire status` shows `ctx` per member,
and the `CONTEXT` line says it passed its point (40% of its window, or the
`context_handoff_tokens` of the spec; no measurement backs the 40% for
Claude). The member writes `reports/<role>.handoff.md` and records `done` with
a summary that starts with `handoff:`. Then run `teamwire handoff <team> --role
<role>`. A failed handoff is recovered with `teamwire reopen <team> --role <role>`.

When a member sits at a dialog the launcher does not answer, read its pane
with the host's own tools, and do not retry in a loop. When a member is idle
for 30 minutes with no completion, run `teamwire status` and read its pane:
it may be running the full suite, or stuck on a failing command from its
brief. On a database problem, such as damage or a newer schema, run
`teamwire doctor`, then `teamwire verify <team> --deep`. `--repair-files`
rewrites `comms/` copies that are missing or differ from the database.
