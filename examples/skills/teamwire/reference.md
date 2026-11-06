# teamwire reference

This file holds the machine setup, the architecture notes, the watch lines and the other commands that `SKILL.md` points to.

Read this when:

- you set up a machine or troubleshoot an install: "Setting up a machine";
- a watch line you do not know arrives: "Watch lines";
- `send` refuses a message: "Messages";
- a member or a whole team you cannot reach: "Reopen";
- a member passes its handoff point or a handoff fails: "Context handoff";
- a `verify` flag you do not know: "Verify flags";
- a pr-watch line: "Pull request lines";
- a close that refused: "Close".

## Setting up a machine

This section describes Claude Code. A Codex or pi lead finds its own setup
in its part at the top of the skill.

teamwire never edits `~/.claude/settings.json` or `~/.claude.json`. Only
`teamwire install` and `teamwire uninstall` change Claude Code's setup,
through `claude plugin`, plus `claude mcp remove` and deleting an older
install's skill folder. Install writes one plugin, `teamwire@teamwire`, to
`$TEAMWIRE_HOME/claude-plugin/` and registers it at user scope. It carries
this skill, the lead's MCP server and the lead's Stop and tool hooks.

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

The watch joins what one poll found with ` | `. On herdr and tmux a
completion arrives through the Stop hook, so the watch prints nothing for a
result typed into your pane, and prints `DECLARED`, `UNDELIVERED` or
`UNCONFIRMED` only when delivery lags or fails. `date -u +%T` gives the
current UTC time, to compare with a line's time. `Monitor` stops a command
after 5 minutes unless you set `timeout_ms`.

One watch runs per team. A second refuses and names the first one's pid,
unless that process is gone or has not checked in for 10 minutes.
`teamwire watch <team> --once` runs one poll, but it needs the watch lease,
so it is refused while a watch runs. Use `teamwire status` then.

`DIED <role>` (the session is gone with no reported completion), `BLOCKED <role>` (at
a dialog), `LATE <role> <age>` (past its deadline with nothing recorded),
`SILENT <role>` (quiet with no completion, so ask it) and
`TEAM-STALLED <roles>` (every remaining member stuck) report trouble to act
on. `STORE-ERROR <code>` means the watch could not read or write the team's
database, so run `teamwire doctor`. The other lines:

| Line | Meaning |
| --- | --- |
| `WATCHING n members` | The watch started. This line never comes with `TEAM-COMPLETE`, so a watch you start beside a `launch --add` stays alive and picks the new member up. The poll reads the roster a moment before it prints, so a member whose row is written inside that moment is still missed (ADR 0037): run `teamwire status` when you need to be sure. |
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
| `TEAM-COMPLETE` | Every current member has stopped. `n/m done` counts only status `done` and names the others. `m` counts every member still in the team, including one that reported `failed`, and leaves out members refused or failed at launch and removed members. It comes on the poll after `WATCHING` at the earliest, so a team that was already finished when you started the watch takes one interval to say so. A later wave, a reviewer or the closer may still be due. While the team has an implementer and no closer has reported, the line ends `If no wave is due: add the integrator when branches join, then the closer (teamwire preset closer).` |

## Messages

`send` refuses a notice without a commit, a message over its limit, a
`reprompt` or `note` to another team's agent, a `reprompt` to a member whose
launch failed or that was removed, more than 5 messages to one recipient in one minute, the same
text to the same recipient within 10 minutes, a message to yourself or to a
removed member, and a message to a team that is closing, closed or unknown.

Each refusal names itself in the error code.

## Reopen

`teamwire reopen <team> [--role R] [--dry-run]` brings back a team whose
sessions were closed. It resumes the lead and each member whose session is
gone, each in a new tab, and leaves running sessions alone. `--role R`
reopens only that member and leaves the lead alone. `--dry-run` prints the
plan and opens nothing. Reopen types nothing, so a member killed mid-turn
sits idle until you send it a `reprompt` or a `note`. `host_no_session` with
"the session still runs (pid N)" follows a stop and resume of the member's
process, or a herdr without its session hook (`teamwire doctor` checks it).
After a stop and resume, end that process, then run
`teamwire reopen <team> --role <role>`.

## Context handoff

