# What teamwire gains over one agent: pilot results (2026-10-22)

This report follows the plan in `2026-10-22-gains-study-plan.md`. Part one
describes the archived teams. Part two runs two tasks in three setups. The
pilot proves that the method works. With 2 tasks and one run per setup, it
cannot show how large any gain or loss is (section 4).

## Summary

- **Accuracy:** all six runs passed every hidden test and the fast gate. The
  pilot tasks did not separate the setups on correctness.
- **Cost:** teamwire cost 17.5 times the solo run on the one-agent task
  ($13.67 against $0.78). It cost 18.5 times the solo run on the three-fix
  task ($32.86 against $1.78).
- **Wall time:** teamwire took 17.8 times as long on the one-agent task (49
  minutes against 2.75). It took 4.7 times as long on the three-fix task (72
  minutes against 15.4).
- **Blind grade:** the grader scored both teamwire diffs lower than the solo
  diffs. The lower teamwire scores came from work the brief did not ask for.
  One of the two grader reasons does not hold up (section 2.3).
- **Subagents:** the "solo with subagents" setup never started a subagent.
  On these tasks it was the same setup as solo, run a second time.
- **Human interventions:** zero in every run.

## 1. Part one: the archive

Part one counts cost and time per archived team. No archived task was also
done by one agent, so part one cannot show a gain.

**Correction, 2026-10-28.** The first version of this table priced Sonnet
5.5 cache reads at $0.20 per million tokens. Anthropic's pricing page gives
$0.10, which Claude Code's own cost figure confirms. The table below was
mined again on 2026-10-28 with the corrected price, so it also holds 7 teams
closed after 2026-10-22.

`bench/mine.py` read 106 archives and left out 10 smoke and test teams by
name. It wrote 96 rows. Only 4 briefs name their team shape, so the table
groups teams by size instead. Member tokens exist only for the 39 teams
launched from 2026-10-16 on. Lead tokens come from the lead's transcript and
exist for all 96 teams.

Each cell gives the median, then the 25th to 75th percentile, then the count.

| Members | Teams | Wall time (h) | Fix-round rate | First verify passes | Member $ | Lead $ | Total $ | Lead share of total |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1–3 | 38 | 0.29 [0.15–1.45] | 1.00 [0.88–1.00] n=18 | 1.00 [0.50–1.00] n=26 | 5.9 [3.4–10.4] n=15 | 2.0 [0.9–4.8] | 8.3 [5.4–12.5] n=15 | 0.20 [0.10–0.35] n=15 |
| 4–6 | 32 | 1.23 [0.52–2.28] | 0.75 [0.33–1.00] n=21 | 0.71 [0.46–1.00] n=22 | 14.0 [12.2–20.5] n=12 | 5.1 [2.3–8.8] | 20.9 [16.2–30.0] n=12 | 0.25 [0.16–0.35] n=12 |
| 7–12 | 21 | 2.89 [1.00–7.78] | 0.83 [0.18–1.00] n=16 | 0.71 [0.50–0.92] n=17 | 29.2 [16.3–40.6] n=9 | 12.9 [5.7–16.6] | 36.1 [29.2–52.8] n=9 | 0.28 [0.05–0.42] n=9 |
| 13+ | 5 | 16.46 [2.22–142.68] | 1.00 n=3 | 0.70 [0.27–0.80] n=4 | 162.5 n=3 | 35.3 [2.7–51.7] | 165.1 n=3 | 0.04 n=3 |

The 13+ row's wall time includes `mfa`, a team that stayed open for days.

How to read the table:

- The **lead's own tokens are about a fifth of a team's cost**. teamwire's
  `metrics` command never counts them. A cost figure from teamwire alone
  understates a team by about 20%.
- The **fix-round rate** is the share of implementers that got at least one
  reprompt. The rate counts every reprompt, not only the ones that carry
  review findings, so it runs higher than the 54% in the lead field reports.
- Four teams share lead tokens with another team, because one lead session
  ran two teams at once: `ctxtrace` with `fixall`, and `deckmbe` with `deckux`.
  The `lead_shared_with` column marks them. Their lead cost is counted in both
  rows.

## 2. Part two: the pilot benchmark

