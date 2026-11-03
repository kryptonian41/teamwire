# Plan: measure what teamwire gains over one agent (2026-10-22)

This file is the whole plan. It assumes no earlier conversation. A fresh lead
session reads it, builds the tools with a small team, runs the pilot, and
writes the report.

## 1. The question

Does running work through teamwire beat one Claude Code session on the same
task? The study answers in three measures, each per kind of task:

| Measure | What the study counts |
| --- | --- |
| Efficiency | Tokens by model and type, priced at list price, per task that passes its hidden tests. Every session counts: the lead, each member, and every subagent. |
| Throughput | Wall time from the first prompt to the final result. Human interventions, which should be zero. |
| Accuracy | Hidden tests that pass, the fast gate on the result, files changed outside the task, and a blind grade of the diff. |

## 2. Decisions already made (with the user, 2026-10-22)

1. The study has two parts. Part one mines the 96 archived teams for
   descriptive numbers. Part two is a controlled pilot that runs the same
   tasks in each setup. The pilot comes first, and scaling up needs the
   user's yes.
2. The pilot's spending limit is about $50 for the benchmark runs. That buys
   2 tasks with one run per setup. The pilot proves the method works. The
   pilot cannot show how large a gain is, and the report must say so.
3. The study compares three setups on each task:
   - **Solo**: one Claude Code session on Opus, with the built-in subagent
     tool turned off.
   - **Solo with subagents**: the same session, with subagents allowed.
   - **teamwire**: one Claude Code session on Opus running `/teamwire:afk`
     with the same brief. The lead picks the team shape itself. A lead that
     picks "no team" is a valid result and gets recorded as such.
4. Tasks come from teamwire's own history, the Rust crates and `ui/`. Work
   repositories stay out.

## 3. What already exists

- `~/.teamwire/archive/<team>-<date>/team.db` holds every event of 96 closed
  teams. `docs/metrics.md` lists the event types and their data fields.
- `teamwire metrics <team>` prints `teamwire.member.tokens` per member, with
  model and effort. Teams launched since about 2026-10-17 carry these labels.
  Older teams have no token data.
- teamwire never counts the lead's own tokens. The lead's Claude Code
  transcript holds them, under
  `~/.claude/projects/<path>/<session-id>.jsonl`. Subagent transcripts sit in
  a `subagents/` folder beside the transcript. The study reads these files
  and never writes there.
- `docs/research/2026-10-18-lead-field-reports.md`, section 2.7, already
  counts fix rounds, questions and verify flags over the first 57 teams.
  Part one extends those counts to all 96 teams and adds cost and time.

## 4. Part one: mine the archive (a script, no agents)

A script reads every archived `team.db` read-only and writes one row per team
to a CSV file:

- team, date, title, member count by kind, and the shape when the brief names it
- wall time from `team.created` to `team.closed`
- completions by status, reprompts, and the fix-round rate per implementer
- the share of first `verify` records that pass, and flags by kind
- lead wake-ups by source
- member tokens by model and type, where recorded
- lead tokens, read from the lead's transcript, where the lead session id
  and its transcript can be found

The script leaves out smoke tests and dummy teams by a fixed name list kept
in the script: `smoke*`, `twdummy*`, `twtest`, `tmuxsmoke*`, `unreadsmoke`,
`holdchk`, `plugtest`. The script also leaves out every benchmark team from
part two, whose names start with `bn`.

Part one can describe cost and time per team. Part one cannot show a gain,
because no archived task was also done by one agent. The report says that in
its first paragraph about part one.

## 5. Part two: the pilot benchmark

### 5.1 Picking the two tasks

A script lists candidate commits from `develop` that meet every rule:

- the commit is newer than `v0.1.0` and is not a merge
- the commit adds or changes tests, run by `cargo test -p <crate>` or by
  `bun test` in `ui/`
- the diff is 30 to 200 lines and touches no protected file from `AGENTS.md`
- the commit's tests call an interface that a brief can name in one line,
  such as a CLI flag or a public function

The lead picks two tasks from the list:

- **A task that suits a team**: three candidate commits that touch different
  crates, given together as one request. teamwire's own rules expect a team
  to win here.
- **A task that suits one agent**: one commit of 100 to 200 lines across 2 to
  4 files. teamwire's own rules expect a team to lose here.

The lead writes one brief per task. A brief states the end state and names
the interface the hidden tests call. A brief never contains the commit's
code or its tests.

