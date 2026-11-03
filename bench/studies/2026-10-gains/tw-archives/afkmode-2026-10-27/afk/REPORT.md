# AFK run afkmode: report

The run built AFK mode for teamwire. The finished work is the local branch
`afk/afkmode` at b762d89. Nothing was pushed. Every check that ran on the
final commit passed. The live check with a real Claude Code session still
needs you.

D-numbers below are entries in
`/private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/afk/decisions.md`.
The full test evidence is in the closer's report,
`/private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/reports/closer.md`.

## What you asked

From `afk/task.md`:

> teamwire needs AFK runs: a lead runs one team on one task while the user
> is away, with no questions to the user until a report is written.
>
> The design and build plan are in `docs/design/2026-10-19-afk-mode.md`.
> [...] End state: everything that sections 5 and 6 of the design describe
> exists and is tested.
>
> - `teamwire afk start` (section 5.2).
> - The AFK hold in the lead's Stop hook (section 5.3).
> - The budget and the model table in launch (section 5.4).
> - Close and status for AFK teams (section 5.5).
> - The AFK skill with the agreed content of section 6, installed the way
>   the other skills are (section 5.6).

The task also fixed some names: four extra refusal codes for `afk start`,
the `afk_hold_notes` function, a fourth parameter on `create_afk_team`, the
`afk` object in status, the `afk_budget_ends_at` key in `teams --json`, and
the dashboard's schema copy. The task said: do not push, and end with the
branch name. Implementers were to use sonnet and every other member opus.

## What I understood

No separate understanding file was written. The design and the task's list
of names fixed the end state. An AFK team is an ordinary team whose row has
`afk_budget_ends_at` set (design 5.1). The run had to deliver these parts:

| Part | What it does | Design |
| --- | --- | --- |
| `teamwire afk start --team --title --task [--budget 8h]` | The command creates an empty team led by the calling session. The command copies the task, takes the sleep hold, and refuses with the design's codes plus `no_lead_session`, `no_lead_process`, `afk_already_running` and `doctor_failed`. | 5.2 |
| The AFK hold | The lead's Stop hook keeps the lead running while nothing else will wake it. The hook tells the lead what to do next. | 5.3 |
| Budget and model table | Launch refuses a new implementer once the budget has ended (`afk_budget_spent`). Launch warns about a member whose model or effort is below the AFK table. | 5.4 |
| Close and status | Close refuses an AFK team until `afk/REPORT.md` has eight headings. Status shows the budget and the decision count. `teams` marks the team. | 5.5 |
| The AFK skill | The skill `/teamwire:afk` holds the agreed text of section 6. `teamwire install` writes the skill as a second skill of the Claude plugin. | 5.6, 6 |

## The result

- Branch: `afk/afkmode` at b762d89, in `/private/tmp/claude-501/bnruns/bn4tu2e1us/clone`.
- Base: `afk/base`, which is `main` (fb1fd8f) plus two lead commits.
  5b86c1e adds the `extra` parameter of `create_afk_team`. fb842b4 adds the
  `afk start` arguments as a shared starting point for the members.
- Size: 57 files changed, 3191 lines added, 203 removed (`git diff --stat main afk/afkmode`).
- Five implementers each built one part on their own branch: start, budget,
  stophold, closestat and skill. No two branches touched the same file.

`git log --oneline main..afk/afkmode` gives 30 commits, 6 of them merges:

```
b762d89 Merge branch 'afkmode/skill' into afk/afkmode
5778a70 Check the second word of an AFK skill command, fix the start refusal order, and say AFK is supported only for a Claude Code lead
065b102 Merge branch 'afkmode/skill' into afk/afkmode
55842b8 Fix the AFK docs and skill after review, and let close end a team that never had a member
29ec5e6 Merge branch 'afkmode/closestat' into afk/afkmode
8158ee6 A whole-team close of an AFK team that never had a member needs no report
137f4fc Merge branch 'afkmode/stophold' into afk/afkmode
37267a1 Merge branch 'afkmode/start' into afk/afkmode
18b63d9 Merge branch 'afkmode/budget' into afk/afkmode
629e68f Count only the members a budget-limited launch attempts as requested
a1236c0 Close names an unreadable AFK report as unreadable, and the full-repo ...
94e4675 Pin both halves of the rule that a watch wakes the lead
ef5d4c7 Document AFK runs in the API docs, the architecture, the README and ...
e026e22 Build teamwire afk start: refusals, doctor checks, success output and ...
d2ad30c Add doctor's launch_checks and move its healthy setup into a shared ...
34098e1 Make the AFK budget code and tests pass clippy
02e8bfd Add ADR 0025, the AFK requirements and the AFK failure modes
f87ba47 teams marks an AFK team and gives its budget end, with its zod copy
c92ec32 status shows an AFK team's budget and decision count, with its zod copy
918c5b9 Render the AFK skill as a second stamped skill of the Claude plugin
b449bf3 Fix the clippy indexing lint in the AFK hold test
0d2db29 Add the launch library steps of afk start: checks, team row, task cop...
b414d9c Add the AFK skill text, the main skill's pointer to it, and the tests...
09933e1 Close refuses an open AFK team whose afk/REPORT.md lacks one of the e...
6d99d48 The lead's Stop hook adds the AFK note after the watch notes
669b8bd Warn when a member of an AFK team is below the AFK model table
1bd20cc Add afk_hold_notes: the lead's AFK note when nothing will wake it
a54e5d6 Refuse a new implementer past an AFK run's budget
fb842b4 Add the teamwire afk start arguments and launch's team helpers as a c...
5b86c1e create_afk_team takes the extra keys of team.created
```

