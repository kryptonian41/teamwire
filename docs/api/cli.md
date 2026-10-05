# CLI: `teamwire`

One binary. Every subcommand works with no other teamwire process running
(STO-16). `--home <dir>` overrides `$TEAMWIRE_HOME` (default `~/.teamwire`) on
every subcommand. `--json` prints machine-readable output where listed.

## What replaces what

| Python today | teamwire |
| --- | --- |
| `team-launch.py < spec.json` | `teamwire launch < spec.json` |
| `team-launch.py --add < more.json` | `teamwire launch --add < more.json` |
| `team-watch.py <team>` | `teamwire watch <team>` |
| `team-status.py <team>` | `teamwire status <team>` |
| `team-verify.py <team> [--role R]` | `teamwire verify <team> [--role R]` |
| `team-close.py <team> [--role R] [--dry-run]` | `teamwire close <team> [--role R] [--dry-run]` |
| `team-msg.py <team> --from X --to Y --type T --text ...` | `teamwire send <team> --to Y --type T --text ...` |
| `team-msg.py <team> --log` | `teamwire log <team>` |
| writing `done/<role>.json` by hand | `teamwire done --status S --summary ...` |
| `~/.claude/hooks/team-report.sh` (global Stop hook) | `teamwire hook claude-stop` (per-member Stop hook) |
| `pr-watch.py -R repo 123 124` | `teamwire pr-watch -R repo 123 124` |
| `team_sleephold.py` (library) | part of `launch` and `close` |
| `team_labels.py` (library) | part of the herdr adapter |
| `team_parent.py` (library) | the locator (ADR 0006), used by every delivery |

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 1 | Refused by a rule; nothing changed |
| 2 | Partial: launch started some members; verify found a member not clean; close stopped at a step |
| 3 | Usage error |
| 4 | Not delivered: at least one recipient is `undelivered` |
| 5 | Unconfirmed: at least one recipient is `unconfirmed`, none `undelivered` |
| 6 | Store error: busy past the timeout, disk full, schema newer than this binary |

## Lead commands

These refuse a caller that carries a member identity (`TEAMWIRE_TOKEN`,
`TEAMWIRE_ROLE` or `TEAMWIRE_TEAM` set) with `refused.identity`, exit 1,
before opening the store: `launch --add`, `watch` (except `--once`) and
`close` (except `--dry-run`). `verify` stays open, because the closer runs
it. A member must never be able to remove a peer, take the watch lease or
add members by mistake (ADR 0008).

### `teamwire launch [--add] [--dry-run] [--spec FILE]`

Reads a team spec (JSON) from stdin or `--spec`. Validates everything first
(LCH-2), then per member: records it as `creating`, makes the worktree, runs
setup, writes contract, brief (also as `<worktree>/TASK.md`, excluded from
git) and per-run config, spawns it through the host,
answers a plain trust dialog, records it as `started`, and sends the task as
its first message. Starts or reuses the sleep hold. `--add` adds members to a
running team. `--dry-run` validates and prints the plan, touching nothing.

Refuses (exit 1, nothing created): a background lead session; any spec error;
a reused role (except one whose earlier launch failed before it started,
LCH-5); a team or role name outside LCH-6 (no hyphens); an address
`tw_<team>_<role>` already live in the host (LCH-25); a title over 28
characters or repeated; an implementer in
`pane` mode; a home-directory `cwd`; a member whose host is `none` (v1
cannot start one); less than 200 MB free.

Spec fields, per team: `team`, `title`, `setup`, `harness` (default
`claude-code`), `host` (default: detected; `herdr` when `HERDR_ENV=1`),
`children`. Per member: `role`, `title`, `kind`, `mode`, `repo`, `branch`,
`base`, `cwd`, `model`, `effort`, `check`, `owns`, `task` or `task_file`,
`deadline_s` (default 1800), `setup`, `dispatch` (default true),
`accept_trust` (default true). Keys starting with `_` are ignored.

Prints one line per member (`started`, `refused: <reason>`, `failed:
<reason>`), the warnings of LCH-9, LCH-10, LCH-18, and the watch command to
run. Exit 2 when any member did not start (including when none started).

### `teamwire watch <team> [--interval SECONDS] [--once]`