A member's answers get worse as its context window fills. teamwire turns
on auto-compact for every Claude Code member at twice its handoff point, so
the handoff note always comes first. `teamwire status`
shows each member's fill in the `ctx` column, for example `ctx 72% 720k`, and
`ctx -` when teamwire cannot read it. The watch prints one
`CONTEXT <role> <pct>% <n>k of <window>k (handoff at <n>k)` line per member
and generation once the member passes its handoff point.

The handoff point is 40% of the model's window unless the spec sets
`context_handoff_tokens`, a number of tokens, on the team or on a member.
A fresh Claude Code session already holds about 60,000 tokens, so set a number well above that. Launch warns below 100,000. No measurement backs the 40% for Claude. Measured studies see quality fall
in absolute tokens, so on a 1M window set the number yourself if 400k is too
high for the task.

Past its point, the member gets a note after its next tool call. The note tells
it to finish its step and, if its role writes code, to commit it. A role that
does not write code commits nothing. Then the member runs
`teamwire handoff --guide`, writes a handoff document at
`reports/<role>.handoff.md` and records `done` with a summary that starts
with `handoff:`. When that `done` arrives, run:

```bash
teamwire handoff <team> --role <role>
```

The command closes the member's old session and opens a fresh one in the same
worktree, on the same branch, with the same brief, report and token. The new
session starts a new generation and reads the brief and the handoff document.
The command refuses a member that is in a turn, at a dialog or has
uncommitted work, and changes nothing then. Wait for the turn to end and run
it again. It never stops a turn. When the handoff document is missing or
older than the member's session, the command still hands off: it prints a
`warning:` line and points the new session at the brief, the member's report
and `git log` instead. Read that warning, because the member may not have
finished writing. The new session's first line says how many unread
messages it has.

If the command fails after it closed the old session, run
`teamwire reopen <team> --role <role>` once the old session has ended. That
resumes the old session, with its full context. The command writes the old
session back first when it can. When the member had no recorded session, the
error says `reopen` has nothing to resume.

A Claude Code member that passes its point and gets no handoff compacts its
own context at twice that point. teamwire sets no compaction for Codex and pi
members, so hand them off at the `CONTEXT` line.

## Verify flags

Verify runs each `check` itself and records the real exit code. It flags
every change outside `owns`, with an extra flag when that change edits
lint, type, test or CI configuration. Even inside `owns`, it flags a
deleted or removed test and an added line that skips or silences a check.
It flags `no-commits` when an implementer said `done` with no commit
beyond its base, `no-owns` for an implementer with an empty `owns` list,
`uncommitted:<path>` for each uncommitted path, `asserts-removed:<path>`
for a file that lost assert lines, and `head-moved` when a commit landed
while the check ran. Verdicts are `PASS`, `FLAGGED`, `FAIL`, `NO-CHECK` and
`SKIPPED`. A brief may have asked for a flagged change, so read every
flag, and let no branch merge past one you have not read.
`teamwire verify <team> --role R --waive FLAG --reason TEXT`
waives one flag, named as verify prints it in brackets, at the member's
current HEAD. A later commit needs a new waiver. `uncommitted:<path>` is
one flag per path, so a waiver covers that path only. `head-moved` cannot
be waived: run verify again. `--deep` adds the database
integrity check and a check of every message hash.

A commit on a member's branch that the member did not make raises
`report-no-line:<sha>`, and `outside-owns:<path>` for each file outside
the member's `owns`. A merge of another member's branch into it raises
`outside-owns` for that branch's files, unless the member's `base` names
that branch, so set `base` on stacked branches. A member that merges a
branch its `base` does not name, such as a moved `develop` under a pull
request, gets an outside-owns flag for every file that merge brought in.
Have it make that merge a commit of its own with nothing else in it, and
name the merge's sha in its report. Then read its own commits with
`git log --no-merges --first-parent`, and waive the merge's flags with
the reason `brought in by merge <sha> of <branch>`.

## Pull request lines