How to try it, from the README's own steps:

WARNING: step 3 replaces the `teamwire` binary on this machine. Other teams
on this machine use that binary. Wait until no team is running. An older
binary opens a version-5 database read-only, so do not go back to an older
binary afterwards without checking (see Risks).

1. `cd /private/tmp/claude-501/bnruns/bn4tu2e1us/clone`
2. `git checkout afk/afkmode`
3. `cargo install --locked --path crates/teamwire-cli`
4. `teamwire install`
5. Start a new Claude Code session with `claude --permission-mode bypassPermissions`.
6. Type a small task, for example `/teamwire:afk Add a CONTRIBUTING.md that says how to run the tests.`
7. Keep the lid open and the machine on power.

## Verified

The closer ran these on its own clone, on 065b102. That commit was the joined
branch before the last fix round:

| Command | Exit | Result |
| --- | --- | --- |
| `scripts/gate.sh` (full gate, detached from tmux) | 0 | fmt, tidy, clippy, test, doc, docs all ok |
| `scripts/gate.sh fast` | 0 | fmt, tidy, clippy ok |
| `cargo test` on the 7 changed crates plus teamwire-mcp, `--skip herdr_env_wins_over_tmux` | 0 | 959 passed, 0 failed |
| `cd ui && bun test` | 0 | 166 pass, 3 skip (live tests), 0 fail |
| `cd ui && bun run typecheck` | 0 | clean |
| `scripts/check-docs.sh` | 0 | `docs check: ok` |
| `teamwire verify afkmode` | 0 | all five implementers PASS at the commits that were merged |

The closer also changed the code behind each new test, 35 single changes.
Every test failed under its change, with one exception. The skill's command
test checked only the first word after `teamwire`. The skill member fixed
that test in 5778a70.

The closer ran every reader against an AFK team with zero members: status,
teams, verify, metrics, reopen, watch, the Stop hook, close and the
dashboard's schemas. Every command exited 0.

The lead ran these on b762d89, the final commit, in the clone above:

| Command | Exit | Result |
| --- | --- | --- |
| `scripts/gate.sh fast` | 0 | ok |
| `cargo test -p teamwire-harness-claude -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux` | 0 | 453 passed, 0 failed |
| `scripts/check-docs.sh` | 0 | ok |

The full gate did not run on b762d89. The last fix round changed one test
helper in teamwire-cli and some docs. The lead's crate tests and docs check
cover those files.

## Decisions made for you

The full list is D1 to D40 in `afk/decisions.md`. These change the result
the most:

- **D1, the roster.** The lead added a planner, a plan reviewer, the
  implementers, one reviewer per implementer branch, a closer and this
  reporter. The lead skipped an analyst, because the design and the task's
  names list already fixed the end state. The lead skipped scouts, because
  the planner read the code and the design's section 2 already gives file
  and line for each existing part.
- **D6 and D37, five implementers instead of the design's three.** The
  design's "endings" part came to about 650 lines and its start part to
  about 730. Both were past the 300-line split rule. So the planner split
  them into start, budget, stophold, closestat and skill.
- **D17, how close judges the report.** Close looks for the eight headings
  by name, ignoring case, numbering and bold. Close does not check their
  order. No flag overrides the check.
- **D38, a Codex lead is not refused.** `afk start` accepts whatever lead
  the session has. A Codex lead is out of scope in the design. The docs now
  say that only a Claude Code lead is supported, not that only one can run.
- **D39, an AFK team that never had a member closes without a report.**
  Without this, a failed `afk start` left a team that nobody could close
  without writing eight headings by hand.

The members also made smaller choices of their own. Each is listed under
"Decisions I made" in their report in `reports/`. The ones you may notice:

- start: the budget end is computed when the checks run, not when the row is written.
- closestat: an unreadable report gives `cannot be read: <error>`, a text D17 did not define.
- closestat: the status line prints the time with milliseconds, the same string as `--json`.
- budget: a member gets a warning if its model or its effort is below the table, each judged on its own.
- skill: the skill tells the lead that a `warning:` line from `afk start` is not a refusal and goes into the report.
- skill: the command test now checks the second word of every command with subcommands, not only `afk`.

