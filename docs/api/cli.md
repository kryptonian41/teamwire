# CLI: `teamwire`

One binary. Every subcommand works with no other teamwire process running
(STO-16). `--home <dir>` overrides `$TEAMWIRE_HOME` (default `~/.teamwire`) on
every subcommand. `--json` prints machine-readable output where listed.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 1 | Refused by a rule; nothing changed |
| 2 | Partial: launch started some members; verify found a member not clean; close stopped at a step; install or uninstall made some changes and then failed |
| 3 | Usage error |
| 4 | Not delivered: at least one recipient is `undelivered` |
| 5 | Unconfirmed: at least one recipient is `unconfirmed`, none `undelivered` |
| 6 | Store error: busy past the timeout, disk full, schema newer than this binary |

## Lead commands

These refuse a caller that carries a member identity (`TEAMWIRE_TOKEN`,
`TEAMWIRE_ROLE` or `TEAMWIRE_TEAM` set) with `refused.identity`, exit 1,
before opening the store: `launch --add`, `watch` (with or without `--once`,
because a single poll takes the lease and uses up announcement keys the lead's
watch would then never print), `close` (except `--dry-run`) and
`verify --waive`. Plain `verify` stays open, because the closer runs it. A
member must never be able to remove a peer, take the watch lease, add
members by mistake, or waive its own flags (ADR 0008, VER-5).

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
`pane` mode; a home-directory `cwd`; a worktree member when git does not
ignore `.worktrees/` (`worktree_not_ignored`, LCH-10); a member whose host
is `none` (v1 cannot start one); less than 200 MB free.

Spec fields, per team: `team`, `title`, `setup`, `harness` (default
`claude-code`), `host` (default: detected; `herdr` when `HERDR_ENV=1`),
`children`, `plugins`. Per member: `role`, `title`, `kind`, `mode`, `repo`, `branch`,
`base`, `cwd`, `model`, `effort`, `check`, `owns`, `task` or `task_file`,
`deadline_s` (default 1800), `setup`, `dispatch` (default true),
`accept_trust` (default true), `plugins`. `plugins` is an object from a
Claude plugin id `name@marketplace` to true or false. The team's map applies
to every member, and a member's value wins per key (ADR 0017). Keys starting with `_` are ignored.

Prints one line per member (`started`, `refused: <reason>`, `failed:
<reason>`), the warnings of LCH-9 and LCH-18, and the watch command to
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
| `TEAM-CLOSED` | The team was closed, or its row is gone, before the watch printed `TEAM-COMPLETE`. The watch exits 0 after it. It is never printed after `TEAM-COMPLETE`, because the watch has already exited |
| `TEAM-COMPLETE n/m done; not done: <role> <state> ...` | Every live (not removed) member is settled, and the watch exits. A member is settled when its newest completion in its current generation was delivered, or was announced as `UNDELIVERED` or `UNCONFIRMED`, or when the member was announced `DIED`. `n` counts only status `done`; every other member is named with its state (`failed`, `blocked`, `invalid`, `died`, `undelivered`, `unconfirmed`) |

### `teamwire status [<team>] [--role R] [--json]`

Reads only (WAT-11). Without a team, lists every team with its state and
member counts; a `teams/<x>/` directory with no database is skipped. With a
team: the lead's session and liveness (left out for a caller that carries a
member identity); per member: state
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
`comms/` copies from the stored body (when that body still matches its hash).
An `outbox/` copy is reported but never rewritten: the store keeps only its
hash, not its bytes (STO-10).

### `teamwire close <team> [--role R] [--stop] [--unverified] [--dry-run] [--keep-state] [--archive-to DIR] [--clear-stale-lock]`

Whole team: the fixed order of CLS-1. `--role` removes one member with the
same checks plus CLS-5; `--stop` allows removing a busy member;
`--unverified` overrides CLS-5 and prints why. `--dry-run` prints the plan.
`--keep-state` leaves `$TEAMWIRE_HOME/teams/<team>` in place, marked closed.
The archive goes to `$TEAMWIRE_HOME/archive/<team>-<date>` (a `-HHMMSS`
suffix when taken) unless `--archive-to` names a directory. The state
directory is deleted last: it is first renamed to
`$TEAMWIRE_HOME/deleting/<team>.<pid>`, so a kill never leaves
`teams/<team>` without its `team.db`, and then removed. A later close
removes any directory there whose pid is dead. Prints the
branches kept and any unpushed commits. `--clear-stale-lock` clears a close
lock whose owner is gone: its pid is dead, or now belongs to a process that
started at another time. When another close took the lock after its owner
was found dead, nothing is cleared and the command refuses with that reason.

### `teamwire send <team> --to ROLES --type TYPE (--text TEXT | --file PATH) [--commit SHA] [--re ID] [--key KEY] [--from ROLE|lead]`

