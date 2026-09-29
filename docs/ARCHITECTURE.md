# teamwire architecture

Version 1 design, 2026-09-29. Requirements are in `docs/requirements.md`
(ids such as DEL-1, INV-7). Decisions and the options they beat are in
`docs/adr/`. The interfaces are in `docs/api/`.

## Overview

teamwire runs a team of coding agents for one lead agent. It replaces the
Python team tooling (`team-launch.py`, `team-watch.py`, `team-status.py`,
`team-verify.py`, `team-close.py`, `team-msg.py`, `pr-watch.py` and the Stop
hook `team-report.sh`) with one Rust binary, `teamwire`.

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
(default `~/.teamwire`), never writes under `~/.claude/`, and registers nothing
in the user's global configuration. The Python tooling keeps running beside it
until the user decides to switch.

## Components

One binary, one workspace, one crate per concern. The core names no harness
and no host (ADP-1).

| Crate | What it does |
| --- | --- |
| `teamwire-core` | Shared types (team, member, message, completion, event), the error codes, the `Harness`, `Host`, `Platform` and `Clock` traits, the capability record, the adapter registry keyed by string ids, and the text sanitiser. No I/O. |
| `teamwire-store` | Opens a team's SQLite database with the required settings, runs migrations, and offers typed transactions for every table. The only code that writes state. |
| `teamwire-platform` | Operating-system facts: pid liveness, a process's environment, running a command in its own process group with a timeout, atomic file writes, and the sleep hold (`caffeinate` on macOS). |
| `teamwire-host-herdr` | The `Host` trait for herdr, through herdr's JSON CLI. Also sets herdr labels once at launch. |
| `teamwire-host-tmux` | The `Host` trait for tmux (wave 5). |
| `teamwire-harness-claude` | The `Harness` trait for Claude Code: launch argv, session files, `claude agents --json`, background-session detection, per-run `settings.json` and `mcp.json`, Stop and PostToolUse hook input, trust dialog. |
| `teamwire-deliver` | Finds a session's current place (ADR 0006), types a line through the host with the retry rules of ADR 0003, and records the outcome. Formats and sanitises every typed line. |
| `teamwire-msg` | The six message types and their rules; send, inbox, ack and log. |
| `teamwire-report` | Completions (`done`), the report check, result files, delivery of results to the lead, and the harness-neutral part of the turn-end and in-turn hooks. |
| `teamwire-launch` | Spec validation, worktrees, setup, the contract text (data files, one per member kind), spawning members, the sleep hold. |
| `teamwire-verify` | Runs checks, diffs from the start commit, applies the per-language rules, writes verify records and waivers. |
| `teamwire-close` | Whole-team close and single-member removal, in the fixed order, with the archive. |
| `teamwire-watch` | The watch loop and the status view. Both derive each member's state the same way. |
| `teamwire-otel` | Local metrics queries and the OpenTelemetry export. |
| `teamwire-mcp` | The stdio MCP server (`rmcp`), a thin front for `msg`, `report` and `watch`. |
| `teamwire-cli` | The `teamwire` binary: argument parsing, wiring concrete adapters, exit codes. |
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
| `briefs/<role>.md` | launch | The brief as sent |
| `contract/<role>.md` | launch | The rendered system-prompt contract |
| `members/<role>/settings.json`, `mcp.json` | launch | Per-run hooks, the member's identity variables (`env`), and its MCP server. Mode 0600; they hold the token, so they are never written into a worktree |
| `setup/<role>.log` | launch | Output of the setup command |
| `comms/<display-id>.md` | send | Full message text (copy; the store has the original) |
| `outbox/<role>.md` | report | Result file for the newest completion, marked generated |
| `reports/<role>.md` | the member | The member's own long report |
| `verify/<role>.log` | verify | Full check output |

The database tables (schema version 1; exact DDL is owned by the store
crate and listed in `docs/api/core.md`):

| Table | One row per | Key columns |
| --- | --- | --- |
| `team` | team (single row) | `ulid`, `name`, `title`, `state` (`open`, `closing`, `closed`), `closing_pid`, `lead_session_id`, `lead_harness`, `lead_host`, `lead_kind`, `lead_route` (`sink` or `watch`), `sleep_hold_pid`, `sleep_hold_owned`, `created_at` |
| `members` | member, never deleted | `role`, `title`, `kind`, `mode`, `harness`, `host`, `place`, `session_id`, `token_sha256`, `cwd`, `repo`, `branch`, `base`, `base_sha`, `check_cmd`, `owns`, `deadline_s`, `state` (`creating`, `started`, `failed`, `removed`), `generation`, `gen_started_at`, `gate_told_generation`, `removed_at`, `removed_note` |
| `messages` | message | `ulid`, `display_id` (unique), `sender`, `client_key` (unique with sender), `seq` (unique with sender), `type`, `re`, `commit_sha`, `body`, `body_sha256`, `created_at` |
| `deliveries` | message and recipient | `state` (`pending`, `handed_off`, `unconfirmed`, `undelivered`, `acked`), `attempts`, `route`, `error_code`, `error_detail`, `reminded_at`, `acked_at` |
| `completions` | completion | `ulid`, `role`, `generation`, `n` (unique with role), `status`, `summary`, `final_message`, `final_source`, `report_state` (`pending`, `delivering`, `delivered`, `undelivered`, `unconfirmed`), `route`, `reason`, `result_sha256` |
| `verify_records` | verify run per member | `role`, `head_sha`, `verdict`, `exit_code`, `seconds`, `flags`, `log_sha256` |
| `waivers` | waived flag | `role`, `flag_id`, `head_sha`, `reason` |
| `watch_lease` | team (single row) | `pid`, `host`, `started_at`, `heartbeat_at` |
| `announcements` | thing the watch said | `key` (such as `LATE:api:2`), `at` |
| `counters` | named counter | `name` (`display_id`, `seq:<sender>`), `value` |
| `events` | state change | `seq` (autoincrement), `ulid`, `time`, `type`, `role`, `subject`, `data` (JSON), `writer_version` |
| `metric_export` | exporter | `last_event_seq` |