## Not done, and why

- **No push, pull request or release.** The task and the design both say to
  ask you first.
- **No live check.** Nobody ran `cargo install`, `teamwire install` and a
  real `/teamwire:afk` session. The live check needs you at the machine, and
  installing would replace the binary that this team itself runs on.
- **No test of an old binary against a version-5 database.** The design
  relies on an existing rule from ADR 0015. Nobody ran it.
- **The full gate did not run on the final commit.** The full gate ran on
  065b102. The last fix round got a fast gate, two crates' tests and the
  docs check (see Verified).
- **Closer finding 6 was left as is (D40).** The Stop hook's note for an AFK
  team with no members still says to close "once REPORT.md exists". D39 lets
  that team close without a report. The note text is fixed by D16 and D31.
  The skill itself gives the right rule.
- **The AGENTS.md line about the tmux test was not added.** The plan asked
  for one line saying that `herdr_env_wins_over_tmux` fails inside tmux.

## Risks

- **A short store error can let one implementer start past the budget.**
  The budget check at `crates/teamwire-launch/src/member.rs:63-68` treats a
  read error as "no budget". Validation reads the same row just before and
  fails on an error, so only an error between the two reads reaches this
  line. The reviewer rated this low, and it is still open.
- **`crates/teamwire-cli/src/cmd/doctor.rs` is 798 lines.** The tidy check
  fails past 800. The next change to doctor must move code out first.
- **A lead that stops twice with nothing running is stuck.** The design
  accepts this. ADR 0025 and the failure-modes doc record it. No test covers
  it.
- **A Claude usage limit pauses every session.** AFK mode cannot work
  around that. The design accepts it.
- **A Codex lead can start an AFK team** (D38). The closer found this by
  reading `crates/teamwire-cli/src/wiring.rs:138-143`, and did not run it.
  The docs now call a Codex lead unsupported.
- **`herdr_env_wins_over_tmux` fails inside tmux.** The test failed on the
  base before this run too. The member checks skipped it. The full gate ran
  it outside tmux, where it passed.
- **Three claims nobody checked directly.** The closer did not try the
  rustc error that justifies a lint suppression at `member.rs:59`. The
  closer did not look at the `caffeinate` process itself. Nobody ran an old
  binary against the new database.

## Cost

Wall time: 2026-10-27T13:09Z (afk start) to 2026-10-27T14:58Z, about
1 hour 49 minutes.

Tokens per member, from `teamwire metrics afkmode`. "Cache read" is context
re-read from the prompt cache, which costs far less than fresh input.

| Member | Kind | Model, effort | Output | Cache write | Cache read | Total |
| --- | --- | --- | --- | --- | --- | --- |
| planner | investigator | opus, xhigh | 214,674 | 573,994 | 47,921,495 | 48,710,583 |
| closer | closer | opus, xhigh | 68,763 | 274,732 | 17,778,973 | 18,122,644 |
| planrev | reviewer | opus, xhigh | 75,864 | 260,621 | 13,786,582 | 14,123,211 |
| skill | implementer | sonnet, high | 54,187 | 213,718 | 11,973,416 | 12,241,455 |
| revskill | reviewer | opus, xhigh | 33,728 | 184,162 | 8,120,998 | 8,338,994 |
| closestat | implementer | sonnet, high | 35,413 | 141,464 | 7,417,959 | 7,594,950 |
| revstart | reviewer | opus, xhigh | 34,973 | 171,879 | 6,804,728 | 7,011,678 |
| start | implementer | sonnet, high | 43,575 | 174,946 | 6,495,633 | 6,714,238 |
| revclosestat | reviewer | opus, xhigh | 32,976 | 141,449 | 5,379,461 | 5,553,976 |
| revbudget | reviewer | opus, xhigh | 27,717 | 124,362 | 3,318,642 | 3,470,779 |
| stophold | implementer | sonnet, high | 18,610 | 94,369 | 2,779,890 | 2,892,923 |
| revstophold | reviewer | opus, xhigh | 18,535 | 106,341 | 2,648,498 | 2,773,426 |
| budget | implementer | sonnet, high | 17,121 | 94,461 | 2,616,310 | 2,727,944 |
| **all 13** | | | **676,136** | | | **140,276,801** |

Fresh input was under 500 tokens per member and is in the totals. The
metrics leave out two sessions: the lead, and this reporter, which was
still running when the numbers were read. The planner used about a third of
all tokens.

Other counts from the same command: 14 members started, 23 completions, 8
reprompts for fix rounds, 17 verify runs (11 PASS, 6 FLAGGED). Every flag
was a lint suppression with a reason, and the lead waived each one (D40).