Runs until the team completes or closes. Prints at most one line per poll,
events joined by ` | `, each starting `[teamwire:<team>]` (WAT-1). Default
interval 20 s. `--once` runs one poll and exits (for tests and scripts).
Refuses to start when another live watch holds the team's lease, and names
its pid.

Events, each announced once per occurrence:

| Event | When |
| --- | --- |
| `WATCHING n members` | First poll |
| `SILENT <role>` | 3 quiet polls, no completion, after the 90 s grace. Once per episode: a new episode starts when the member is seen working again |
| `DIED <role>` | 3 consecutive polls absent from a non-empty liveness listing |
| `BLOCKED <role>` | The host reports a dialog. Once per episode: a new episode starts when the dialog is gone |
| `LATE <role> <age>` | Past its deadline, once per generation |
| `DECLARED <role>` | A completion recorded but not reported for 2 polls |
| `COMPLETED <role> (<status>) <summary> Result: <path>` | Watch route only (ADR 0007) |
| `UNDELIVERED <role> <reason> Result: <path>` | A result the lead could not be reached with, including `delivery_interrupted` (left in `delivering` for over 90 s, ADR 0007 point 6a) |
| `UNCONFIRMED <role> Result: <path>` | A result whose typing stalled or timed out; it may or may not be on the lead's screen |
| `QUESTION-UNDELIVERED <id> from <role>` | A question to the lead that could not be typed |
| `QUESTION-UNCONFIRMED <id> from <role>` | A question to the lead whose typing stalled or timed out |
| `STORE-ERROR <code>` | The store could not be read; once per code |
| `TEAM-STALLED <roles>` | No member is working and at least one is not done; re-armed when something moves |
| `TEAM-COMPLETE n/m done; not done: <role> <state> ...` | Every live (not removed) member is settled, and the watch exits. A member is settled when its newest completion in its current generation was delivered, or was announced as `UNDELIVERED` or `UNCONFIRMED`, or when the member was announced `DIED`. `n` counts only status `done`; every other member is named with its state (`failed`, `blocked`, `invalid`, `died`, `undelivered`, `unconfirmed`) |

### `teamwire status [<team>] [--role R] [--json]`

Reads only (WAT-11). Without a team, lists every team with its state and
member counts. With a team: the lead's session and liveness; per member: state
(`done(<status>)`, `unreported`, `undelivered`, `died?`, `blocked`, `working`,
`quiet`, `late`, `prompted outside teamwire`), generation, age against
deadline, delivery state of its newest result, branch, title; then the last
five messages, message ids that are `pending` over 60 s, `unconfirmed` or
`undelivered`, and undelivered results. `--role R` adds the exact command
to restart that member's session with its per-run configuration (for Claude
Code: `claude --resume <session-id> --settings ... --mcp-config ...
--append-system-prompt-file ...`), because a session resumed without them has
no Stop hook, no identity and no MCP tools.

### `teamwire verify <team> [--role R] [--waive FLAG --reason TEXT] [--deep] [--repair-files]`

Runs each member's check and the diff rules (ADR 0009). Prints one verdict
per member (`PASS`, `FLAGGED`, `FAIL`, `NO-CHECK`, `SKIPPED`) with its flags,
and writes a verify record with the HEAD sha. Exit 0 only when every checked
member is `PASS`; else 2. `--waive` records a waiver for one flag of one
member at its current HEAD. `--deep` also runs `PRAGMA integrity_check` and
checks every message hash. `--repair-files` rewrites missing or differing
`comms/` and `outbox/` copies from the store.

### `teamwire close <team> [--role R] [--stop] [--unverified] [--dry-run] [--keep-state] [--archive-to DIR] [--clear-stale-lock]`

Whole team: the fixed order of CLS-1. `--role` removes one member with the
same checks plus CLS-5; `--stop` allows removing a busy member;
`--unverified` overrides CLS-5 and prints why. `--dry-run` prints the plan.
`--keep-state` leaves `$TEAMWIRE_HOME/teams/<team>` in place, marked closed.
The archive goes to `$TEAMWIRE_HOME/archive/<team>-<date>` (a `-HHMMSS`
suffix when taken) unless `--archive-to` names a directory. Prints the
branches kept and any unpushed commits. `--clear-stale-lock` clears a close
lock whose owner pid is dead, and nothing else.

