# teamwire against one session on a complex task (2026-10-23 to 2026-10-28)

This report follows the pilot in `2026-10-22-gains-study.md`. The pilot's
tasks were small enough that one session solved them completely. This test
replays a whole feature, so a team has room to gain. Each setup ran twice, in
two rounds. Round 2 changed more than the luck of the run (section 4), so the
two rounds are two looks at the task, not a clean repeat.

## Summary

- **Accuracy:** all six runs ended within 5 hidden tests of each other, and
  each setup's two runs within 4. Mean of two runs: solo 178, solo with
  subagents 180.5, teamwire 181.5, of 191. Six tests failed in every run, so
  about 185 were reachable.
- **Cost:** teamwire cost $53.38 and $59.45. Solo cost $6.94 and $6.39. Per
  round, teamwire cost 7.7 and 9.3 times as much as solo. Solo with subagents
  cost $11.88 and $5.02.
- **Time:** solo took 20 and 19 minutes. teamwire took 1h52m in round 2. In
  round 1 the Mac slept with its lid closed, and teamwire worked for about
  2h06m of a 9-hour wall clock.
- **Blind grade:** in round 1 teamwire tied the subagents run and beat solo
  only on correctness. In round 2 teamwire scored 3/3/3 and both solo setups
  scored 2/3/2. The grader could not see the design document, so these grades
  are rough.
- **The brief mattered more than the setup.** A first solo run with a brief
  that left out two interfaces scored 147. With the complete brief, solo
  scored 179 and 177.
- **Round 2's teamwire run read two things it should not have:** the
  installed AFK skill's headings and `teamwire afk start --help`. The user
  chose to count the run with this flag (section 3.2). The hidden tests show no
  gain from it.

## 1. The task

The task replays AFK mode, built by an earlier teamwire team.

| Item | Value |
| --- | --- |
| Start | `fb1fd8f`: the design document `docs/design/2026-10-19-afk-mode.md` and the schema v5 contract are committed |
| End | `ceff260`: the finished feature, before the unrelated `ccfix` commits |
| Size | 26 commits, 1,126 lines of code and 1,209 lines of tests, across 8 crates and the dashboard schema |
| Hidden tests | 191 tests in 13 test files that pass at the end commit |
| Do-nothing run | passes 118 of 191, so 73 tests separate a real result from none |

The brief says: build sections 5 and 6 of the design. It also names every
interface the hidden tests call that the design does not give. Those are four
refusal codes, `afk_hold_notes`, the fourth parameter of `create_afk_team`,
and the fields of `AfkView`. The compiler's errors on the do-nothing run give
the complete list. All setups got the same brief. The subagents brief adds one
line telling that session to use subagents for independent parts. The teamwire
brief adds two setup lines: claude-mem off for members, and Sonnet for
implementers with Opus for every other member.

## 2. Results

| Round | Setup | Hidden tests | Fast gate | Files outside the task | Blind grade (correctness / scope / tests) | Cost | Time |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | Solo, first brief (missing two interfaces) | 147 | pass | 8 | not graded | $9.09 | 42m |
| 1 | Solo | 179 | pass | 5 | 2 / 3 / 3 | $6.94 | 20m |
| 1 | Solo with subagents (4 Opus subagents) | 181 | pass | 7 | 3 / 2 / 3 | $11.88 | 13m |
| 1 | teamwire (12 members) | 181 | pass | 19 | 3 / 2 / 3 | $53.38 | 8h58m wall, 2h06m awake |
| 2 | Solo | 177 | pass | 5 | 2 / 3 / 2 | $6.39 | 19m |
| 2 | Solo with subagents (3 Sonnet subagents) | 180 | pass | 4 | 2 / 3 / 2 | $5.02 | 33m |
| 2 | teamwire (14 members), leak flagged | 182 | pass | 15 | 3 / 3 / 3 | $59.45 | 1h52m |

Hidden tests are out of 191. Nobody typed into any run.

The teamwire leads chose these teams:

| Kind | Round 1 | Round 2 | Model |
| --- | --- | --- | --- |
| planning | planner, a second planner after the first lost turns to sleep | planner (3 sessions after handoffs), plan reviewer | Opus |
| implementer | 4 | 5 | Sonnet |
| reviewer | one per implementer branch | one per implementer branch | Opus |
| closer, reporter | one each | one each | Opus |

teamwire's cost by model:

| Round | Opus (lead and members) | Sonnet implementers |
| --- | --- | --- |
| 1 | $42.84 | $10.54 |
| 2 | $51.69 | $7.76 |

In round 2, planning alone cost about $28, against about $10 in round 1. The
plan reviewer found 9 gaps, and the planner reworked the plan over three
sessions.

### Where the hidden-test scores differ

Six tests failed in every run. They pin details that neither the design nor
the brief states, such as an exact usage-error code, where the task copy goes,
and exact warning wording.

Round 2 teamwire was the only run to pass a seventh test: a team that never
had a member closes with no report. That run's lead chose that behaviour after
a review finding. The design did not ask for it.

## 3. What happened during the teamwire runs

### 3.1 Round 1: the machine slept

- The lid closed 11 minutes into the run, on battery at 13%. The machine then
  slept with short wakes for 6h52m. teamwire's sleep hold uses `caffeinate`,
  which cannot stop a closed lid.