### 5.2 Keeping the answer out of reach

Each run starts in a fresh clone made like this:

1. Bundle the history up to the task's parent commit with
   `git bundle create task.bundle <parent>`.
2. Clone the bundle into a new folder under a temporary directory.
3. Remove the clone's `origin` remote.

A git worktree or a plain clone would share or copy the later commits, so an
agent could read the answer with `git log --all`. The bundle holds only
history up to the parent.

Every setup also runs with these settings:

- claude-mem is off, in every setup. claude-mem injects memory from earlier
  sessions, which can carry the answer.
- The clone's path is new, so Claude Code's project memory for that path
  starts empty.
- The global `~/.claude/CLAUDE.md` and the user's other plugins stay as they
  are. All three setups see the same ones, so the comparison stays fair.

### 5.3 Running a setup

The two solo setups run in print mode, one after the other, never at the same
time:

```bash
claude -p --model opus --max-budget-usd 4 --output-format json \
  --settings <per-run settings with claude-mem off> \
  [--disallowedTools <the subagent tool's name>] "<brief>"
```

The build step checks the subagent tool's real name with `claude --help`
and one short test call.

The teamwire setup runs an interactive Claude Code session in its own tmux
pane, in the clone, with the first message `/teamwire:afk <brief>` and an AFK
budget of $15. Team names start with `bn`, for example `bn1t`.

Runs go one at a time, so machine load stays the same for each. Gates have
failed under load when other teams ran tests at the same time.

### 5.4 Grading

A grading script runs after each run, in the run's clone:

1. Copy the original commit's test files into the clone.
2. Run the crate's tests, or `bun test`, and record each test's pass or fail.
3. Run `scripts/gate.sh fast` and record its exit code.
4. List files changed outside the files the original commit changed.

A blind grader then scores each diff. The grader is one Opus reviewer that
gets the brief and the diff, with the setup's name removed. The grader scores
correctness, scope and test quality on a fixed 0-to-3 scale written into its
brief. The grader sees the six diffs in a shuffled order.

### 5.5 Cost

A cost script reads every transcript a run produced: the session itself, its
subagents, and for teamwire the lead and every member. The script sums tokens
by model and type and prices them from a price table in one file. The build
step fills that table from Anthropic's current list prices, with the date it
read them. The same table prices every setup.

### 5.6 Stopping rule

Run the cheap solo setups first. After each run, the lead adds up the spend.
The lead stops before a run that would take the total past $50, and reports
which runs are missing.

## 6. The team that builds the tools

The building work is mostly one piece of code, so it follows the
`implement-then-review` shape. The grading code decides every accuracy
number, so a fresh reviewer checks it before any run.

| # | Kind | Task | Owns | Model, effort | Order |
| --- | --- | --- | --- | --- | --- |
| 1 | implementer | Write the mining, task-listing, clone, grading and cost scripts, in Python 3.9 with the standard library only, with unit tests on small fixture files | `bench/` | sonnet, medium | first |
| 2 | reviewer | Read the finished diff against this plan. Look hardest at answer leaks, grading that can pass a wrong result, and tokens counted twice or missed | nothing | opus, xhigh | after 1 |

- The implementer's `check` is `python3 -m unittest discover -s bench`.
- The scripts write results under `bench/results/`, which git ignores.
  The scripts never write under `~/.claude/` or `~/.teamwire/`.
- The fixtures are small invented files in the shapes of `team.db` and a
  transcript line. The fixtures never copy real transcripts into the repo.
- At most two fix rounds follow the review. No second review of the same diff.

Building these tools costs extra, separate from the $50 for the runs. A
Sonnet implementer and one Opus review should cost far less than one
teamwire benchmark run.

## 7. The report

The report goes to `docs/research/2026-10-22-gains-study.md`. The report has:

1. Part one's table: medians and spreads per measure, by team shape.
2. Part two's table: one row per task and setup, with every measure, and the
   shape the teamwire lead chose.
3. Paired differences per task, never a single total across tasks.
4. What the pilot cannot show. With 2 tasks and one run each, any difference
   could come from run-to-run variation alone.
5. A cost estimate for a full run, and the decision it needs from the user.

## 8. Open questions the lead must not answer alone

1. Which two tasks to run. The lead shows the candidate list and its two
   picks to the user before the first run.
2. Whether to scale up after the pilot, and to what budget.
3. Whether the benchmark tools get committed to `develop`, or stay on their
   branch.