### 2.1 The tasks and the setups

| Task | Commits | Hidden tests | New tests a do-nothing run fails |
| --- | --- | --- | --- |
| Three fixes (suits a team) | 9b05edc8 (report), 40eedcf2 (msg), f0fe42ae (ui), all on parent e78c32be | 53 | 13: 2 report, 2 msg, and the whole ui test file (9) |
| Close cache (suits one agent) | 32b8a283 (close), parent f1ac22cb | 39 | 2 |

The user picked these tasks on 2026-10-22. Each brief states the end state
and the interfaces the hidden tests call. No brief holds code or tests. The
briefs are kept with the results (section 6).

The grader first ran each task's hidden tests on the parent plus the task's
commits. That run fixes which tests are expected to pass. Then it graded an
untouched clone (a "null run"). The null run must fail the new tests, and it
did.

Every run used Opus 5.5 at high effort, a fresh clone made from a git bundle
that ends at the parent, and claude-mem turned off. Each setup ran like this:

| Setup | How it ran |
| --- | --- |
| Solo | `claude -p --model opus --max-budget-usd 4 --permission-mode bypassPermissions --disallowedTools Agent Workflow`, with the brief on standard input |
| Solo with subagents | The same, with only `Workflow` blocked |
| teamwire | An interactive Opus session in a tmux session, with a fresh `TEAMWIRE_HOME`, first message `/teamwire:afk <brief>` plus one setup line that turned claude-mem off for members |

### 2.2 Results

| Task | Setup | Team the lead chose | Hidden tests | Fast gate | Files outside the task | Blind grade (correctness / scope / tests) | Cost | Wall time |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Close cache | solo | (one session) | 39/39 | pass | 1: its own new test file | 3 / 3 / 3 | $0.78 | 2m45s |
| Close cache | solo with subagents | no subagent started | 39/39 | pass | 1: its own new test file | 3 / 3 / 3 | $0.79 | 2m40s |
| Close cache | teamwire | 4 members: implementer, reviewer, closer, reporter, all Opus | 39/39 | pass | 2: its own new test file, `docs/api/cli.md` | 2 / 1 / 3 | $13.67 | 48m58s |
| Three fixes | solo | (one session) | 53/53 | pass | 0 | 3 / 3 / 3 | $1.78 | 15m24s |
| Three fixes | solo with subagents | no subagent started | 53/53 | pass | 0 | 3 / 3 / 2 | $1.45 | 6m33s |
| Three fixes | teamwire | 10 members: planner, plan reviewer, 3 implementers, 3 reviewers, closer, reporter, all Opus | 53/53 | pass | 3: `docs/api/cli.md`, `docs/api/mcp.md`, a new ui test file | 2 / 3 / 3 | $32.86 | 71m53s |

Every hidden test passed, so the cost per passing task equals the cost.
Nobody typed into any run. The leak scan found no read of the source
checkout, `~/.teamwire/archive`, `~/.claude/projects` or another run's folder
in any of the 20 transcripts.

### 2.3 Paired differences, per task

| Task | teamwire minus solo: cost | teamwire minus solo: wall time | Blind grade difference |
| --- | --- | --- | --- |
| Close cache | +$12.89 (×17.5) | +46m (×17.8) | correctness −1, scope −2 |
| Three fixes | +$31.08 (×18.5) | +56m (×4.7) | correctness −1 |

The solo and subagents setups differ by $0.01 and by $0.33. The three-fix
task took 15m24s in one solo run and 6m33s in the other, with the same
setup in practice. That gap shows how much one run can vary by itself.

The two lower teamwire grades have different causes:

- **Close cache, scope 1:** the teamwire run also deleted caches that other
  teams left behind, refactored the deletion code and edited the CLI docs.
  The brief asked for none of that. During the run, the team's closer also
  found that the first version followed a symbolic link and could delete
  folders outside teamwire's own. Fix round 2 added a guard. The hidden tests
  do not cover this case, and the solo runs never wrote the extra deletion
  that caused it.
- **Three fixes, correctness 2:** the grader marked down the teamwire diff
  for leaving out unread deliveries addressed to another team. Its own reason
  says the brief does not ask for that. The grade is grader noise, not a
  defect.