- Each sleep killed the turn in progress. The lead replaced the stalled
  planner by itself.
- The run went about one hour past its 8-hour AFK budget.

### 3.2 Round 2: the leak scan flagged two sessions

| Session | What it read | Answer content? |
| --- | --- | --- |
| lead | teamwire's own lead skill, `skills/teamwire/SKILL.md` | No: this is the skill the lead runs on |
| lead, planner | `teamwire afk start --help` from the installed binary | Partly: the finished command's flags and default budget |
| planner | front matter and headings of the installed `skills/afk/SKILL.md` | Yes: the end state of part of the task |
| planner | `grep '"setup"'` over `~/.teamwire/archive` | No: only past teams' `setup` commands |

Every teamwire run on this task loads the finished AFK skill into its lead,
because the run starts with `/teamwire:afk`. So part of what the planner read
was already in the lead's context, in both rounds. The solo sessions could
also have run the installed binary's `--help`; they did not. Round 2 teamwire
still failed the tests that pin the details the help shows, such as
`a_bad_budget_is_a_usage_error`. The user chose to count the run and flag the
leak.

## 4. Why the two rounds are not a clean repeat

| Change between rounds | Effect |
| --- | --- |
| The user's settings moved Opus 5.5 from high to medium effort on 2026-10-26 | Round 2 ran at medium effort in every setup. Round 1 ran at high. The user chose to keep medium for round 2. |
| Claude Code updated from 2.1.308 to 2.1.310 | Possibly different defaults and behaviour |
| The machine stayed awake in round 2 | teamwire's round 2 time is usable. Its round 1 time is not. |
| Every earlier run clone and the grader's build cache were deleted before round 2 | No run could read an earlier run's work |

Within each round, the three setups match: same brief, same effort, same
Claude Code version.

## 5. Fixes made to the method

| Problem | Effect | Fix |
| --- | --- | --- |
| The brief left out two interfaces that the original team changed after its contract commit | Three test files did not compile against a correct design | The brief names every interface the tests call, taken from the compiler's errors on the do-nothing run. Solo ran again. |
| Cargo ran no test in a crate when one test file of that crate did not compile | One file zeroed up to 43 unrelated tests | The grader runs each test file as its own binary (`benchbin/perbin`) |
| The gate at this start commit is older than `TEAMWIRE_BASE` and needs a local `main` | The gate failed for every setup | Run clones and the grading copy get a local `main` at the start commit |
| The price table had Sonnet 5.5 cache reads at $0.20 per million tokens | Every Sonnet cost was too high. Round 1 teamwire was first reported as $57.37. | Corrected to $0.10 from Anthropic's pricing page on 2026-10-26. The cost script now matches Claude Code's own figure exactly. |
| The cap watcher looked for "(open)", but an AFK team prints "(open, afk)" | The watcher never saw round 1's team close | Fixed before round 2 |
| Results sat in `/private/tmp`, which macOS cleans | The pilot's result files and round 1's solo and subagents score files were deleted | Everything left was moved out of `/private/tmp` on 2026-10-28, and is now in `bench/studies/2026-10-gains/` |

## 6. What this test can and cannot show

- On this feature, one Opus session reached nearly the same hidden-test
  score as a 12- or 14-member team, at about an eighth of the cost and a sixth
  of the time.
- teamwire's process caught real problems along the way. Its reviewers and
  closer sent fix rounds, and round 2's result got the best blind grade. The
  hidden tests do not capture most of that.
- The subagents setup is cheap and close to teamwire's score. Its cost and
  time varied the most between rounds: $11.88 in 13 minutes with Opus
  subagents, $5.02 in 33 minutes with Sonnet subagents.
- Two runs per setup cannot separate differences of a few tests from noise.
- The hidden tests come from one team's implementation, and six of them could
  not be reached from the brief.
- The task came with a complete design document, so most of the planning was
  done in advance. A team's planning and review add less when the plan is given.

## 7. Spend

| Item | Round 1 | Round 2 |
| --- | --- | --- |
| Solo, first brief | $9.09 | — |
| Solo | $6.94 | $6.39 |
| Solo with subagents | $11.88 | $5.02 |
| teamwire | $53.38 | $59.45 |
| Blind grader | $3.00 | $3.47 |
| Tool fixes (`benchfix`, `benchbin` members) | $1.56 | — |
| **Total** | **$85.85** | **$74.33** |

The two rounds together cost $160.18. The lead session's own tokens are not
in these totals.

## 8. Where everything is

- Tools: `bench/`, with the run scripts in `bench/run/`. `bench/README.md`
  shows how to run this task again.
- Briefs: `bench/briefs/afk.md`, and `bench/briefs/afk-subagents.md` for the
  subagents setup. The teamwire setup used `afk.md` plus the setup line
  in section 1.
- Results, in `bench/studies/2026-10-gains/`:
  - Round 2: `afk-round2/<setup>/`. Round 1: `afk-round1/<setup>/`, where only
    the teamwire folder and the other two diffs are complete.
  - Blind grader inputs: `blind-afk-round1/` and `blind-afk-round2/`. Keys and
    outputs: `blind-keys/`.
  - Both teamwire archives: `tw-archives/afkmode-2026-10-23/` and
    `tw-archives/afkmode-2026-10-27/`.
