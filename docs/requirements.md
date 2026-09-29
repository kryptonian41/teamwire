# teamwire requirements

This list merges the five wave-1 investigation reports of team `tw`
(2026-09-29) into one numbered set. Every item is **MUST** or **SHOULD** and
names its source. The architecture, the failure-mode table and the build plan
cite these ids.

## How to read the sources

| Short name | Report | Its own ids |
| --- | --- | --- |
| specmsg | Messaging side of the Python tooling | A1-A13, B1-B11, C1-C5, D1-D8, E1-E16, L1-L15, R1-R46 |
| speclife | Lifecycle side of the Python tooling | L1-L25, C1-C11, V1-V8, W1-W13, F1-F17, LC-1..LC-60, N1-N10 |
| harness | Harness and terminal-host paths | R1-R21 |
| protocols | Prior art: MCP, A2A, ACP, CloudEvents | R1-R28 |
| rust | Rust stack, SQLite experiments | requirements 1-29 |

`specmsg R5` means requirement R5 in the specmsg report. `speclife F5` means
fragility F5 in the speclife report. A date names a real incident recorded in
the Python code, the `agent-team` skill, or the archived team data.

Words used below:

- **Lead**: the agent session that launched the team and integrates its work.
- **Member** (or child): an agent session teamwire launched as part of a team.
- **Harness**: the agent program, such as Claude Code.
- **Host**: the terminal program the agent runs in, such as herdr or tmux.
- **Sink**: the way teamwire puts text into a running session.
- **Completion**: a member's statement that it finished, with a status.
- **Result file**: the file teamwire writes for each completion (the Python
  `outbox/<role>.md`).

A MUST means that losing the behaviour repeats a real incident, or loses or
misdelivers data. A SHOULD means a measured cost or a clear improvement with
no incident behind it.

## Invariants

These behaviours exist because of a real incident. None of them may regress.
Each one has a test named in `docs/failure-modes.md`.

