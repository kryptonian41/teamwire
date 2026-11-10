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
- a close that refused: "Close";
- you build a feature with GitHub's spec-kit: "Spec-kit features".

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

## Spec-kit features

GitHub's spec-kit (https://github.com/github/spec-kit) writes a spec, a plan
and a task list before any code. Each spec-kit command is a prompt file that
one agent session follows. No spec-kit code reads `tasks.md`, and spec-kit
runs no task in parallel. A teamwire team runs a spec-kit feature with
spec-kit's own commands, each in the seat whose rules fit it, and spec-kit
stays unchanged. Start the team spec from `teamwire preset speckit`. These
facts come from spec-kit commit c438494 (v1.3.2 and 9 commits). Check them
again after a spec-kit upgrade.

### What the base branch must hold

A worktree holds only committed files, so commit these on the base branch
before launch:

- `.specify/`, from `specify init --here --force --integration claude`, or
  with the integration of your first member harness.
- The command folder of each member harness. Add the others with
  `specify integration install codex` and `specify integration install pi`.
- A written `.specify/memory/constitution.md`. `init` leaves it as a
  template full of placeholders. Write it with the user, through spec-kit's
  `constitution` command, before the first feature.

Then check that spec-kit added nothing that runs on its own:

- The `installed:` list in `.specify/extensions.yml` holds no `git`
  extension.
- No extension manifest, `.specify/extensions/<id>/extension.yml`, has a
  top-level `events:` key, and no `.specify/integration-events.yml` exists.
  An extension declares its `events:` in its own manifest, not in
  `.specify/extensions.yml`.
- No `.codex/config.toml` exists, and `.claude/settings.json` holds no hook
  that spec-kit wrote. Spec-kit tags each of its hooks with
  `"__speckit_event__": true`.

The `git` extension creates and switches branches, and teamwire owns the
branches. An extension with `events:` writes `.codex/config.toml`, and
Codex launch refuses a repository that holds that file. The same extension
writes hooks into `.claude/settings.json`, and those hooks run unreviewed
with the member's rights. Remove such an extension before launch.

### How each harness runs a spec-kit command

| harness | command files | how a member runs `implement` |
| --- | --- | --- |
| Claude Code | `.claude/skills/speckit-<cmd>/SKILL.md` | `/speckit-implement <scope>` |
| Codex | `.agents/skills/speckit-<cmd>/SKILL.md` | `$speckit-implement <scope>` |
| pi | `.pi/prompts/speckit.<cmd>.md` | teamwire starts pi with `--no-approve` (ADR 0029), which skips the folder's prompt files. The brief says: read `.pi/prompts/speckit.implement.md` and follow it, with `$ARGUMENTS` set to the scope. |

The other commands follow the same pattern, such as `/speckit-analyze`,
`$speckit-analyze` or `.pi/prompts/speckit.analyze.md`. A scope is plain
text, such as `Implement only Phase 3 (US1)`. The seat rules name no
spec-kit command, so write the form for the member's harness in its brief.

### The flow

1. **Spec.** A `spec` implementer in its own worktree owns
   `specs/NNN-name/`. Its brief names the folder and gives the feature
   description. It runs `specify`, then `clarify`, and stops after each
   one with its questions, which you bring to the user. It then runs `plan`
   and `tasks`, and commits after each command. The user reviews `spec.md`
   and `plan.md`.
2. **Check.** Add a plan reviewer with `teamwire launch --add`. Its `cwd`
   is the spec member's worktree, such as `<repo>/.worktrees/myteam-spec`
   for the branch `myteam/spec`. It runs `analyze`, which writes nothing, and reads the briefs.
   It also runs any task in `tasks.md` that claims "confirm the new tests
   fail", in its own scratch copy outside the repository, because a claim
   that is half false can lead the implementer to weaken tests.
   Reprompt the spec member once with the findings that hold. Run no second
   analyze on the same plan.
3. **Build.** One `impl` implementer runs `implement` phase by phase over
   the whole `tasks.md`. Its `base` is the spec member's branch, so the
   feature folder is in its worktree.
4. **Join.** Only when the stories split (below). The integrator merges the
   Foundational branch first, then the stories in priority order.
5. **Close.** The closer runs `converge` beside its usual checks. The
   owners fix what it finds.