## 3. Methods that held up, and what went wrong

- The cost script's total matched Claude's own `total_cost_usd` to the cent
  on all four solo runs.
- For the close-cache teamwire run, the cost script's per-member costs were
  within 2% of teamwire's own member counters. The script read slightly
  higher each time. teamwire takes its count when a member delivers its
  result, so it probably misses the tokens after that point. The lead's $2.55
  appears only in the transcript.
- The review before the first run found five problems that would have
  changed numbers without any error. One example: subagent messages were
  priced from a placeholder line, about 22% low. All five were fixed before
  any run (`bench/` on branch `gainsbuild/tools`).
- **The budget was exceeded.** The plan set $15 per teamwire run and $50 in
  total. The AFK skill's budget is a time limit, so the lead watched the cost
  from outside. During the three-fix teamwire run, that watcher reached its
  30-minute limit at about $14, while the lead was waiting on the user's
  answer about raising the cap. The user raised the cap to $30. The run
  finished on its own at $32.86. The benchmark runs cost $51.34 in total.
- **Members ran on Opus.** Both AFK leads put every member on Opus, the
  implementers included. The skill's role table suggests Sonnet for
  implementers. This choice is part of what the study measured, and it
  explains part of the cost.

## 4. What the pilot cannot show

- With 2 tasks and one run per setup, any difference could come from
  run-to-run variation alone. The two runs of the same solo setup differed by
  2.4 times in wall time.
- Both tasks were small, and one session solved both completely. The pilot
  has no task where one agent fails, which is where a team could gain.
- The blind grade comes from one grader call. One of its six reasons
  contradicts itself.
- The "solo with subagents" setup never started a subagent. The pilot says
  nothing about subagents.
- The benchmark tasks come from teamwire's own history. A lead could also
  read the original work on disk. The leak scan rules that out for these
  runs, but only after the fact.

## 5. Cost of a full run, and the decision it needs

Per task and repeat, the pilot spent about $1.6 per solo run and $14 to $33
per teamwire run. One repeat of all three setups on one task costs about $20
to $36.

| Scale | Runs | Estimated cost |
| --- | --- | --- |
| 6 tasks × 3 setups × 3 repeats | 54 | $360–650 |
| 10 tasks × 3 setups × 3 repeats | 90 | $600–1,100 |
| 10 tasks × 2 setups (drop subagents) × 3 repeats | 60 | $500–1,000 |

Before scaling up, the user needs to decide:

1. **Whether to scale up, and to what budget.** The pilot found no accuracy
   gain on small tasks, at about 18 times the cost.
2. **Whether to add harder tasks.** A task that one session fails would test
   where a team could gain. The candidate list holds only commits of 30 to 200
   lines.
3. **What to do with the subagents setup.** Either drop it, or tell that
   session to use subagents.
4. **Whether to pin member models** in the teamwire setup, for example Sonnet
   implementers, or to keep measuring what the AFK lead chooses.
5. **A cost cap that holds.** Either teamwire gets a dollar budget, or a
   watcher with no time limit runs outside the lead session.

## 6. Where everything is

- The tools are in `bench/`. `bench/README.md` lists the commands, and
  `bench/run/` holds the scripts that run a benchmark end to end.
- The briefs are in `bench/briefs/`: `close-cache.md` (called t2 above) and
  `three-fixes.md` (t1).
- The results are in `bench/studies/2026-10-gains/`. Its `README.md` lists
  what each folder holds.
  - The re-mined archive table: `archive-teams.csv`.
  - Both pilot teamwire archives: `tw-archives/closecache-2026-10-22/` and
    `tw-archives/fixes3-2026-10-22/`.
- The pilot's per-run files, its first `mine.csv` and its `candidates.csv` sat
  in `/private/tmp`. macOS's temp cleanup deleted them before 2026-10-28. This
  report keeps their numbers.

Costs outside the benchmark: building the tools cost about $22 ($3.76
Sonnet implementer at the corrected cache-read price, $4.29 reviewer, $13.70
lead session). The blind grader cost $2.50. The pilot's six benchmark runs
used only Opus, so the price correction does not change them.
