# CLI: `teamwire`

One binary. Every subcommand works with no other teamwire process running
(STO-16). `--home <dir>` overrides `$TEAMWIRE_HOME` (default `~/.teamwire`) on
every subcommand. `--version` prints `teamwire <version>+<build id>`, the same identity as
`teamwire version`. The build id is the first 12 hex digits of the commit the
binary was built from, or `unknown` when git was not available. A debug build
keeps the commit it was first built at. `--json` prints machine-readable output where listed.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 1 | Refused by a rule; nothing changed |
| 2 | Partial: launch started some members; reopen refused or failed on a session; verify found a member not clean; close stopped at a step; install or uninstall made some changes and then failed |
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
to every member of the same spec file, and a member's value wins per key
(ADR 0017). An `--add` spec must repeat the team's map, because the store keeps
none. Keys starting with `_` are ignored.

Prints one line per member (`started`, `refused: <reason>`, `failed:
<reason>`), the warnings of LCH-9 and LCH-18, a warning naming each file a
member's setup created that launch added to `info/exclude` (LCH-24), and the
watch command to run. Exit 2 when any member did not start (including when none started).

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
| `UNREAD <id> -> <role> <age>` | A message to a member still in the team stayed `handed_off` for 10 minutes (`UNREAD_AFTER`), counted from its latest attempt. A message whose whole body fit in the typed line is never unread (ADR 0021), and neither is one the member recorded a completion after (ADR 0022). Once per message and recipient |
| `UNANSWERED <id> from <role> <age>` | A question to the lead, handed off or acknowledged, has no `answer` whose `re` points at it after 10 minutes (`UNANSWERED_AFTER`). Once per question. Questions between members appear only in `status` |
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
`undelivered`, and undelivered results. It also shows the team's watch
(`running` with pid and heartbeat age, `stale`, or `none`), deliveries still
`handed_off` after 10 minutes, every question with no answer, and per member
the teamwire build that started it (`started_by`, from its `member.started`
event's `writer_version`) with `older_build` when that differs from this
binary. `--json` carries the same fields as `watch`, `unread_deliveries`,
`unanswered_questions`, and `started_by` and `older_build` per member. `--role R` adds the exact command
to restart that member's session with its per-run configuration (for Claude
Code: `claude --resume <session-id> --settings ... --mcp-config ...
--append-system-prompt-file ...`), because a session resumed without them has
no Stop hook, no identity and no MCP tools. The command carries the
`--model` and `--effort` the member was launched with, when launch recorded
them. A member started by an older build resumes on Claude's default.

### `teamwire reopen <team> [--role R] [--dry-run]`

Resumes the sessions of a team whose sessions are gone, for example after the
terminal was closed. A caller that carries a member identity is refused with
`refused.identity`, exit 1. A team that is unknown, `closing` or `closed`, or a
`--role` that names no member, is refused with exit 1.

The command reopens the lead and every member in state `started` whose
session is gone (`Harness::is_alive` says `Dead`), whether or not that member
reported done. `--role R` reopens only that member and leaves the lead alone.
It checks every session before it opens any tab. Tabs then open one at a time
until one is up, the lead's first, so a host that groups a team's tabs puts
the others beside it. The remaining tabs open at the same time, because each
waits several seconds for its session to load. The store records each new tab
after all of them are up.
Each session is resumed in a new tab named `tw_<team>_lead` or
`tw_<team>_<role>`, in its own folder, by its own session id. A member's argv
is the one `status --role` shows, with the recorded model and effort. The
lead's argv is `claude --resume <session id>`, because the lead loads the
teamwire plugin itself. A plain folder-trust dialog is answered as launch
answers it. The stronger dialog is never answered. When the trust step fails,
the new tab is closed and the session is reported `failed`; the member's
recorded place changes only after the trust step succeeds. Nothing is typed
into a reopened session. A member that was killed mid-turn sits idle until the
lead sends it a `reprompt` or a `note`.