`teamwire pr-watch` prints `WATCHING` first, then at most one line per poll: a check's
first failure, a first `CONFLICTING`, `BLOCKED` or `DRAFT` merge state,
and in the end a `DONE` line per pull request, or `MERGED` or `CLOSED`
when that happened first. A `DONE` line that ends with
`(merge state: ...)` names a state other than clean, such as `BLOCKED`, so
read it before anyone merges. One that adds `(required, never reported: ...)`
names required checks that never ran. After 3 polls with no answer from
`gh`, it prints `UNREADABLE` once per outage. A pull request or repo that
does not exist prints `NOT-FOUND` and counts as done and failed. It exits
when every pull request is done, with code 1 when a check failed, a pull
request was `NOT-FOUND`, or a required check never reported and the merge
state is not clean.

## Close

Close runs in the only safe order and stops at the first step that fails:

1. It refuses while a member is `busy`, at a dialog, or in a state it cannot
   read. To stop one, run `teamwire close <team> --role <role> --stop` first,
   adding `--unverified` when its verify or the closer has not passed, and
   tell the user plainly.
2. It refuses while a worktree has uncommitted changes. Commit them or copy
   them out, then run again. A worktree on a detached HEAD whose commits are
   on no branch gets the branch `teamwire/rescue/<team>-<role>-<sha7>`, and
   close prints it among the kept branches.
3. It archives the team (briefs, contracts, messages, results, reports, setup
   logs) to `$TEAMWIRE_HOME/archive/<team>-<date>`, or to
   `DIR/<team>-<date>` with `--archive-to DIR`, and adds a time to the name
   when it is taken. It verifies the copy before removing anything.
4. It removes worktrees and tabs, and keeps the branches. For parallel
   tickets the branch is the deliverable, so delete branches yourself only
   for throwaway teams.
5. It releases the sleep hold if a team started it and this was the lead
   session's last live team.
6. It deletes the team's state directory only when every step succeeded.
   `--keep-state` keeps it, marked closed.

Close prints the branches kept and any unpushed commits. A close that crashed
leaves a lock, and `teamwire doctor` reports it.
`teamwire close <team> --clear-stale-lock` clears a lock whose owner process
is gone, and nothing else. Run close again, and it continues from step 1.

## Other commands

| Command | Does |
| --- | --- |
| `teamwire decisions [<team>] [--role R] [--ref REF] [--grep TEXT] [--json]` | The "Decisions I made" blocks of members' reports, for one team, a ticket ref, or a text search across running teams and archives. Reads only. |
| `teamwire export <team> [--out FILE]` | Every record as JSON lines. |
| `teamwire metrics [<team>] [--since DURATION] [--json]` | Local metrics from the event rows, including tokens per member. After close, the team form reads the team's newest archive under `$TEAMWIRE_HOME/archive/`, but an archive put elsewhere with `--archive-to` is not read, so read it before you close in that case. The counter `teamwire.member.tokens` has the labels `team`, `role`, `kind`, `model`, `effort`, `task_class` (`unclassified` when the spec set none) and `type` (`input`, `output`, `cache_read`, `cache_creation`). A team whose results reach you as `COMPLETED` watch lines, because your host cannot type into your pane, records no tokens. |
| `teamwire handoff <team> --role R [--dry-run]` | Gives a member a fresh session in the same seat, after it wrote its handoff document and recorded `done` with `handoff:`. Refuses a busy member, one at a dialog, or one with uncommitted work, with exit 1 and nothing changed. A missing or old handoff document gives a `warning:` line. Exit 2 after the old session was closed and the new one failed: run `teamwire reopen <team> --role R`. Exit 6 when the store cannot be read. `teamwire handoff --guide` prints the guide a member follows. |
| `teamwire doctor` | Read-only checks of the state, the disk, the tools and the plugin. Exit 0 when all pass, 2 on any `warn` or `fail` line. |
| `teamwire version` | The binary's version and the schema version it writes. |

When a member sits at a dialog the launcher does not answer (`BLOCKED`),
read its pane with the host's own tools, and do not retry in a loop. A
permission prompt that the harness denies on its own after a timeout needs
nothing. A login, update or trust dialog needs the user: tell the user which
member and which tab, even before `TEAM-STALLED`. When a member is idle
for 30 minutes with no completion, run `teamwire status` and read its pane:
it may be running the full suite, or stuck on a failing command from its
brief. On a database problem, such as damage or a newer schema, run
`teamwire doctor`, then `teamwire verify <team> --deep`. `--repair-files`
rewrites `comms/` copies that are missing or differ from the database.