When the user brings a finished `tasks.md`, committed with its `spec.md` and
`plan.md`, leave out the spec member, but the plan reviewer still runs
`analyze` on it before any implementer starts, because no member has read
that spec yet. Its `cwd` is a checkout of the branch that holds the spec,
such as your own checkout. That checkout may have no
`.specify/feature.json`, so its task says to run analyze's scripts with
`SPECIFY_FEATURE_DIRECTORY=specs/NNN-name` and
`SPECIFY_FEATURE_NO_PERSIST=1` set. The user fixes what holds and commits
it. Then cut the implementer's branch from that branch.

### Rules for the briefs

- **One spec at a time.** `specify` picks the next folder number by
  scanning `specs/`. Each spec member scans `specs/` in its own worktree,
  so two spec members on branches cut from the same base pick the same
  number, even when they run one after the other. Run one spec member at a
  time, and name its folder, such as `specs/001-user-auth/`, in its brief.
  Spec-kit issue #4270 is the related upstream report.
- **Questions.** `specify`, `clarify` and `checklist` wait for an answer in
  chat, and the seat rules forbid that wait. So the spec member stops once
  per batch of questions:
  1. It runs `specify` and stops with its questions from `specify`, at
     most 3. `specify` ranks no option, so the member marks its own
     recommended option for each question and says the mark is its own.
  2. You ask the user and reprompt it with the answers. It writes them into
     `spec.md`, runs `clarify`'s scan, and stops again with clarify's
     questions, at most 5, each with clarify's recommended option.
  3. You ask the user and reprompt it again. `clarify` asks one question at
     a time in chat, so the member cannot run it with a batch of answers.
     It follows clarify's write steps by hand with your answers, then runs
     `plan` and `tasks`.

  Both batches go in clarify's `## Clarifications` section of `spec.md`,
  under `### Session YYYY-MM-DD`, one line per answer in clarify's format
  `- Q: <question> → A: <answer>`, so `spec.md` has one place for every
  answer.

  A further stop is allowed only when an answer raises a new question. The
  member never answers a spec-kit question itself, unless your reprompt
  says to take the recommended option: the member's own mark at stop 1 and
  clarify's at stop 2. Then it lists each such answer under Decisions I
  made, with whose mark it took, so the user can tell those answers from
  the user's own in `spec.md`.

  At each stop the member writes its questions under 'Questions for the
  lead' in its report and records
  `teamwire done --status blocked --summary "3 questions for the lead. ..."`,
  with its own count. Its check fails at that stop, and its seat allows
  `blocked` with a failing check. A `blocked` result whose summary starts
  with `N questions for the lead.` is a question round, not a failure. Send
  the answers with
  `teamwire send <team> --to spec --type reprompt --file <answers>`. The
  member records `done` only after `tasks` is written and its check passes.
- **Checklists.** `specify` writes `checklists/requirements.md` itself and
  ticks its items when it validates the spec, so that checklist is ticked
  before any review. `checklist` writes its items unticked, and spec-kit
  says only a reviewer ticks them. The reviewer-only rule is about the
  `checklist` command's checklists. So the user ticks those in
  `specs/NNN-name/checklists/*.md` at the review of `spec.md` and
  `plan.md`. Then reprompt the spec member to commit them before any implementer starts,
  because the implementer's worktree holds only commits. `implement` stops on a checklist with unticked items with
  "Do you want to proceed with implementation anyway? (yes/no)". So the
  stop fires only when a checklist has unticked items, such as one from
  `checklist` that the user left open. The implementer's brief says to
  treat that stop as a question to you.
- **Ticks.** `implement` ticks `[X]` on each finished task in `tasks.md`.
  Put `specs/NNN-name/tasks.md` in each implementer's `owns`, and tell it
  to tick only the lines of its own phases. Lines in separate phases should
  merge cleanly. When they do not, the integrator stops on the conflict.
- **Ignore files.** `implement` writes `.gitignore`: its ignore-file step
  appends spec-kit's critical patterns for the technology it detects, 10 in
  the pilot, most of them unrelated to the feature. The step may also
  create `.dockerignore`, `.eslintignore` and similar files. The preset
  puts `.gitignore` in the implementer's `owns`. The brief says the
  implementer follows the ignore-file step for `.gitignore` only, appends
  only the patterns the step lists, lists each one it added in its report,
  and writes no other ignore file unless its brief owns that file.
- **Test claims.** A task such as "confirm the new tests fail" can be half
  false: in the pilot, three of six new cases passed before the code
  existed. The brief says a "confirm the tests fail" task is finished when
  the cases it names fail, and the implementer never changes a test to
  make it fail.