The lead's folder is the newest `lead_cwd` that `launch` or `lead --adopt`
recorded. For a team with none, the harness reads it from the session's live
record, then from its transcript. A resume only finds a session from that
folder. A lead whose folder is the user's home directory is refused, because
reopen would answer the folder-trust dialog for it (LCH-15).

Each session gets one line on stdout: `lead reopened`, `<role> reopened`,
`<role> skipped: <why>`, `<role> refused: <why>` or `<role> failed: <why>`.
A session is skipped when it is already open, when it is not known whether it
runs (two processes on one session would make delivery refuse with
`two_places`), when the member is removed or failed to start, or when the lead
is a background session, has no session recorded, or is the calling session
(`already open (this session)`). A session is refused when it has no session
id, or its folder or one of its per-run files is gone, or the lead's host is
`none`, or the lead's folder cannot be found or is the home directory. For the
lead, the reason then carries the command to run by hand. A member whose
`member.started` event has no `model` or `effort` key (an older build) gets
one `warning:` line on stderr.

After the sessions, the command lists the results the lead has not received,
in the form `status` uses, with a line telling the lead to read those files,
because teamwire never sends a result again by itself. The sleep hold is
started or reused as launch does; a hold that cannot be taken is a warning.
Then come the line `watch: teamwire watch <team>` and a line that says a
member killed mid-turn needs a `reprompt` or a `note`. When the caller is not
the team's lead session, a last line says that results go to the reopened lead
and that `teamwire lead --adopt <team>` makes the caller the lead instead.

`--dry-run` prints the same lines as a plan, then `dry run: nothing was opened
and nothing was written`. It starts no tab, writes no row, and opens the
database read-only, so it does not migrate an older database. Exit 0 when no
session was refused or failed, and 2 otherwise.

### `teamwire verify <team> [--role R] [--waive FLAG --reason TEXT] [--deep] [--repair-files]`

Runs each member's check and the diff rules (ADR 0009). Prints one verdict
per member (`PASS`, `FLAGGED`, `FAIL`, `NO-CHECK`, `SKIPPED`) with its flags,
including `no-commits` for an implementer that recorded `done` while its
HEAD is still its base,
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
change (a first `CONFLICTING`, `BLOCKED` or `DRAFT` merge state is said once),
then `DONE: n passed, m failed` per PR, ending ` (merge state: <state>)` when
the state is neither `CLEAN` nor `DIRTY`. A `BLOCKED` state at DONE is read
once more after half the interval (at most 5 s), because GitHub recomputes it
after the last check finishes. The DONE line also names required checks that
never reported, `(required, never reported: a, b)`. The required set is read
once per PR from the base branch's branch endpoint and rules endpoint (all
pages). It is unknown, and the line says `(required checks unknown)`, when
either endpoint fails with anything but HTTP 404. Exits when every PR is done;
exit 1 when any check failed, or when a required check never reported and
the merge state is not `CLEAN`. Ignores the exit code of `gh pr checks` and
`gh pr view`; a failed call of either is skipped. A PR with no checks is done only after three polls in a row
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
| `teamwire hook claude-post-tool` | a member's per-run PostToolUse hook | Returns a reminder of unread message ids as `additionalContext`, once per id. A `handed_off` message whose whole body fit in the typed line is not unread. Always exits 0. |
| `teamwire hook claude-lead-stop` | the lead's Stop hook, from the teamwire plugin (ADR 0018) | Holds the lead's stop once when the session leads an open team with a started member that has no completion for its current generation and no live watch lease, with one line per team naming `teamwire watch <team>`. Returns nothing in a member session, for a subagent, on a continued stop, or on any error. Opens stores read-only and writes nothing; gives up after about 5 s. Always exits 0. |
| `teamwire --home H mcp [--team T --role R [--token K] \| --lead]` | the harness, from `mcp.json` | The stdio MCP server (`docs/api/mcp.md`). A member's token comes from `TEAMWIRE_TOKEN` when `--token` is absent (ADR 0011). |

## Maintenance