Sends one message (ADR 0003). `--to` takes roles separated by commas, or
`lead`; an agent of another team is `role@team`, and its lead `lead@team`
(ADR 0016). A message to another team is stored in both teams, and the
other team's store types the line. `<team>` is a positional argument. For a member, `<team>` and `--from`
default to `TEAMWIRE_TEAM` and `TEAMWIRE_ROLE`. With no `--from`, no
`TEAMWIRE_ROLE` and no `TEAMWIRE_TOKEN`, `--from` defaults to `lead`, as
`resend` does; the harness session then decides (ADR 0008). Prints the display id and one line per recipient. On
`undelivered` it also prints `NOT DELIVERED to <role>: <reason>` on stderr and
exits 4. A recipient whose delivery record could not be written is left
`pending`, named on stderr as `NOT RECORDED for <role>`, and exits 6; retry
it with `resend`. `--type reprompt` (lead only) starts the member's next
generation and says whether a watch is running, or that it could not tell.

### `teamwire resend <team> <ID> [--to ROLES]`

Retries one stored message, by display id, to its `undelivered`,
`unconfirmed` or stale `pending` recipients (ADR 0003 point 2a). Same message
id, a new delivery attempt, no burst or repeat check. The sender or the lead
only. For a recipient in another team it writes that team's copy if it is
missing, and never types a line that team already handed off. Exit codes as
for `send`.

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
call is skipped. A PR with no checks is done only after three polls in a row
find none, because GitHub has no check runs for a few seconds after a push.

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

The roster, as `team_members` returns it (`docs/api/mcp.md`), of the
caller's own team or any other. The text form prints each member's
`lifecycle` and the types it accepts from other members. Whether a member
is working or done is in `teamwire status`.

### `teamwire teams [--json]`

Every team under the home with its title, state and roster, as `team_teams`
returns it (MSG-21), and how to address its agents (`lead@<team>`,
`<role>@<team>`). Reads only and needs no proof. A team whose store cannot
be read is named on stderr and makes the command exit 6; the others are
still listed.

## Harness and server entry points

These are started by a harness, not typed by people.

| Command | Started by | Does |
| --- | --- | --- |
| `teamwire hook claude-stop` | a member's per-run Stop hook | Route A of ADR 0007: hold once for report gaps or unread messages, else write the result and deliver it. Reads the hook input on stdin. Always exits 0. |
| `teamwire hook claude-post-tool` | a member's per-run PostToolUse hook | Returns a reminder of unread message ids as `additionalContext`, once per id. Always exits 0. |
| `teamwire --home H mcp [--team T --role R [--token K] \| --lead]` | the harness, from `mcp.json` | The stdio MCP server (`docs/api/mcp.md`). A member's token comes from `TEAMWIRE_TOKEN` when `--token` is absent (ADR 0011). |

## Maintenance

| Command | Does |
| --- | --- |
| `teamwire doctor` | Read-only checks: `quick_check` on each team database, schema versions (a team at an older schema passes, with a note that the next write migrates it), free disk, network filesystem, stale close locks (a lock is stale when its pid is gone or now belongs to a process that started at another time) and watch leases, herdr's session hook present, `.worktrees/` ignored in the current repo, harness and host binaries on `PATH`, and the lead setup installed and matching this binary (only whether both items are teamwire's when this binary is a build under `target/`), with a warning for a local- or project-scope `teamwire` entry in the current directory, which Claude Code starts in place of the user one. Exit 0 when all pass, 2 otherwise. |
| `teamwire install [--dry-run] [--force]` | Installs the lead setup (ADR 0012): writes the `teamwire` skill to `<claude dir>/skills/teamwire/SKILL.md` and registers the user MCP server `teamwire` as `<this binary> [--home DIR] mcp --lead` through `claude mcp`. Skips an item that is already current. After an MCP change it says that only new Claude Code sessions see it. Refuses, before any write, a skill edited by hand, an MCP entry named `teamwire` that is not the lead's server, a binary inside a `target/` directory, and a `claude` that does not run; `--force` passes the first three. Exit 0 when done or when nothing needed a change, 1 when refused or when a change failed before anything was changed, 2 when a change failed after another was made (the message lists what was done; run it again). |
| `teamwire uninstall [--dry-run] [--force]` | Removes the skill and the MCP entry that `install` wrote. Refuses, before any change, a skill edited by hand, an MCP entry named `teamwire` that is not the lead's server, and a `claude` that does not run when the entry must be removed; `--force` removes the first two anyway. A binary under `target/` may uninstall. Exit codes as for `install`. A file the user put beside the skill stays. |
| `teamwire export <team> [--out FILE]` | Every record as JSON lines with CloudEvents-style names (STO-14). |
| `teamwire metrics [<team>] [--since DURATION] [--json]` | Local metrics from the event rows (`docs/metrics.md`). |
| `teamwire metrics export [--once]` | Sends events after the export mark to the OTLP endpoint, then moves the mark. A batch stops before the first unsent event whose 60-second bucket has not ended, so each bucket is sent as one point (ADR 0005); events of the current minute go in a later export. Without `--once` it repeats every 60 s until stopped; with `--once` it makes one pass and exits. When export is off it exits 0 at once. |
| `teamwire version` | Binary version and the schema version it writes. |
