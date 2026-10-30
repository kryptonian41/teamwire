# teamwire architecture

Version 1 design, 2026-09-29. Requirements are in `docs/requirements.md`
(ids such as DEL-1, INV-7). Decisions and the options they beat are in
`docs/adr/`. The interfaces are in `docs/api/`.

## Overview

teamwire runs a team of coding agents for one lead agent. It is one Rust
binary, `teamwire`.

It does five jobs:

1. **Launch** members, each with a role, a brief, its own checkout when it
   writes code, and per-run configuration that points its hooks and MCP server
   back at teamwire.
2. **Carry messages** between members and the lead under fixed rules, with
   every message stored before any delivery attempt.
3. **Report completions**: a member says it is done; teamwire writes the
   result file and wakes the lead with one line, or says loudly that it
   could not.
4. **Watch** the team and print one line when something needs the lead.
5. **Verify and close**: run each member's check, flag weakened tests and
   out-of-scope edits, then archive and tear down in a safe order.

The user's priorities decide every trade-off, in this order:

| Priority | What it means here |
| --- | --- |
| Stability | No daemon. Every change is one SQLite transaction. Every external call has a timeout. A hook never fails the harness's turn. A failed step leaves state that the next command can read. |
| Message integrity | A message or completion is stored before delivery, has a unique id, and is never reported as read unless the recipient said so. A refusal or failure is recorded and shown, never dropped. |
| Metrics | Every state change writes an event row. Metrics are computed from those rows, shown by the CLI, and exported to OpenTelemetry by a separate step. |