| Command | Does |
| --- | --- |
| `teamwire doctor` | Read-only checks: `quick_check` on each team database, schema versions (a team at an older schema passes, with a note that the next write migrates it), free disk, network filesystem, stale close locks (a lock is stale when its pid is gone or now belongs to a process that started at another time) and watch leases, herdr's session hook present, `.worktrees/` ignored in the current repo, harness and host binaries on `PATH`, `claude --version` and `claude --help` (a warning when the help lacks any of `--session-id`, `--resume`, `--permission-mode`, `--append-system-prompt-file`, `--settings`, `--mcp-config`, `--model`, `--effort`), the host's version (`herdr --version` or `tmux -V`; a probe that fails or takes over 5 s is a warning), a warning naming, never showing, any of `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `CLAUDE_CODE_OAUTH_TOKEN`, `CLAUDE_CODE_USE_BEDROCK` or `CLAUDE_CODE_USE_VERTEX` that is set (each overrides the stored login for every member), a warning per open team with a member that another teamwire build started, the teamwire plugin installed at user scope, enabled, and with the files Claude Code loads matching this binary (not compared when this binary is a build under `target/`), a failure for a leftover ADR 0012 skill or MCP entry (`old lead setup`), and a warning for a local- or project-scope `teamwire` entry in the current directory, which Claude Code starts as a second server beside the plugin's. Exit 0 when all pass, 2 otherwise. |
| `teamwire install [--dry-run] [--force]` | Installs the teamwire plugin `teamwire@teamwire` (ADR 0018): writes a local marketplace to `$TEAMWIRE_HOME/claude-plugin/` holding the skill, the lead's MCP server `<this binary> [--home DIR] mcp --lead` and the lead's Stop hook `<this binary> [--home DIR] hook claude-lead-stop`, then registers it at user scope through `claude plugin marketplace add|update` and `claude plugin install|update|enable`. The plugin's version is a hash of its files, so a new binary or skill is a new version. Then removes an ADR 0012 setup: the stamped skill under `<claude dir>/skills/teamwire/` and the user MCP entry `teamwire` (through `claude mcp remove`). Skips what is already current. Refuses, before any change, a skill edited by hand, an MCP entry named `teamwire` that is not the lead's server, a marketplace named `teamwire` from another source, a binary inside a `target/` directory, and a `claude` whose `plugin` command does not run; `--force` passes the first four. Exit 0 when done or when nothing needed a change, 1 when refused or when a change failed before anything was changed, 2 when a change failed after another was made (the message lists what was done; run it again). |
| `teamwire uninstall [--dry-run] [--force]` | Runs `claude plugin uninstall` and `claude plugin marketplace remove`, deletes `$TEAMWIRE_HOME/claude-plugin/`, and removes an ADR 0012 skill and MCP entry with the same refusals as `install`; `--force` removes an edited skill, a foreign entry, or a marketplace named `teamwire` from another source anyway. A binary under `target/` may uninstall. Exit codes as for `install`. `claude plugin uninstall` leaves the plugin's cache folder on disk, which is Claude Code's behaviour: Claude Code keeps it at `<claude dir>/plugins/cache/teamwire/teamwire/<version>/`, one folder per installed version. `uninstall` does not delete it. |
| `teamwire export <team> [--out FILE]` | Every record as JSON lines with CloudEvents-style names (STO-14). |
| `teamwire metrics [<team>] [--since DURATION] [--json]` | Local metrics from the event rows (`docs/metrics.md`). |
| `teamwire metrics export [--once]` | Sends events after the export mark to the OTLP endpoint, then moves the mark. A batch stops before the first unsent event whose 60-second bucket has not ended, so each bucket is sent as one point (ADR 0005); events of the current minute go in a later export. Without `--once` it repeats every 60 s until stopped; with `--once` it makes one pass and exits. When export is off it exits 0 at once. |
| `teamwire version` | Binary version, as `<crate version>+<12 hex commit>` (or `+unknown` when the build found no git commit), and the schema version it writes. |