### `teamwire send <team> --to ROLES --type TYPE (--text TEXT | --file PATH) [--commit SHA] [--re ID] [--key KEY] [--from ROLE|lead]`

Sends one message (ADR 0003). `--to` takes roles separated by commas, or
`lead`. For a member, `--team` and `--from` default to `TEAMWIRE_TEAM` and
`TEAMWIRE_ROLE`. Prints the display id and one line per recipient. On
`undelivered` it also prints `NOT DELIVERED to <role>: <reason>` on stderr and
exits 4. `--type reprompt` (lead only) starts the member's next generation
and says whether a watch is running.

### `teamwire resend <team> <ID> [--to ROLES]`

Retries one stored message, by display id, to its `undelivered`,
`unconfirmed` or stale `pending` recipients (ADR 0003 point 2a). Same message
id, a new delivery attempt, no burst or repeat check. The sender or the lead
only. Exit codes as for `send`.

### `teamwire log <team> [--role ROLE] [--json]`

Every message with its time, type, sender, recipients, a 300-character
excerpt and each recipient's delivery state. `--role` shows messages to or
from that role.

### `teamwire lead --adopt <team>`

Run from a new lead session. Records that session as the team's lead and
prints the old and new session ids (ADR 0008).

### `teamwire pr-watch -R OWNER/REPO [-i SECONDS] PR...`

EXT-1. One line per poll at most: a check's first failure, a mergeability
change, then `DONE: n passed, m failed` per PR. Exits when every PR is done;
exit 1 when any check failed. Ignores `gh`'s own exit code; a failed `gh`
call is skipped.

### `teamwire preset [NAME]`

Without a name, lists the six team shapes. With one, prints its spec skeleton
and its rules as text.

## Member commands

Each reads `TEAMWIRE_TEAM`, `TEAMWIRE_ROLE`, `TEAMWIRE_TOKEN` and
`TEAMWIRE_HOME` from its environment. Launch puts them there through the
member's identity channel (ADR 0008): for Claude Code, the `env` key of the
per-member `settings.json`, which reaches the Bash tool and the hooks.

### `teamwire done --status done|blocked|failed --summary TEXT`

Records a completion (ADR 0007). Prints what happens next. Exit 1 with the
reason for an invalid status. For a harness without a turn-end hook, the
first call with report gaps prints the gaps and stores nothing (exit 1); the
second call stores the completion.

### `teamwire inbox [--all] [--json]`

Prints unread messages, full text, oldest first, and marks them read.

### `teamwire ack ID...`

Marks messages read without printing them.

### `teamwire members [<team>] [--json]`

The roster, as `team_members` returns it.

## Harness and server entry points

These are started by a harness, not typed by people.

| Command | Started by | Does |
| --- | --- | --- |
| `teamwire hook claude-stop` | a member's per-run Stop hook | Route A of ADR 0007: hold once for report gaps or unread messages, else write the result and deliver it. Reads the hook input on stdin. Always exits 0. |
| `teamwire hook claude-post-tool` | a member's per-run PostToolUse hook | Returns a reminder of unread message ids as `additionalContext`, once per id. Always exits 0. |
| `teamwire --home H mcp [--team T --role R --token K \| --lead]` | the harness, from `mcp.json` | The stdio MCP server (`docs/api/mcp.md`). |

## Maintenance

| Command | Does |
| --- | --- |
| `teamwire doctor` | Read-only checks: `quick_check` on each team database, schema versions, free disk, network filesystem, stale close locks and watch leases, herdr's session hook present, `.worktrees/` ignored in the current repo, harness and host binaries on `PATH`. Exit 0 when all pass, 2 otherwise. |
| `teamwire export <team> [--out FILE]` | Every record as JSON lines with CloudEvents-style names (STO-14). |
| `teamwire metrics [<team>] [--since DURATION] [--json]` | Local metrics from the event rows (`docs/metrics.md`). |
| `teamwire metrics export [--once]` | Sends events after the export mark to the OTLP endpoint, then moves the mark. |
| `teamwire version` | Binary version and the schema version it writes. |