teamwire is a separate project. It keeps all state under `$TEAMWIRE_HOME`
(default `~/.teamwire`), never writes under `~/.claude/` itself (except that
install deletes an older install's skill folder), and registers
nothing in the user's global configuration. The one exception is
`teamwire install`, which writes the teamwire plugin (the lead's skill in
`skills/teamwire/SKILL.md` and `skills/teamwire/reference.md`, MCP
server and Stop hook) to `$TEAMWIRE_HOME/claude-plugin/` and registers it
through `claude plugin` (ADR 0018).

## Components

One binary, one workspace, one crate per concern. The core names no harness
and no host (ADP-1).

| Crate | What it does |
| --- | --- |
| `teamwire-core` | Shared types (team, member, message, completion, event), the error codes, the `Harness`, `Host`, `Platform` and `Clock` traits, the capability record, the adapter registry keyed by string ids, and the text sanitiser. No I/O. |
| `teamwire-store` | Opens a team's SQLite database with the required settings, runs migrations, and offers typed transactions for every table. The only code that writes state. |
| `teamwire-platform` | Operating-system facts: pid liveness, a process's environment, running a command in its own process group with a timeout, atomic file writes, and the sleep hold (`caffeinate` on macOS). |
| `teamwire-host-herdr` | The `Host` trait for herdr, through herdr's JSON CLI. Also sets herdr labels once at launch. |
| `teamwire-host-tmux` | The `Host` trait for tmux. |
| `teamwire-harness-claude` | The `Harness` trait for Claude Code: launch argv, session files, `claude agents --json`, background-session detection, per-run `settings.json` and `mcp.json`, Stop and PostToolUse hook input, trust dialog. |
| `teamwire-harness-codex` | The `Harness` trait for the Codex CLI: launch and resume argv from `-c` overrides, the member's token file, Stop, PostToolUse and SessionStart hook input, the TUI screen, rollout files and thread locks, and the Codex lead plugin (ADR 0023). |
| `teamwire-harness-pi` | The `Harness` trait for pi: launch argv, the generated pi extension that runs the member hooks inside pi, the member's token file, the TUI screen, and session files for liveness and context fill (ADR 0029); the lead package that `teamwire install` adds, whose extension runs the lead's MCP server, Stop hook and watch, and the lead's session from the state file that extension writes (ADR 0030). |
| `teamwire-deliver` | Finds a session's current place (ADR 0006), types a line through the host with the retry rules of ADR 0003, and records the outcome. Formats and sanitises every typed line. |
| `teamwire-msg` | The six message types and their rules; send, inbox, ack and log. |
| `teamwire-report` | Completions (`done`), the report check, result files, delivery of results to the lead, and the harness-neutral part of the turn-end and in-turn hooks. |
| `teamwire-launch` | Spec validation, worktrees, setup, the contract text (data files, one per member kind), spawning members, the sleep hold. |
| `teamwire-verify` | Runs checks, diffs from the start commit, applies the per-language rules, writes verify records and waivers. |
| `teamwire-close` | Whole-team close and single-member removal, in the fixed order, with the archive. |
| `teamwire-watch` | The watch loop and the status view. Both derive each member's state the same way, except that status reads an idle `"dispatch": false` member quiet at once (WAT-3) and measures ages in wall time, because only a running watch measures sleeps. |
| `teamwire-otel` | Local metrics queries and the OpenTelemetry export. |
| `teamwire-mcp` | The stdio MCP server (`rmcp`), a thin front for `msg`, `report` and `watch`. |
| `teamwire-cli` | The `teamwire` binary: argument parsing, wiring concrete adapters, exit codes. It also runs `teamwire handoff` (ADR 0027): `teamwire-launch` and `teamwire-close` may not depend on each other, so the order of check, place close, open and record lives in `cmd/handoff.rs`. |
| `tests/fakes` (`teamwire-fakes`) | The fake host, harness and platform, the fake `herdr` and `claude` binaries, and the contract-test suites every adapter must pass. |

How they run (ADR 0001):

```
lead session (Claude Code in herdr)
  │ teamwire launch < spec.json ──► members, each in its own place, each with
  │                                  per-run hooks + a teamwire MCP server
  │ teamwire watch <team>   (under Monitor; one line per poll at most)
  │ teamwire send / status / verify / close
  │
  │           ┌──────────── $TEAMWIRE_HOME/teams/<team>/team.db ───────────┐
  │           │ members · messages · deliveries · completions · events ... │
  │           └────────────────────────────────────────────────────────────┘
  │                 ▲ every command and hook opens it, commits, exits
  │
member session ── teamwire mcp (stdio) ── team_send / team_inbox / team_done
               ── Stop hook → teamwire hook claude-stop → result file → lead
               ── PostToolUse hook → reminder of unread messages
```

Nothing in this picture is a daemon. The watch and each MCP server are
ordinary processes started by someone else and safe to kill.

## Data model

All state for a team sits in `$TEAMWIRE_HOME/teams/<team>/`:

| Path | Written by | Holds |
| --- | --- | --- |
| `team.db` (with `-wal`, `-shm`) | teamwire only | Every record below |
| `briefs/<role>.md` | launch | The brief as sent (a worktree member also gets a copy at `<worktree>/TASK.md`, listed as `/TASK.md` in the repository's `info/exclude` until the last worktree that needs it is closed) |
| `contract/<role>.md` | launch | The rendered system-prompt contract |
| `members/<role>/settings.json`, `mcp.json` | launch | Per-run hooks, the member's identity variables (`env`), for Claude the compaction backstop (`autoCompactEnabled`, `autoCompactWindow`, ADR 0027), the merged `plugins` map as `enabledPlugins`, always holding `teamwire@teamwire: false` so a member never loads the lead's plugin (ADR 0017, ADR 0018), and its MCP server. Mode 0600; they hold the token, so they are never written into a worktree |
| `setup/<role>.log` | launch | Output of the setup command |
| `comms/<display-id>.md` | send | Full message text (copy; the store has the original) |
| `outbox/<role>.md` | report | Result file for the newest completion, marked generated |
| `reports/<role>.md` | the member | The member's own long report |
| `verify/<role>.log` | verify | Full check output |
| `$TEAMWIRE_HOME/locks/exclude-<sha256>.lock` | launch, close | An empty file, made if missing and never deleted. Launch and close hold the kernel's `flock` on it while they rewrite `info/exclude`, waiting at most 10 s. The lock ends with the holder's open file, so a killed holder leaves nothing held |

The database tables (schema version 6; exact DDL is owned by the store
crate and listed in `docs/api/core.md`):

| Table | One row per | Key columns |
| --- | --- | --- |
| `team` | team (single row) | `ulid`, `name`, `title`, `state` (`open`, `closing`, `closed`), `closing_pid`, `closing_pid_started_at`, `lead_session_id`, `lead_harness`, `lead_host`, `lead_kind`, `lead_route` (`sink` or `watch`), `sleep_hold_pid`, `sleep_hold_owned`, `created_at`, `afk_budget_ends_at` (set only on an AFK run's team: when its budget ends) |
| `members` | member, never deleted | `role`, `title`, `kind`, `mode`, `harness`, `host`, `place`, `session_id`, `token_sha256`, `cwd`, `repo`, `branch`, `base`, `base_sha`, `check_cmd`, `owns`, `deadline_s`, `state` (`creating`, `started`, `failed`, `removed`), `generation` (a reprompt or a handoff raises it), `gen_started_at`, `gate_told_generation`, `removed_at`, `removed_note` |
| `messages` | message | `ulid`, `display_id` (unique), `sender` (`lead`, a role, or `role@team` / `lead@team` for another team's agent), `client_key` (unique with sender), `seq` (unique with sender), `type`, `re`, `commit_sha`, `body`, `body_sha256`, `created_at` |
| `deliveries` | message and recipient | `recipient` (as `sender`), `state` (`pending`, `handed_off`, `unconfirmed`, `undelivered`, `acked`), `attempts`, `route`, `error_code`, `error_detail`, `reminded_at`, `acked_at` |
| `completions` | completion | `ulid`, `role`, `generation`, `n` (unique with role), `status`, `summary`, `final_message`, `final_source`, `report_state` (`pending`, `delivering`, `delivered`, `undelivered`, `unconfirmed`), `delivering_since`, `route`, `reason`, `reason_detail`, `result_sha256` |
| `verify_records` | verify run per member | `role`, `head_sha`, `verdict`, `exit_code`, `seconds`, `flags`, `log_sha256` |
| `waivers` | waived flag | `role`, `flag_id`, `head_sha`, `reason` |
| `watch_lease` | team (single row) | `pid`, `host`, `started_at`, `heartbeat_at` |
| `announcements` | thing the watch said | `key` (such as `LATE:api:2`, generation 2; `SILENT:api:2:3`, episode 3), `at` |
| `counters` | named counter | `name` (`display_id`, `seq:<sender>`, `completion:<role>`, `watch.stalls_ended`), `value` |
| `events` | state change | `seq` (autoincrement), `ulid`, `time`, `type`, `role`, `subject`, `data` (JSON), `writer_version` (`<crate version>+<12 hex commit>` or `+unknown`; compared as text, never parsed) |
| `metric_export` | exporter | `last_event_seq`, `claimed_to`, `claimed_by`, `claimed_by_started_at`, `claimed_at` |

Rules that hold across the model:

- Ids are ULIDs. Short display ids (`m014`) and per-sender `seq` come from the
  `counters` table inside the insert transaction, never from counting rows.
- Nothing is deleted while the team is open. A removed member keeps its row.
  An undelivered message keeps its row with the reason.
- Every write transaction that changes lifecycle or message state also
  writes the event row for its change (OBS-1). Bookkeeping writes write no
  event: a member's place, session and token hash, a member's stored branch
  cleared by a retry, the generation a gate told, a completion's final
  message, the sleep-hold record, announcements, the watch lease heartbeat, and the metric export mark
  (an event for the mark would feed the export its own tail).
- Times are stored as UTC RFC 3339 strings with milliseconds. Timeouts
  inside one process use a monotonic clock.

## Message lifecycle and integrity

A message from `specmsg` (investigator) to `harness` (investigator), type
`finding`:

1. **Check.** `teamwire send` (or `team_send`) proves the sender (ADR 0008),
   then applies the rules: the type is one of six, the sender kind may send
   it, the recipient may receive it, the size is within the limit, a `notice`
   names a reachable commit, an `answer` answers a real question asked of the
   sender. A refusal records a `message.refused` event and exits 1.
2. **Store.** One transaction inserts the message (ULID, display id, `seq`,
   body, SHA-256), one `pending` delivery row per recipient, and the
   `message.sent` event. After commit, `comms/m014.md` is written atomically. If that write fails, the lead's typed
   line adds `(not written; run teamwire verify <team> --repair-files)` after
   the path.
3. **Locate.** For each recipient, outside any transaction, the locator asks
   the harness which live process runs the recipient's session and where it
   says it is, and asks the host who is in that place. Both must agree on one
   place (ADR 0006). A refusal marks the row `undelivered` with the reason.
4. **Type.** The host view is checked once more, then one sanitised line is
   submitted:
   `[teamwire:tw msg m014 finding from specmsg] <excerpt> ... Full text: teamwire inbox. <tail>`,
   or `<body>. <tail>` with no pointer when the whole body fits in the
   300-character excerpt (ADR 0021). Host success marks `handed_off`. A stall or timeout marks `unconfirmed`
   and is not retried. A "not submitted" code is tried at most 3 times (2
   retries) in about 5 seconds, then marks `undelivered`.
5. **Remind.** If the recipient was working, its PostToolUse hook reminds it
   of unread ids once; its Stop hook holds it once if it tries to stop with
   unread messages (ADR 0003, point 8). A `handed_off` message whose typed
   line held the whole body is not unread (ADR 0021). For the watch and
   `status` only, neither is one the member recorded a completion after
   (ADR 0022).
6. **Read.** The recipient calls `teamwire inbox` or `team_inbox`, which
   checks each body's hash, returns unread messages oldest first, and marks
   them `acked`.
7. **Report.** The sender's command exits 0 when every recipient is
   `handed_off`, 4 when any is `undelivered` (printing
   `NOT DELIVERED to <role>: <reason>`), 5 when any is `unconfirmed` and none
   is undelivered.

Completions follow the same pattern with the lead as recipient (ADR 0007):
the `done` call stores the completion; the member's Stop hook checks the
report once, writes the result file, marks `delivering`, locates the lead,
types one line, and records the outcome. A completion the lead could not
receive becomes `undelivered`, and the watch prints `UNDELIVERED` once with
the result path.

A message to another team's agent (`--to db@billing`) is stored twice under
one ULID (ADR 0016). The sender's copy is stored as in step 2, with a row for
`db@billing`. Then the sender writes billing's own copy into billing's store,
with billing's display id and a `pending` row for `db`. Steps 3 to 4 run
against billing's store, and the outcome is copied back onto the sender's
row. Each write is a transaction in one store, never two at once. A resend
from the sending team repeats the second step and everything after it.

What each guarantee rests on:

| Guarantee | How | Requirement |
| --- | --- | --- |
| A message is never lost silently | Stored before delivery; every outcome recorded; `status` lists `pending`, `unconfirmed` and `undelivered` | MSG-4, MSG-12 |
| A crash causes at most one duplicate | Idempotent key (sender, client key); completions marked `delivering` before the send | MSG-5, REP-2 |
| Ids never repeat | ULID plus counter rows under the write lock | MSG-4 |
| Gaps are visible | Per-sender `seq` assigned in the insert transaction | MSG-6 |
| Content is unchanged | SHA-256 checked on every read and by `verify` | MSG-7 |
| "Read" is never claimed falsely | Only the recipient's own `inbox` or `ack` sets `acked` | MSG-11 |
| No wrong-session typing | Two agreeing views, checked again right before the send | DEL-1..DEL-5, INV-1 |
| No forged lines | Every agent-supplied string is one printable line, capped | DEL-11, INV-20 |

One gap stays open, and it is named here: herdr cannot address a session by
id, only a pane. Between the final host check and the submit there is a
window of a few milliseconds in which the pane could change hands (DEL-4).

## Stability and failure handling

Principles, each with the requirement it serves:

- **One transaction per change.** A crash at any instruction leaves the old
  or the new state (STO-1..STO-3). Files outside SQLite are written through a
  temp file, a sync and a rename (STO-11).
- **No lock across an external call.** SQLite's write lock is held for
  milliseconds; host, harness and git calls happen between transactions
  (STO-4, MSG-8).
- **Every external call is bounded.** Host calls 30 s (`agent start` up to
  150 s), except during a delivery, where each host and harness call gets
  5 s and never more than the time left in the Stop hook's 40 s budget;
  harness listings 5 s, setup 600 s, git inside the report check 10 s,
  checks by the member's timeout (default 1800 s), each run in its own
  process group and killed as a group (DEL-14, VER-1).
- **Record before the irreversible step.** Launch records a member as
  `creating` before `git worktree add`. The host starts the agent inside its
  spawn call (herdr's `agent start`), so launch records the place right after
  that call returns, and marks `started` later. A crash inside the spawn call
  leaves a `creating` member with no place and a live agent. Close looks for
  the member's address among the host's live agents, and if it is there,
  refuses, names the agent to stop by hand, and keeps the state. Close finds
  everything else recorded, whatever step a crash hit (LCH-3).
- **Hooks never fail a turn.** Every hook path exits 0. When a member hook
  gives up before its work starts, it prints one `teamwire hook: <why>` line on
  stderr. A failure inside the turn-end work is logged on stderr as a warning.
  A store error with code busy, full, corrupt or io also leaves a `store.error`
  row when the store is reachable (REP-10). The lead's Stop hook stays silent
  (`docs/api/cli.md`).
- **Refuse rather than guess.** An unknown busy state refuses close; a
  missing view refuses delivery; a newer schema refuses to open (CLS-1, DEL-2,
  STO-6). `close --role --stop` still accepts an unknown busy state
  (ADR 0019 point 5). The host closes a place only when the agent in it carries
  the member's name, so a reused place id never closes another agent.
- **Distinct exit codes.** 0 success; 1 refused by a rule, nothing changed;
  2 partial (launch started some members, verify found a member not clean,
  close stopped at a step); 3 usage error; 4 not delivered; 5 unconfirmed;
  6 store error (busy past the timeout, disk full, newer schema).
- **Sleep and wake.** Nothing runs during sleep. The watch sees a poll gap
  longer than three intervals and treats it as a fresh start: counters reset,
  startup grace applies again (WAT-14).
- **Two versions at once.** Both open the same database. The older one
  refuses when the schema is newer than it knows. Every event row records the
  writer's version.
- **Full disk.** SQLite rolls the transaction back and teamwire exits 6 with
  a message that names the database file. Hooks still exit 0. `launch` refuses
  when the state directory's disk has less than 200 MB free, and `doctor`
  fails below the same 200 MB.

Every failure mode, how it is detected, what teamwire does and which test
proves it, is in `docs/failure-modes.md`.

## Metrics and observability

Every state change writes an event row in the same transaction (ADR 0005).
`teamwire metrics` answers questions such as "how many lead wake-ups did this
team cost", "how long did results take to reach the lead", "how many messages
went undelivered and why", "how often did the report check hold a member",
straight from those rows with no collector.

`teamwire metrics export` and the watch (every 60 s, on a background thread)
send the same data to an OpenTelemetry collector when
`OTEL_EXPORTER_OTLP_ENDPOINT` is set: counters and histograms for metrics,
and spans rebuilt from events for traces (one trace per team, with a
deterministic trace id). Each member's environment gets `teamwire.team` and
`teamwire.role` appended to `OTEL_RESOURCE_ATTRIBUTES`, so the harness's own
telemetry, when the user turns it on, carries the same labels. The full list
of metrics, spans and labels is in `docs/metrics.md`.

teamwire's own diagnostic logs go to stderr through `tracing`, filtered by
`TEAMWIRE_LOG`, with colour codes only when stderr is a terminal. Tokens and any variable named like a secret are never logged
(SEC-1).

## Harness and host adapters

Three traits keep the core free of product names (ADR 0004):

- **Harness**: the agent program. Claude Code and Codex CLI ship (ids
  `claude-code`, `codex`, ADR 0023) and pi (id `pi`, ADR 0029 for members,
  ADR 0030 for the lead).
- **Host**: where the program runs. herdr and tmux ship (ADR 0019). No host at all is a valid configuration. The lead's host
  is herdr when `HERDR_ENV=1`, else tmux when `TMUX` is set. When both are
  set, teamwire walks its own parent processes (own pid first, at most 64
  steps) and the nearest program named `tmux` or `herdr` decides; with
  neither found, herdr wins.
- **Platform**: the operating system. macOS and Linux.

Adapters are identified by string ids (`claude-code`, `codex`, `pi`, `herdr`,
`tmux`, `none`) and looked up in a registry that the CLI fills; the core has no list
of products. The harness and host belong to each member and to the lead, so a
Claude Code lead can lead a member on another harness that has an adapter, such
as Codex or pi (ADR 0004, ADP-12).

The core chooses routes from each adapter's capability record, never from
its name:

| Question the core asks | Capability | Claude Code in herdr | Claude Code in tmux | No host |
| --- | --- | --- | --- | --- |
| Can I wake this member by typing? | host `can_type` | yes | yes | no; `inbox` only |
| Can I find the lead's place from two views? | host `knows_session_of_place` or pid tree, plus harness processes | yes | yes (pid tree) | no; lead route is `watch` |
| Is there a turn-end hook for completions? | harness `turn_end_hook` | yes | yes | yes |
| Can the host tell working from idle? | host `reports_state`, else harness listing | host | harness listing | harness listing |

Claude Code specifics that the adapter owns:

- Members start with `claude -n tw_<team>_<role> --session-id <uuid>
  --permission-mode bypassPermissions --append-system-prompt-file
  contract/<role>.md --settings members/<role>/settings.json --mcp-config
  members/<role>/mcp.json [--model ...] [--effort ...]`. teamwire chooses the
  session id. The member's identity variables travel in the `env` key of that
  `settings.json`, because herdr cannot pass environment to the agent
  (ADR 0008).
- The lead's session id and kind come from `CLAUDE_CODE_SESSION_ID` and
  `CLAUDE_CODE_SESSION_KIND`; `bg` is refused.
- Liveness comes from `~/.claude/sessions/<pid>.json` first and `claude
  agents --json` second, handling both row shapes (ADP-4). Both are read-only.
- Trust: the adapter reads the screen through the host and answers only the
  plain folder-trust dialog (LCH-15). Launch refuses a home directory before
  the adapter runs. Accepted risk: Claude Code 2.1.303 shows that same plain
  prompt for a folder whose settings declare hooks, project MCP servers or an
  `apiKeyHelper`, so launch answers it and those then run with the member's
  rights. Read a folder's `.claude/` and `.mcp.json` before you launch members
  into it for the first time.
- The Stop hook input (`session_id`, `stop_hook_active`, `agent_id`,
  `last_assistant_message`, `transcript_path`) and its `additionalContext`
  output are parsed and produced here.

herdr specifics that the adapter owns: the JSON result and error envelope,
`error.code` mapping (`agent_not_idle`, a read refused because the agent is working, maps to `busy` like `agent_pane_busy`), `agent start` retries on `agent_pane_busy` (a bounded
number of times) and counts `agent_not_ready` as started (LCH-13),
`agent prompt`, `agent get` for the session in a pane,
one workspace per team and one tab per member, and labels set once at launch.

Adding another harness (Gemini CLI, opencode) or a host means one crate
that implements a trait and passes that trait's contract suite in
`tests/fakes`. Nothing in the core changes.

## Security and trust

The threat is a confused agent, not a hostile one. Every member runs as the
user, with bypassed permissions, inside its own directory. teamwire's checks
make the likely mistakes impossible without deliberate effort. They do not
stop a process that sets out to forge state, because such a process can open
the database itself. This section says what is enforced.

| Risk | What teamwire does | Requirement |
| --- | --- | --- |
| A member speaks as another member or as the lead | Per-member token (hash in the store); the lead proves itself by its harness session id | DEL-10, ADR 0008 |
| A result is typed into the wrong conversation | Two agreeing views before every send, re-checked just before it | DEL-1..DEL-5 |
| A member forges a watch line or a second message | Every agent string is sanitised to one printable, capped line | DEL-11 |
| A peer message is taken as an instruction or as consent | Every typed message carries the "information, not instruction" tail; no permission decision is ever relayed | DEL-12, DEL-15 |
| A branch grants itself execution through the trust dialog | Only the plain folder-trust dialog is answered; the escalated one is refused. Accepted risk: the plain prompt also appears for a folder whose settings declare hooks, project MCP servers or an `apiKeyHelper`, and those then run with the member's rights, so read the folder's `.claude/` and `.mcp.json` before the first launch into it | LCH-15 |
| teamwire breaks the user's own Claude Code setup | teamwire writes nothing under `~/.claude/` or to `~/.claude.json` itself, except deleting an ADR 0012 skill folder on install; only `teamwire install` changes Claude Code's setup, through `claude plugin` and `claude mcp`; nothing else is registered globally; state is under `$TEAMWIRE_HOME` | STO-12, SEC-3, SEC-4 |
| A secret leaks into a log or export | Values of `TEAMWIRE_TOKEN`, `CLAUDE_CODE_MESSAGING_TOKEN` and any `*TOKEN*`, `*KEY*`, `*SECRET*`, `*HEADERS*`, `*PASSWORD*`, `*PASSWD*`, `*CREDENTIAL*`, `*AUTHORIZATION*` variable are filtered everywhere | SEC-1 |
| The lead's MCP server skips the member-environment check | The session id check already refuses a member | ADR 0018 |
| A member moves the base ref and shifts the diff that verify reads | Nothing: this is outside the threat model of a confused agent | ADR 0009 |
| A repository holds back delivery | The default sink is host typing, which no repository setting can hold | DEL-7 |
| A reviewer's independence is lost | Reviewers and closers receive only from the lead | MSG-3 |

## Installing teamwire

The steps to set teamwire up on a machine, in order:

1. Install the binary (`cargo install --path crates/teamwire-cli`) and run
   `teamwire doctor`. It checks herdr's session hook, the database settings,
   the disk, and the `.worktrees/` ignore rule, and changes nothing.
2. Run the live checks by hand (`docs/test-strategy.md`, section 6). No test
   runs them.
   - The Claude adapter's check (`crates/teamwire-harness-claude/examples/live_check.rs`)
     proves the per-run `--settings` hooks and `--mcp-config` work with the
     installed Claude Code.
   - The host adapter's check for the host in use
     (`crates/teamwire-host-herdr/examples/live_check.rs` for herdr, or
     `crates/teamwire-host-tmux/examples/tmux_live_check.rs` for tmux).
3. Run `teamwire install`. It registers the teamwire plugin, which holds the
   lead's skill, MCP server and Stop hook (ADR 0018). Start a session, launch a throwaway team, and
   call `team_status` on it. A `refused.identity` reply that says no session
   id reached the server means the lead must use the CLI.
4. Run one throwaway team with teamwire in a scratch repository.

`teamwire uninstall` removes the skill and the MCP server entry again.
teamwire's state stays in `$TEAMWIRE_HOME` either way.

## Open questions for the user

Each one needs the user's preference or account. The design works with the
default shown, and nothing in the build plan waits on an answer.

| # | Question | Default in this design | Where it came from |
| --- | --- | --- | --- |
| 1 | Which OpenTelemetry backend, if any, and with which account | No endpoint; export off; local metrics only | brief, ADR 0005 |
| 2 | Is a warning dialog at every member start acceptable, to use Claude Channels push? | Channels not built; typing plus `inbox` | harness Q2, protocols Q6 |
| 3 | Should a message between members that fails to deliver wake the lead? | No. Only undelivered results and undelivered questions to the lead do; `status` lists the rest | specmsg Q4 |
| 4 | How long are records kept? | Until close; the archive keeps everything forever; no pruning | specmsg Q7 |
| 5 | Should planning rules (implementer without check, overlapping owns, more than five open branches) refuse a launch or only warn? | Warn, with an override flag | speclife Q7 |
| 6 | Who should notice when the lead session itself dies? | Nobody in v1; `status` shows lead liveness | speclife Q8 |
| 7 | Keep the stricter verify verdict (`FLAGGED` blocks member removal until waived)? | Yes | speclife Q1, ADR 0009 |
| 8 | Should v1 also ship adapters for Codex CLI, Gemini CLI or opencode? | Codex CLI yes (ADR 0023); pi yes (ADR 0029, ADR 0030); Gemini CLI and opencode no | harness Q6 |
| 9 | Is Linux (without a sleep hold) a supported platform for v1, or macOS only? | macOS fully; Linux without the sleep hold | rust Q7 |
| 10 | May the build install `cargo-nextest`, `cargo-deny` and `cargo-mutants`? They add per-test timeouts, a licence and advisory audit, and mutation scores | `cargo-nextest`: installed 2026-10-10, and the gate runs its tests with it from 2026-10-20 (`docs/conventions.md` §2). `cargo-deny` and `cargo-mutants`: not installed, so the gate has no licence audit (`docs/conventions.md` §2) | repopract Q1, R10, R14, R20 |
| 11 | Should this repository commit its own `.claude/settings.json` with edit-time hooks (format on edit, a guard on protected files) for agents working in it? | No; the gate and `xtask tidy` enforce the same rules after the fact | repopract Q2 |