- **Check.** The implementer's `check` needs `tasks.md`, runs the tests
  and fails on any unticked task, such as
  `test -s specs/NNN-name/tasks.md && <test command> && ! grep -nE '^- \[ \] T[0-9]+' specs/NNN-name/tasks.md`.
  Keep the `test -s`. Without it the check passes when the folder name is
  wrong, because `!` turns grep's missing-file error into success.
  The check still passes on a lowercase `[x]` or a deleted task line, so
  the closer counts the ticks (see Converge below).
  When the stories split, a member's `check` is
  `test -s specs/NNN-name/tasks.md && <its own tests>`, with no grep. Wave 1
  cannot pass a grep over the story tasks it does not build, and the tasks
  `converge` appends carry no story label. The integrator's `check` on the
  joined branch carries the whole-file grep instead, so it also catches
  converge's open tasks.
- **Commits.** `implement` has no commit step, and the seat rules ask for a
  commit after each unit. The brief says: commit at the end of every phase
  that has tasks. Each commit holds that phase's code and its ticks, with
  the task ids in the message, such as `T012-T018: US1 user model`.
  Spec-kit's `tasks.md` writes `**Checkpoint**` lines only after story
  phases, so Setup and Polish have none. A phase with no tasks, such as an
  empty Foundational phase, gets no commit. The brief overrides the
  "Commit after every checkpoint" line that spec-kit's template writes
  into `tasks.md`.
- **The feature file.** Spec-kit finds the feature through the variable
  `SPECIFY_FEATURE_DIRECTORY`, else `.specify/feature.json`. Spec-kit
  git-ignores `.specify/feature.json`, so a new worktree has none, and a
  team spec sets no environment variable. So the implementer's `setup`
  writes the file, as the preset shows:
  `printf '%s\n' '{"feature_directory": "specs/NNN-name"}' > "$1/.specify/feature.json"`.
  The spec member needs no such line, because `specify` writes the file.
  When a spec-kit script stops with "Feature directory not found", run that
  command with `SPECIFY_FEATURE_DIRECTORY=specs/NNN-name` and
  `SPECIFY_FEATURE_NO_PERSIST=1` set.
- **The constitution.** The seat rules send a member to `CLAUDE.md` or
  `AGENTS.md`, not to `.specify/memory/constitution.md`. Every brief says:
  read the constitution before the first change and follow it. `analyze`
  and `converge` report a conflict with it as CRITICAL.
- **Split per story.** One implementer per user story only when the stories
  after the Foundational phase touch disjoint files. Read the file paths in
  the task lines to decide. Wave 1 is one member for Setup and
  Foundational. Wave 2 has one member per story, each with wave 1's branch
  as its `base` and its own phase as its scope. Polish comes last, on a
  branch cut from the integrator's branch once the stories are merged.
- **Converge.** Start the closer from `teamwire preset closer`, and add two
  steps to its task. First, the closer counts the ticks at each of the
  implementer's commits and checks that no task line is missing, because
  the implementer's check still passes on a lowercase `[x]` or a deleted
  task line. Second, in its own clone, on each head that ships, the closer
  writes `.specify/feature.json` at the clone's root. It then reads the
  clone's `converge` command file of any installed harness, such as
  `.claude/skills/speckit-converge/SKILL.md`, and follows it, running every
  script that file names from the clone's root. The closer never invokes
  `/speckit-converge` from its own session, because its session starts in
  your checkout, so the command would read your checkout's files. It puts
  converge's findings table in the report. The section converge appends to
  `tasks.md` stays in the clone. Converge reads code and runs no tests, so
  its "Converged" is a model's judgement and the closer's other checks
  still run.
- **The fix round.** With one implementer, send it the closer's findings.
  The closer's section is not in its `tasks.md`, so the implementer
  runs `converge` on its own branch, then `implement` on the new phase,
  then `converge` again. Its own `converge` appends the open work as a new
  phase. It stops at "Converged" or after two rounds. In a per-story
  split, story owners run no `converge`, because converge compares the
  code with the whole spec and on a story branch reports every other story
  as `missing`. So in a split the fix round runs on a branch cut from the
  integrator's branch, by one implementer, such as the Polish member. It
  runs the same loop on the joined code and is the only writer of the
  Convergence phase. The integrator then merges its branch.
- **The ref.** The team carries the feature as a ref,
  `{"provider": "speckit", "id": "NNN-name"}`, with the folder name as the
  id. `teamwire decisions --ref speckit:NNN-name` then lists the decisions
  of every team that worked on the feature.