| Id | Behaviour that must hold | Incident | Requirements |
| --- | --- | --- | --- |
| INV-1 | A result is typed into the lead's session only after teamwire confirms, at delivery time, that the lead's session is the one in that place. Otherwise nothing is typed and the result is queued. | 2026-09-14: eleven reports were typed into an unrelated conversation that had taken over the lead's pane (specmsg A1, A3). | DEL-1, DEL-2, DEL-3 |
| INV-2 | The lead is found by its session id, not by the process id or pane recorded at launch. A resumed lead still receives results. | `claude --resume` keeps the session id and starts a new process; the old pid check refused a live lead (specmsg A2). | DEL-1 |
| INV-3 | A launch from a background session is refused before anything is created. | 2026-09-14, twice: a daemon-forked session inherited another pane's `HERDR_PANE_ID`, so every report went to someone else's conversation (specmsg A6, speclife L1). | DEL-6, LCH-1 |
| INV-4 | The default delivery channel is one a repository setting cannot hold back. | 2026-09-24 test: with `crossSessionInbound: "hold"` in a repo, `SendMessage` was held and never shown, while `herdr agent prompt` was acted on in 4 s (specmsg A10, harness R6). | DEL-7 |
| INV-5 | A delivery that timed out or stalled is recorded as unconfirmed and never resent automatically, and the state it caused is kept. | herdr documents that `agent_prompt_stalled` may still have delivered; blind resends duplicate (specmsg A11, speclife L20). | DEL-8, LCH-14 |
| INV-6 | A completion is reported once. A later turn with the same completion reports nothing. | Before the reported marker existed, every later turn re-notified the lead (specmsg B2). | REP-2 |
| INV-7 | A completion that reaches no one, or may not have, is never silent. It is queued with a reason and the watch prints one line for it: `UNDELIVERED` (including a delivery the hook never finished) or `UNCONFIRMED`. | brandpalette, 2026-09-15: four reports sat undelivered in `inbox/` for five days (specmsg E3; files under `~/.claude/team-archive/brandpalette-2026-09-15/inbox/`). | REP-4, REP-14 |
| INV-8 | The watch never types into any session. | The watch once retyped a queued line into the same pane one poll later, which undid the hook's refusal (specmsg E4). | DEL-16 |
| INV-9 | The watch prints at most one line per poll and announces each event once per occurrence. | LATE re-fired on every poll: five identical lines in 143 s cost 13.6M tokens (specmsg E8, `team-watch.py:402-405`). | WAT-1, WAT-2 |
| INV-10 | A member is not called quiet during a 90-second startup grace unless it has been seen working. | prreview, 2026-09-11: every child of a fresh team was declared SILENT, then TEAM-STALLED, within two minutes while all three worked (specmsg E6). | WAT-3 |
| INV-11 | Completion is decided from teamwire's own records and process liveness, never from the host's done or idle state. | `herdr agent focus` (the user's prefix+a) turns `done` into `idle`, so pressing it consumed the signal (specmsg E11). | WAT-5 |
| INV-12 | `TEAM-COMPLETE` counts only status `done` and names every member that failed, blocked or died. | "3/3 finished cleanly" was reported for a team where every child had failed (specmsg E12). | WAT-6 |
| INV-13 | The watch re-reads membership every poll. Added members are watched. Removed members and closed teams are not reported as dead. | nvimrev, 2026-09-29: a closed team's children were reported as DIED; an added child went unwatched while TEAM-COMPLETE fired (specmsg E13). | WAT-7 |
| INV-14 | A reprompt starts a new generation for that member. The watch re-arms it and its next completion is a new result. | A finished member's extra turn went unwatched (specmsg B3, E14). | REP-3, WAT-8 |
| INV-15 | The final message in a result comes from the harness at stop time. The transcript on disk is only a fallback. | 2026-09-29: three of five result files got a mid-work line because the transcript lagged the turn (specmsg C1). | REP-7 |
| INV-16 | teamwire owns the result file. An agent-written file found at that path is moved to the agent's own directory and linked, never overwritten. | A task told a child to write into `outbox/`; the hook overwrote it and the "final message" was "I wrote it to that file". Cost two full investigations (specmsg C2). | REP-8 |
| INV-17 | The report check holds a member at most once per stop, and delivery still happens on the next stop. | The check once bailed out on a continuing stop and dropped the report of a child another hook had held (specmsg B5). | REP-5 |
| INV-18 | A member whose report is missing progress lines or its decisions list is told once, with the facts. | 2026-09-23: three of three Haiku children skipped the report; with the hook note, two of two wrote it (specmsg B6). | REP-5 |
| INV-19 | Progress-line checks count only the member's own non-merge, first-parent commits since it started. | A merged contract amendment made a child invent lines; 2026-09-29, a rebase made a child write 187 lines (specmsg B7). | REP-6 |
| INV-20 | A member-supplied string reaches another agent's input or the event stream only as one printable, length-capped line. | A child could forge a whole `TEAM-STALLED` line, which pages the user (specmsg A12). Rated non-negotiable by specmsg though no forgery was observed. | DEL-11 |
| INV-21 | Failure to list live sessions is "no information", never "every member died". `DIED` needs three consecutive absent polls. | Recorded in `team-watch.py:408-422`: a failed listing would have ended the watch (specmsg E7). | WAT-4 |
| INV-22 | The contract and any multi-line text are passed to a harness as a file, never as a command-line argument. | herdr refused multi-line args with `invalid_agent_argument` (speclife L15). | LCH-12 |
| INV-23 | Worktrees are made with git directly, inside the repo, not with the host's worktree command. | `herdr worktree create` made an idle repo workspace on 17 of 41 creations; herdr's default root has no trusted ancestor (speclife L9). | LCH-8 |
| INV-24 | A coding member's environment is prepared by the team's `setup` command before the agent starts. A failed setup refuses that member and undoes what was made for it. | Every brief since 2026-09-09 carried a symlink script; on 2026-09-14 the lead had to wire worktrees by hand (speclife L12). | LCH-11 |
| INV-25 | Only a plain folder-trust dialog is answered automatically. A dialog that also grants folder-declared commands is refused. | Trusted-folder testing recorded in SKILL.md "Trust": the same button grants a branch's own hooks and helpers (speclife L17). | LCH-15 |
| INV-26 | The sleep hold is released when the last team on that lead closes, and only if a team started it. | Before 2026-09-24 nothing released it; a hold stayed for hours and drained the battery (speclife C7). | SLP-1 |
| INV-27 | The sleep hold waits on the lead's real harness process, not on the shell that ran the launcher. | `caffeinate -w` on the throwaway Bash-tool shell exited at once and held nothing (speclife L22). | SLP-2 |
| INV-28 | A member is removed only after a passing, current verify record and after the closer reported, unless the lead overrides. | 2026-09-29: four teams left finished members open 15-51 minutes; removing early loses verification, since verify skips removed members (speclife C8, C9). | CLS-5 |
| INV-29 | verify runs each member's check itself and records the real exit code. | A child reported `node --test test/` passing, a command that cannot work on Node 24 (speclife V1). | VER-1 |
| INV-30 | verify diffs from the member's start commit, moved forward past a merged contract amendment. | A merged CONTRACT AMENDMENT would otherwise be flagged as an edit outside `owns` (speclife V2). | VER-2 |
| INV-31 | Close refuses while a member is busy or while busy state is unknown, refuses over uncommitted work, archives and verifies the archive before destroying anything, and never forces. | "The team directory is the only copy of what the team produced" (speclife C1-C6). | CLS-1, CLS-2, CLS-3 |
| INV-32 | teamwire never takes a name the running Python tooling can use. Its host agent names are `tw_<team>_<role>`, which contain no hyphen, while every Python name is `<team>-<role>` and always contains one. Launch also refuses a name already live in the host. | User's hard rule: teamwire must not break the running Python setup (`CLAUDE.md`). `team-msg.py` types into members by herdr agent name (`team-msg.py:321-324`); a shared name would let Python type into a teamwire member or make the teamwire launch fail (archrev 7). herdr names must match `[a-z][a-z0-9_-]{0,31}` and be unique among live agents (`herdr --skill`, read 2026-09-29). | LCH-6, LCH-25 |

## Requirements

### Storage and crash safety (STO)

| Id | Level | Requirement | Source |
| --- | --- | --- | --- |
| STO-1 | MUST | One SQLite database is the single source of truth for a team: members, messages, deliveries, completions, events and metrics. No other file is needed to answer "what happened". | rust 1; speclife F1, F2 |
| STO-2 | MUST | Every connection sets `journal_mode=WAL`, `synchronous=FULL`, `foreign_keys=ON` and `busy_timeout >= 5000`. A test reads each value back. | rust 2; SQLite pragma docs |
| STO-3 | MUST | Every write transaction starts with `BEGIN IMMEDIATE`. | rust 3; rust's upgrade experiment failed at 0.00 s despite a 5 s timeout |
| STO-4 | MUST | No write transaction is open during a call to a host, harness or other external program. | rust 4; specmsg L3, R27 |
| STO-5 | MUST | Member state is one row per member, changed inside a transaction. Nothing rewrites a whole team document. Two processes updating different members lose neither update. | rust 17; speclife F1, F2, LC-55 |
| STO-6 | MUST | The schema carries a version (`PRAGMA user_version`) and a fixed `application_id`. Migrations run forward in one transaction. A binary refuses a database newer than it knows, names both versions, and changes nothing. | rust 13; speclife F15, LC-54 |
| STO-7 | MUST | Each process opens its own connection. No process forks with a connection open. | rust 16; SQLite "how to corrupt" |
| STO-8 | MUST | The database lives on a local disk. `teamwire doctor` warns when `TEAMWIRE_HOME` is on a network filesystem. | rust 20 |
| STO-9 | MUST | Copies of a live database use SQLite's backup API or `teamwire export`, never a plain file copy. | rust 19 |
| STO-10 | MUST | Human-readable copies exist beside the database: each message body as `comms/<id>.md`, each result as `outbox/<role>.md`. They are written after the commit, atomically, and the database holds their SHA-256. `teamwire verify --repair-files` rewrites a missing or different copy. | rust 18; protocols R9; speclife LC-57 |
| STO-11 | MUST | Every machine-read file outside SQLite is written to a temp file, synced and renamed into place. | speclife LC-3, LC-32 |
| STO-12 | MUST | All state is under `$TEAMWIRE_HOME` (default `~/.teamwire`). teamwire never writes under `~/.claude/` or to `~/.claude.json`. | `CLAUDE.md`; speclife LC-56, N1; harness R7 |
| STO-13 | MUST | `teamwire doctor` runs `PRAGMA quick_check`; `teamwire verify --deep` runs `PRAGMA integrity_check`. | rust 14 |
| STO-14 | SHOULD | `teamwire export` writes every record as JSON lines with CloudEvents-style names. | rust 18; protocols R12 |
| STO-15 | MUST | A corrupt or unreadable record is reported and skipped. It never blocks other records and is never read as success. | protocols R15; specmsg L2, L6; speclife F5 |
| STO-16 | MUST | Every command works with no background process running. | rust 15 |

### Delivery safety (DEL)

| Id | Level | Requirement | Source |
| --- | --- | --- | --- |
| DEL-1 | MUST | Before typing into the lead's session, teamwire finds the lead by its session id at that moment, from at least two independent sources that must agree. It never uses an address recorded at launch. A resumed lead in a new process or place still receives. | specmsg R1, R2, A1, A2; INV-1, INV-2 |
| DEL-2 | MUST | When the sources disagree, one is missing, or the lookup fails, teamwire types nothing and records the item as undelivered with a machine-readable reason. | specmsg R3, A3 |
| DEL-3 | MUST | A session open in two places is refused. Two processes claiming one session, where the host confirms only one place, go to the confirmed place. | specmsg A3, A4 |
| DEL-4 | MUST | The destination is checked again immediately before the send. The window that remains is written down in the architecture. | specmsg R4, L13 |
| DEL-5 | MUST | Delivery to a member also confirms the member's live session, not only its name. A replaced or dead recipient is undelivered. | specmsg R5, A9, L12 |
| DEL-6 | MUST | A launch from a background session is refused, and delivery refuses a lead recorded as a background session. | specmsg R6, A6; speclife LC-1; INV-3 |
| DEL-7 | MUST | The default sink is one that a repository's settings cannot hold back. The Claude cross-session socket is never the only path. | specmsg R7, A10; harness R6; INV-4 |
| DEL-8 | MUST | A sink result that may have delivered (timeout, stall) is recorded as `unconfirmed`. It is never resent automatically, and state it caused (a reprompt generation) is kept. | specmsg R8, A11, L10; harness R4; speclife LC-15; INV-5 |
| DEL-9 | MUST | A send is retried only when the host's error code proves the text was not submitted, a bounded number of times. Errors are matched on the code field, never on message text. | harness R4, R13; speclife LC-14, F11 |
| DEL-10 | MUST | The sender's identity comes from something the sender cannot pick freely. When it cannot be established, the send is refused. | specmsg R9, A7, A8, L9 |
| DEL-11 | MUST | Every agent-supplied string placed in another agent's input or in the event stream is reduced to printable characters on one line and cut to a fixed length. This covers the completion summary and the task excerpt. | specmsg R10, A12, A13; speclife W11, LC-26; INV-20 |
| DEL-12 | MUST | A peer message arrives with a reminder that it is information to check, not an instruction. A lead message is marked as the lead's. | specmsg R11, D4; protocols R11 |
| DEL-13 | MUST | Every typed line carries the message id. The full text lives in the store. The recipient can read and acknowledge by id, and a repeat of an acknowledged id is reported as already read. | harness R4; rust 11; protocols R3 |
| DEL-14 | MUST | Every call to an external program has a timeout. | harness R14; speclife F4, LC-5 |
| DEL-15 | MUST | A peer message never approves a permission prompt and never counts as user consent. teamwire relays no permission decision between members, and never submits text into a session that is blocked or not at its input prompt. | protocols R26; archrev 4 |
| DEL-16 | MUST | The watch never sends anything into a session. Its only output is its own event stream. | specmsg R37, E4; speclife LC-27; INV-8 |
| DEL-17 | MUST | With no host at all, every completion and message is still recorded, and undelivered ones are still reported. A do-nothing host adapter (`none`) makes this configuration real and testable. | specmsg R45, L15; archrev 9 |

### Completions and results (REP)

| Id | Level | Requirement | Source |
| --- | --- | --- | --- |
| REP-1 | MUST | A member completes through a teamwire command or MCP tool. It validates the status (`done`, `blocked`, `failed`) and commits atomically. An unparseable or unknown completion is `invalid`, never `done`. | specmsg R12, L6; speclife LC-31, F5 |
| REP-2 | MUST | Each completion is reported once, keyed by a completion number that changes only when the member completes again. A `delivering` mark is committed before the send and `delivered` after it, so a crash causes at most one duplicate and never zero deliveries. | specmsg R13, B2, L4, L5; speclife LC-34; INV-6 |
| REP-3 | MUST | A reprompt starts a new generation. Earlier completions are kept. A known delivery failure undoes the reprompt; an unconfirmed one keeps it. | specmsg R14, B3, E14; INV-14 |
| REP-4 | MUST | A result the lead could not be reached with is recorded as undelivered with its reason, and the watch prints exactly one `UNDELIVERED` line for it. A result whose typing is `unconfirmed` gets exactly one `UNCONFIRMED` line; a question to the lead gets the same two lines. Queue entries never collide. | specmsg R15, E3, L7; INV-7; archrev 2 |
| REP-14 | MUST | The turn-end hook finishes its delivery inside a fixed budget (40 s) that sits below the hook timeout written into the member's per-run settings (60 s). A completion left in `delivering` longer than 90 s (the hook was killed) is marked `undelivered` with `delivery_interrupted` by the watch, which prints one `UNDELIVERED` line. | archrev 1; Python reported this case as `DECLARED` (`team-report.sh:326-333`) |
| REP-5 | MUST | The report check holds a member at most once per stop, never when the harness says a stop hook already held this stop, and treats its own errors as "no gaps". Delivery happens on the next stop whatever the report says. | specmsg R16, B5, B6, B8; speclife LC-33; INV-17, INV-18 |
| REP-6 | SHOULD | The progress-line check counts only the member's own commits: non-merge, first-parent, authored after the member started minus 120 s. | specmsg R17, B7; speclife LC-33; INV-19 |
| REP-7 | MUST | The final message in the result is the one the harness hands over at stop time. The transcript is a fallback, and the result file says which one was used. | specmsg R18, C1; INV-15 |
| REP-8 | MUST | teamwire owns the result file and marks it as generated. Anything unmarked at that path is moved to the member's own directory and linked, never deleted. Members write long output to their own report file. | specmsg R19, C2, C3; speclife LC-35; INV-16 |
| REP-9 | MUST | The result file is written, atomically, before delivery is attempted. | specmsg R20, C5 |
| REP-10 | MUST | The turn-end hook never fails the harness turn. It exits 0 on every path, returns at once for sessions that are not team members, and ignores subagent stops. | specmsg R21, B9, B10; speclife LC-37 |
| REP-11 | MUST | A finished member is matched by its session id and its team identity together. A different session in the same place is not the member. | specmsg R22, L11 |
| REP-12 | MUST | The wake-up line is short: role, status, a summary of at most 200 characters, and the result file path. | specmsg C4 |
| REP-13 | MUST | With a harness that has no turn-end hook, a completion is still reported to the lead, through the watch. | speclife Q2; harness 1.4 |

### Messages between members (MSG)

| Id | Level | Requirement | Source |
| --- | --- | --- | --- |
| MSG-1 | MUST | Six message types with fixed sender and recipient rules and size limits: `notice` (implementer to members, 600 chars), `finding` (investigator to members, 800), `question` (implementer or investigator to the lead or an investigator, 800), `answer` (to the asker only, 800), `reprompt` and `note` (lead only, no limit). Refusals state their reason. | specmsg R23, D1, D3; protocols R10 |
| MSG-2 | MUST | A `notice` names a commit that exists in the sender's checkout and is reachable from its current head. | specmsg R24 |
| MSG-3 | MUST | Reviewers and closers send nothing and receive only from the lead. | specmsg R25, D2 |
| MSG-4 | MUST | The message and one delivery row per recipient are committed, state `pending`, before any delivery attempt. Ids are never derived from counting records. | specmsg R26, L1, L2; rust 4, 6; protocols R1, R2 |
| MSG-5 | MUST | A send is idempotent on (sender, client-supplied key). Retrying after a crash returns the first record and creates no second one. | rust 5; rust crash test row 28963 |
| MSG-6 | MUST | A per-sender sequence number is assigned in the same transaction as the insert. Gaps are detectable. | rust 7; protocols R6 |
| MSG-7 | MUST | The SHA-256 of each body is stored and checked on read and by verify. | rust 8 |
| MSG-8 | MUST | No lock is held while waiting on a delivery. Each delivery has a time bound. | specmsg R27, L3 |
| MSG-9 | MUST | An `answer` answers a real question that was asked of the answerer, and goes only to the asker. | specmsg R28 |
| MSG-10 | MUST | A failed delivery makes the send exit non-zero with a distinct code and print `NOT DELIVERED to <role>: <reason>`. | specmsg R29, D5 |
| MSG-11 | MUST | Each recipient has its own delivery state. teamwire never claims a message was read unless the recipient acknowledged it. | rust 9; protocols R4, R5 |
| MSG-12 | MUST | Undelivered messages stay in the store with attempt count and reason, and status lists them. Nothing is deleted on failure. | rust 10; protocols R8 |
| MSG-13 | SHOULD | Messages between members do not wake the lead. A `question` to the lead does. | specmsg D7, Q4 |
| MSG-14 | SHOULD | The sender side refuses bursts and identical repeats to one recipient, and logs the refusal. An earlier copy that was never delivered does not count as a repeat. | protocols R28; archrev 3 |
| MSG-19 | MUST | A stored message can be resent explicitly, by its id, to its `undelivered`, `unconfirmed` or stale `pending` recipients only. A resend is a new delivery attempt on the same message, never a new message, and is exempt from the burst and repeat checks. Only the sender or the lead may resend. | archrev 3; lead's triage m016 |
| MSG-15 | MUST | The type set is closed and checked before anything is stored. Refused sends are logged as refusals. | protocols R10 |
| MSG-16 | SHOULD | Field names follow CloudEvents where one fits (`id`, `source`, `type`, `time`, `subject`). | protocols R12 |
| MSG-17 | SHOULD | Causal links use `re` (the id being answered). No vector clocks. | protocols R7 |
| MSG-18 | MUST | The rules above apply identically to the CLI and the MCP tools, and each tool result says `delivered`, `undelivered` or `unconfirmed` per recipient. | specmsg R31; harness R8 |

### Watch and status (WAT)

| Id | Level | Requirement | Source |
| --- | --- | --- | --- |
| WAT-1 | MUST | The watch prints at most one line per poll, joining events with ` \| `. A normal completion adds no watch line, because the completion already woke the lead. | specmsg R32, E1, E2; speclife LC-20; INV-9 |
| WAT-2 | MUST | Each event is announced once per occurrence. "Already announced" and "stuck now" are separate state. An overdue member that is working is not stalled. `TEAM-STALLED` re-arms when something moves. `SILENT` and `BLOCKED` carry an episode number in their key and re-arm when the member works again or the dialog closes; `LATE` carries the generation. | specmsg R33, E8; speclife LC-21; INV-9 |
| WAT-3 | MUST | No quiet counting for the first 90 s of a member's run, or until it has been seen working. | specmsg R34, E6; speclife LC-22; INV-10 |
| WAT-4 | MUST | A failed liveness listing is not evidence of death. `DIED` needs three consecutive absent polls and a non-empty listing. | specmsg R35, E7; speclife LC-23; INV-21 |
| WAT-5 | MUST | Completion comes from teamwire's records and process liveness, never from the host's or harness's idle or done flag. | specmsg R36, E11; speclife LC-24; harness R11; INV-11 |
| WAT-6 | MUST | `TEAM-COMPLETE n/m` counts only `done` and names each member that is failed, blocked, invalid or died. A member announced `DIED`, or whose result was announced `UNDELIVERED` or `UNCONFIRMED`, is settled, so one dead member does not keep the team open forever. | specmsg R38, E12; speclife LC-25; INV-12 |
| WAT-7 | MUST | Membership is re-read every poll. Added members are watched, removed members are ignored, and a closed or missing team ends the watch with no output. | specmsg R39, E13; speclife LC-28; INV-13 |
| WAT-8 | MUST | A reprompt resets that member's counters and timers. A prompt that reached a finished member outside teamwire is shown in status as "prompted outside teamwire". | specmsg R40, E14, L14; INV-14 |
| WAT-9 | SHOULD | `SILENT` after 3 quiet polls, `LATE` at a per-member deadline (default 1800 s), poll interval 20 s. All are configurable and tested. | specmsg R41, E15 |
| WAT-10 | SHOULD | `TEAM-STALLED` is the only event the lead is told to turn into a push notification to the user. | specmsg R42 |
| WAT-11 | MUST | `status` changes nothing and shows, per member: state, delivery state, reprompt count, age against deadline; then the last five messages, undelivered message ids and undelivered results. | specmsg R43, F; speclife LC-30, S1 |
| WAT-12 | MUST | `DECLARED` is announced when a completion has been recorded but not reported for two polls. | specmsg E10 |
| WAT-13 | MUST | `BLOCKED` is announced once when the host reports the member blocked by a dialog. | specmsg E9 |
| WAT-14 | MUST | A gap between polls much longer than the interval (the machine slept) resets quiet counters instead of producing SILENT or DIED for everyone. | architect, from rust 1.9 and INV-10 |
| WAT-15 | SHOULD | Use host events where the host offers them; polling stays as the fallback. | speclife LC-29, F10 |
| WAT-16 | MUST | Only one watch per team is active at a time. What it announced is kept in the store, so a restarted watch does not repeat itself. | architect, from INV-9 |

### Launch (LCH)

| Id | Level | Requirement | Source |
| --- | --- | --- | --- |
| LCH-1 | MUST | Refuse to launch from a session with no terminal of its own. The harness adapter detects it (Claude: `CLAUDE_CODE_SESSION_KIND=bg`). Nothing is created and the host gets zero calls. | speclife LC-1, L1; INV-3 |
| LCH-2 | MUST | Validate the whole spec, and every path the launch will touch, before creating anything. | speclife LC-2, L2 |
| LCH-3 | MUST | Record each resource (worktree, place, agent) with a state such as `creating` or `started` before the next step that cannot be undone. A crash leaves a resource close can find. | speclife LC-4, F3, L18 |
| LCH-4 | MUST | A host timeout during launch is a failure of that member, with the same cleanup as any other, never an abort of the team. | speclife LC-5, F4 |
| LCH-5 | MUST | A role name is never reused in a team, including a removed member's. One exception: a member whose launch failed before it reached `started` (for example a failed setup) may be launched again under the same role with `--add`; the failed attempt stays in the event log as `member.replaced`. | speclife LC-6, LC-12, L4, L13; archrev 6 |
| LCH-6 | MUST | Team names match `^[a-z][a-z0-9]{0,11}$` and role names `^[a-z][a-z0-9_]{0,15}$`; neither may contain a hyphen. The address, used as the host agent name and the harness session name, is `tw_<team>_<role>` (at most 32 characters, herdr's limit). It is unique across live teams. | speclife LC-7, L3; INV-32 |
| LCH-25 | MUST | Launch lists the host's live agent names and refuses a member whose address is already present, before creating anything for it. | INV-32; lead's triage m016 (herdr's handling of a duplicate name is untested, so teamwire does not rely on it) |
| LCH-7 | MUST | An implementer gets its own checkout. | speclife LC-8, L6 |
| LCH-8 | MUST | Worktrees are made with `git worktree add` under `<repo>/.worktrees/<slug>` by default, and the start commit (`base_sha`) is recorded. | speclife LC-9, L9, L11; INV-23 |
| LCH-9 | MUST | When the branch already exists, say so and say that `base` was ignored. Two branches that map to one slug are refused. | speclife LC-10, L10 |
| LCH-10 | SHOULD | Check with `git check-ignore` that the worktree root is ignored by the repo, and refuse loudly when it is not. | speclife LC-11 |
| LCH-11 | MUST | Run `setup` in the member's directory with that directory as `$1` and the variables `TEAMWIRE_TEAM`, `TEAMWIRE_ROLE`, `TEAMWIRE_HOME`, `TEAMWIRE_CHILD_CWD`, before the agent starts, with a timeout and a log file. A failure refuses that member and undoes its worktree and place without force. | speclife LC-12, L12, L13; INV-24 |
| LCH-12 | MUST | The contract and the brief reach the harness as files, never as free text on a command line. | speclife LC-13, L15; INV-22 |
| LCH-13 | MUST | Retry an agent start only on the host's "not ready" code, bounded (8 tries, 1.5 s apart). | speclife LC-14, L16 |
| LCH-14 | MUST | The first message (the task) is sent once. After a stall or timeout it is not resent; the exact manual command is printed. | speclife LC-15, L20; INV-5 |
| LCH-15 | MUST | Trust is handled by the harness adapter: the caller's named directory is the trust decision; only a provably plain folder-trust dialog is answered; a dialog that grants folder-declared commands is refused; a home directory is refused; `accept_trust: false` refuses. | speclife LC-16, L8, L17; harness R12; INV-25 |
| LCH-16 | MUST | A partial launch exits with a distinct code and names the members that started. | speclife LC-17, L23 |
| LCH-17 | MUST | The contract text is versioned data, one file per kind plus shared parts, with a test that each recorded rule is present. It names teamwire's own commands, not the Python scripts. | speclife LC-18, 1.2 |
| LCH-18 | SHOULD | Warn (the lead can override) when an implementer has no `check` or `owns`, when `owns` overlap within a launch, or when more than five implementer branches are open. | speclife LC-19 |
| LCH-19 | MUST | The task is sent as the member's first user message, not in the system prompt. `dispatch: false` starts the member idle. | speclife L19 |
| LCH-20 | MUST | Every member gets its identity (`TEAMWIRE_TEAM`, `TEAMWIRE_ROLE`, `TEAMWIRE_HOME`, `TEAMWIRE_TOKEN`) through a channel its adapters declare: the harness's per-session config (Claude: `settings.json` `env`), else the host's spawn environment. With neither, launch refuses the member. The launcher's own environment is never assumed to reach the agent. Identity is never inferred from the host. | harness R9; archrev B1; lead's live check 2026-09-29 |
| LCH-21 | MUST | Per-member harness configuration (hooks, MCP server) is passed per run through files in the team directory. teamwire never registers anything in the user's global configuration. | harness R7; `CLAUDE.md` |
| LCH-22 | SHOULD | teamwire chooses the member's session id before launch where the harness allows it (Claude: `--session-id`). | harness 1.3 |
| LCH-23 | SHOULD | Titles are required, at most 28 characters, unique per team ignoring case. | speclife L5 |
| LCH-24 | MUST | A worktree member's brief is also copied to `<worktree>/TASK.md`, and `TASK.md` is added to the repository's `info/exclude`, so the member reads its brief where it works and never commits it. Close and verify ignore that one path. | speclife L7; archrev 8 |

### Verify (VER)

| Id | Level | Requirement | Source |
| --- | --- | --- | --- |
| VER-1 | MUST | Run each member's `check` in its directory, record the real exit code and full output; a timeout is exit 124 and kills the whole process group. | speclife LC-38, V1, F16; INV-29 |
| VER-2 | MUST | Diff from the recorded start commit, moved forward to the merge-base with the base branch only when that commit descends from the start commit. | speclife LC-39, V2; INV-30 |
| VER-3 | MUST | Flag edits outside `owns`, deleted tests, added skip or expected-failure markers, added lint or type suppressions, changed lint/type/test/CI config outside `owns`, uncommitted paths, missing progress lines, and a missing decisions list. | speclife LC-40, V3-V6 |
| VER-4 | MUST | The weakening rules cover Rust (`#[ignore]`, `#[should_panic]`, `#[allow(`, `#![allow(`, deleted `#[test]`, `clippy.toml`, `rustfmt.toml`, `deny.toml`, `rust-toolchain.toml`, `.cargo/config.toml`, `Cargo.toml` lint tables) as well as Python, JS/TS, Go and Java. The pattern sets are data, per language. | speclife LC-41, F7 |
| VER-5 | MUST | A verify record holds the verdict, the flags and the HEAD sha it verified. The verdict is `PASS` only when the check passed and no flag is raised or every flag is waived by the lead. A flagged pass is `FLAGGED`. | speclife LC-42, F6 |
| VER-6 | MUST | A worktree member with no `check` is `NO-CHECK` and never clean. | speclife LC-43 |
| VER-7 | SHOULD | `owns` patterns have documented semantics (`dir/` is a prefix, otherwise a glob where `*` stays within one path segment and `**` crosses segments). | speclife LC-44 |
| VER-8 | MUST | verify changes no worktree. | speclife N10 |

### Close (CLS)

| Id | Level | Requirement | Source |
| --- | --- | --- | --- |
| CLS-1 | MUST | Close order is fixed: refuse if any member is busy or busy state is unknown; refuse over uncommitted work; archive and verify the archive; remove worktrees without force and keep branches; close places; delete the team state only if every step succeeded; release the sleep hold last. Each failing step stops the sequence with state intact. | speclife LC-45, C1-C7; INV-31 |
| CLS-2 | MUST | The archive is verified by content (a hash per file), not by count and size. An existing archive path is never overwritten. | speclife LC-46, F8, C10 |
| CLS-3 | MUST | A close lock stops launch, reprompt and removal from running during a close. Busy and dirty state are re-checked under the lock right before each destructive step. | speclife LC-47, F2, F9 |
| CLS-4 | MUST | Removing one member runs the same checks for that member; `--stop` is the only way to remove a busy member. | speclife C8 |
| CLS-5 | MUST | Removing a member refuses an implementer without a passing verify record for its current HEAD, and any member while the closer has not reported. `--unverified` overrides and prints why. | speclife LC-48, C9; INV-28 |
| CLS-6 | MUST | A removed member keeps its record (`removed_at`, note), so its role is never reused and later steps skip it. | speclife LC-49 |
| CLS-7 | MUST | teamwire acts only on a member's own place and never passes a force flag to a host or to git. | speclife LC-50, C5, N2 |
| CLS-8 | SHOULD | Close prints the branches kept and any unpushed commits. | speclife LC-51 |

### Sleep hold (SLP)

| Id | Level | Requirement | Source |
| --- | --- | --- | --- |
| SLP-1 | MUST | One hold per lead session: reuse an existing untimed hold, record `{pid, owned}`, release only when the last live team on that session closes and only when a team owns it, never kill a hold the user started, and re-check before killing that the process is still that hold. | speclife LC-52, C7; INV-26 |
| SLP-2 | MUST | The hold waits on the lead's real harness process. If it cannot be found, say so loudly and record no hold. | speclife L22, LC-53; INV-27 |
| SLP-3 | MUST | The mechanism sits behind a platform adapter (`caffeinate` on macOS; `systemd-inhibit` possible on Linux). | speclife LC-52 |

### Adapters (ADP)

| Id | Level | Requirement | Source |
| --- | --- | --- | --- |
| ADP-1 | MUST | The core knows teams, members, sessions, messages, completions and deliveries. It names no harness and no host: adapters are identified by string ids looked up in a registry, and the core has no closed list of products. Harness, host and platform facts sit behind traits. The core crate depends on no adapter crate. | harness R1; specmsg R44; archrev B2 |
| ADP-12 | MUST | The harness and host belong to each member and to the lead, not to the team. A team whose lead and members use different harnesses type-checks, and every lookup uses the adapters of the party it concerns. | brief (other harnesses fit without changing the core); archrev B2 |
| ADP-2 | MUST | The host trait covers: spawn, submit text, status, read the screen tail, send keys, close, list, and locate a session. | harness R2; specmsg R44 |
| ADP-3 | MUST | The harness trait covers: launch spec (argv, env, files), session identity, liveness, background-session detection, per-run hook and MCP configuration, trust handling, and OpenTelemetry environment. | harness R3 |
| ADP-4 | MUST | The Claude adapter reads liveness from the session files and `claude agents --json`, handling both row shapes (`status` and `state`), with a 5 s bound. | harness R10, Q7 |
| ADP-5 | MUST | The herdr adapter reads herdr's JSON output and matches `error.code`, never stderr substrings. | harness R13; speclife F11 |
| ADP-6 | MUST | Each adapter declares a capability record (can type, can push, has a turn-end hook, gives an ack, reports state, knows session ids). The core picks routes from it. | harness R19; protocols R20 |
| ADP-7 | MUST | One contract-test suite runs against the fake host and every real host adapter. | harness R2, R20; rust 26 |
| ADP-8 | MUST | The fake host and fake harness can fail, hang, or succeed-but-report-failure on command. | rust 26 |
| ADP-9 | SHOULD | A member may have no host place and be reached only through its inbox. | protocols R18 |
| ADP-10 | SHOULD | The lead-locating sources are chosen per harness and host. A combination with fewer than two independent sources says so at launch. | specmsg R46 |
| ADP-11 | SHOULD | A Claude channel push route exists behind a flag, off by default, never required for correctness. | harness R16; protocols R22 |

### MCP and skills (MCP)

| Id | Level | Requirement | Source |
| --- | --- | --- | --- |
| MCP-1 | MUST | teamwire ships a stdio MCP server that exposes send, resend, inbox, ack, done, status, members and log, with the same rules and refusals as the CLI. | specmsg R31; harness R8, R17; protocols R21 |
| MCP-2 | MUST | Tool results state what happened: the ids created and each recipient's delivery state. A refused call creates no record other than the refusal. | harness R8 |
| MCP-3 | SHOULD | The inbox is also a resource, whose update notification is only a hint. Dropping every notification loses nothing. | protocols R24 |
| MCP-4 | SHOULD | Use `rmcp` pinned to an exact 3.x version. | rust 24; protocols R23 |
| MCP-5 | SHOULD | Ship a teamwire skill and the six team shapes as data files in the repo, with no machine-specific paths. | speclife LC-59, LC-60 |

### Metrics and observability (OBS)

| Id | Level | Requirement | Source |
| --- | --- | --- | --- |
| OBS-1 | MUST | Every lifecycle and message event is written to the store in the same transaction as the change it records. Metrics are computed from these rows. | rust 21; protocols R13 |
| OBS-2 | MUST | OpenTelemetry export is a separate step that reads the store after an exported-up-to mark. A missing or slow collector never changes a command's exit code or timing. | rust 21, 22 |
| OBS-3 | MUST | `teamwire metrics` shows the local metrics with no collector configured. | brief |
| OBS-4 | MUST | Each member's environment gets `OTEL_RESOURCE_ATTRIBUTES` with `teamwire.team` and `teamwire.role` appended, and the harness session id is recorded on teamwire's own spans. Per-message and per-run ids never go into resource attributes. | harness R18 |
| OBS-5 | SHOULD | Pin `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp` (0.33.x) and `tracing-opentelemetry` (0.34.x) together. | rust 25 |

### Security and trust (SEC)

| Id | Level | Requirement | Source |
| --- | --- | --- | --- |
| SEC-1 | MUST | Never log an environment value whose name contains `TOKEN`, `KEY` or `SECRET`, including `CLAUDE_CODE_MESSAGING_TOKEN` and teamwire's own member tokens. | harness R21 |
| SEC-2 | MUST | Any route that pushes into a session accepts only senders in the team's member list. | protocols R27 |
| SEC-3 | MUST | teamwire registers no hook, MCP server, skill or launch agent outside its own repository and state directory. Example configs are files in the repo. | `CLAUDE.md`; harness R7 |
| SEC-4 | MUST | Nothing is ever written to `~/.claude/settings.json`, `~/.claude.json`, or herdr's managed hook file. Reading them is allowed. | harness R7, 1.5 |

### Extras (EXT)

| Id | Level | Requirement | Source |
| --- | --- | --- | --- |
| EXT-1 | SHOULD | A PR watch with the same one-line-per-poll rule: `FAIL` once per check, `MERGEABLE` on change, `DONE` when nothing is pending, exit 1 on any failure, `gh`'s exit code ignored. It is kept apart from the team lifecycle. | speclife LC-58 |
| EXT-2 | SHOULD | Keep titles in the record and show them in status. Renaming host workspaces to follow the lead's label is host-specific and optional. | speclife Q6, W13 |