Rules that hold across the model:

- Ids are ULIDs. Short display ids (`m014`) and per-sender `seq` come from the
  `counters` table inside the insert transaction, never from counting rows.
- Nothing is deleted while the team is open. A removed member keeps its row.
  An undelivered message keeps its row with the reason.
- Every write transaction also writes the event row for its change (OBS-1).
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
   `message.sent` event. After commit, `comms/m014.md` is written atomically.
3. **Locate.** For each recipient, outside any transaction, the locator asks
   the harness which live process runs the recipient's session and where it
   says it is, and asks the host who is in that place. Both must agree on one
   place (ADR 0006). A refusal marks the row `undelivered` with the reason.
4. **Type.** The host view is checked once more, then one sanitised line is
   submitted:
   `[teamwire:tw msg m014 finding from specmsg] <excerpt> ... Full text: teamwire inbox. <tail>`.
   Host success marks `handed_off`. A stall or timeout marks `unconfirmed`
   and is not retried. A "not submitted" code is retried at most 3 times in
   about 5 seconds, then marks `undelivered`.
5. **Remind.** If the recipient was working, its PostToolUse hook reminds it
   of unread ids once; its Stop hook holds it once if it tries to stop with
   unread messages (ADR 0003, point 8).
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
  150 s), harness listings 5 s, setup 600 s, checks by the member's timeout
  (default 1800 s), each run in its own process group and killed as a group
  (DEL-14, VER-1).
- **Record before the irreversible step.** Launch records a member as
  `creating` before `git worktree add`, records the place before the agent
  starts, and marks `started` after. Close finds everything recorded, whatever
  step a crash hit (LCH-3).
- **Hooks never fail a turn.** Every hook path exits 0. Errors go to stderr
  and to an event row when the store is reachable (REP-10).
- **Refuse rather than guess.** An unknown busy state refuses close; a
  missing view refuses delivery; a newer schema refuses to open (CLS-1, DEL-2,
  STO-6).
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
  the path and free space. Hooks still exit 0. `launch` refuses when the
  state directory's disk has less than 200 MB free, and `doctor` warns below
  1 GB.

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
`TEAMWIRE_LOG`. Tokens and any variable named like a secret are never logged
(SEC-1).

## Harness and host adapters

Three traits keep the core free of product names (ADR 0004):

- **Harness**: the agent program. Claude Code is the v1 implementation.
- **Host**: where the program runs. herdr is the v1 implementation; tmux
  follows in wave 5. No host at all is a valid configuration.
- **Platform**: the operating system. macOS and Linux.

Adapters are identified by string ids (`claude-code`, `herdr`, `tmux`,
`none`) and looked up in a registry that the CLI fills; the core has no list
of products. The harness and host belong to each member and to the lead, so a
Claude Code lead can lead a member on another harness once that harness has
an adapter (ADR 0004, ADP-12).

The core chooses routes from each adapter's capability record, never from
its name:

| Question the core asks | Capability | Claude Code in herdr | Claude Code in tmux | No host |
| --- | --- | --- | --- | --- |
| Can I wake this member by typing? | host `can_type` | yes | yes | no; `inbox` only |
| Can I find the lead's place from two views? | host `knows_session_of_place` or pid tree, plus harness processes | yes | yes (pid tree) | no; lead route is `watch` |
| Is there a turn-end hook for completions? | harness `turn_end_hook` | yes | yes | yes |
| Can the host tell working from idle? | host `reports_state`, else harness listing | host | harness listing | harness listing |

Claude Code specifics that the adapter owns:

- Members start with `claude -n <team>-<role> --session-id <uuid>
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
- Trust: `~/.claude.json` is read (never written) for
  `hasTrustDialogAccepted`; when trust is missing the adapter reads the screen
  through the host and answers only the plain folder-trust dialog (LCH-15).
- The Stop hook input (`session_id`, `stop_hook_active`, `agent_id`,
  `last_assistant_message`, `transcript_path`) and its `additionalContext`
  output are parsed and produced here.

herdr specifics that the adapter owns: the JSON result and error envelope,
`error.code` mapping, `agent start` retries on `agent_pane_busy` and
`agent_not_ready`, `agent prompt`, `agent get` for the session in a pane,
one workspace per team and one tab per member, and labels set once at launch.

Adding a harness (Codex CLI, Gemini CLI, opencode) or a host means one crate
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
| A branch grants itself execution through the trust dialog | Only the plain folder-trust dialog is answered; the escalated one is refused | LCH-15 |
| teamwire breaks the running Python setup | Nothing is written under `~/.claude/` or to `~/.claude.json`; nothing is registered globally; state is under `$TEAMWIRE_HOME` | STO-12, SEC-3, SEC-4 |
| A secret leaks into a log or export | Values of `TEAMWIRE_TOKEN`, `CLAUDE_CODE_MESSAGING_TOKEN` and any `*TOKEN*`, `*KEY*`, `*SECRET*` variable are filtered everywhere | SEC-1 |
| A repository holds back delivery | The default sink is host typing, which no repository setting can hold | DEL-7 |
| A reviewer's independence is lost | Reviewers and closers receive only from the lead | MSG-3 |

## Migration and cutover

teamwire is built and tested while the Python tooling keeps running the
user's real teams. Whether and when to switch is the user's decision; this
section only makes the switch possible and safe.

What makes side-by-side running safe:

- Separate state: teamwire uses `$TEAMWIRE_HOME` (default `~/.teamwire`);
  Python uses `~/.claude/team/`. Neither reads the other's teams.
- Separate hooks: teamwire's hooks are per member, passed with `--settings`.
  The Python Stop hook still runs for teamwire members, because it is
  registered globally, but it exits at its first check: it finds no Python
  manifest that lists the member's pane or session.
- Separate lines: teamwire's wake-up and watch lines start with
  `[teamwire:<team>]`; Python's start with `[team:<team>]`.
- Separate commands: the contract text given to teamwire members names only
  `teamwire` commands and MCP tools.

The steps a cutover would take, in order, when the user asks for it:

1. Install the binary (`cargo install --path crates/teamwire-cli`) and run
   `teamwire doctor`. It checks herdr's session hook, the database settings,
   the disk, and the `.worktrees/` ignore rule, and changes nothing.
2. Run the Claude adapter's manual live check (`crates/teamwire-harness-claude/examples/live_check.rs`),
   which proves the per-run `--settings` hooks and `--mcp-config` work with the
   installed Claude Code. The user runs it by hand; no test does.
3. Run one throwaway team with teamwire in a scratch repository.
4. Let every running Python team finish and close under Python.
5. Point the lead's skill at teamwire: install the `teamwire` skill from
   `examples/skills/teamwire/` (a file copy the user makes).
6. Only then, remove `team-report.sh` from `~/.claude/settings.json`. The user
   does this by hand; teamwire never edits that file.

Rolling back is the same list in reverse. teamwire's state stays in
`$TEAMWIRE_HOME` either way.

## Open questions for the user

Each one needs the user's preference or account. The design works with the
default shown, and nothing in the build plan waits on an answer.

| # | Question | Default in this design | Where it came from |
| --- | --- | --- | --- |
| 1 | Whether and when to cut over from the Python tooling | Not scheduled. Both run side by side. | brief |
| 2 | Which OpenTelemetry backend, if any, and with which account | No endpoint; export off; local metrics only | brief, ADR 0005 |
| 3 | Is a warning dialog at every member start acceptable, to use Claude Channels push? | Channels not built; typing plus `inbox` | harness Q2, protocols Q6 |
| 4 | Should a message between members that fails to deliver wake the lead? | No. Only undelivered results and undelivered questions to the lead do; `status` lists the rest | specmsg Q4 |
| 5 | How long are records kept? | Until close; the archive keeps everything forever; no pruning | specmsg Q7 |
| 6 | Should planning rules (implementer without check, overlapping owns, more than five open branches) refuse a launch or only warn? | Warn, with an override flag | speclife Q7 |
| 7 | Who should notice when the lead session itself dies? | Nobody in v1; `status` shows lead liveness | speclife Q8 |
| 8 | Keep the stricter verify verdict (`FLAGGED` blocks member removal until waived)? | Yes | speclife Q1, ADR 0009 |
| 9 | Should v1 also ship adapters for Codex CLI, Gemini CLI or opencode? | No; the trait and fakes only | harness Q6 |
| 10 | Is Linux (without a sleep hold) a supported platform for v1, or macOS only? | macOS fully; Linux without the sleep hold | rust Q7 |
| 11 | May the build install `cargo-nextest`, `cargo-deny` and `cargo-mutants`? They add per-test timeouts, a licence and advisory audit, and mutation scores | Not installed; the gate uses `cargo test` and has no licence audit (`docs/conventions.md` §2) | repopract Q1, R10, R14, R20 |
| 12 | Should this repository commit its own `.claude/settings.json` with edit-time hooks (format on edit, a guard on protected files) for agents working in it? | No; the gate and `xtask tidy` enforce the same rules after the fact | repopract Q2 |
