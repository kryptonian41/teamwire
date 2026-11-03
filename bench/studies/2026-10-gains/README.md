# Gains study results, October 2026

The results behind two reports:
`docs/research/2026-10-22-gains-study.md` (the pilot and the archive) and
`docs/research/2026-10-26-gains-complex-task.md` (the AFK feature, two rounds).
The plan is `docs/research/2026-10-22-gains-study-plan.md`.

| Folder | What it holds |
| --- | --- |
| `afk-round2/<setup>/` | Round 2 of the AFK task (2026-10-26 to 27, medium effort): `score.json`, `diff.patch`, `cost.json` and `cost.txt`, `leaks.txt`, raw test output. `reference/` holds the expected set for the task. |
| `afk-round1/<setup>/` | Round 1 (2026-10-23, high effort). Only `teamwire/` is complete. `solo/` and `subagents/` keep only `diff.patch`. `solo-briefv1/` is the run with the incomplete brief. `null*/` are do-nothing runs. |
| `blind-afk-round1/`, `blind-afk-round2/` | The blind grader's inputs: rubric, shuffled diffs, order |
| `blind-keys/` | Which diff id belongs to which setup, and the grader's raw answers |
| `tw-archives/` | The teamwire team archives of the benchmark runs |
| `archive-teams.csv` | `bench/mine.py` over 96 archived teams, mined 2026-10-28 with corrected prices |

What is missing, and why:

- The pilot's per-run files (tasks three-fixes and close-cache) and its first
  `mine.csv` sat in `/private/tmp`, and macOS's temp cleanup deleted them.
  The pilot report keeps their numbers.
- Every team archive has had its `members/` folders removed, because they hold
  `TEAMWIRE_TOKEN` values.
- `tw-archives/fixes3-2026-10-22/afk/tmp/` had two reviewers' scratch clones of
  the repository (75 MB, with `node_modules`). They are removed. That archive's
  `SHA256SUMS` still lists their files.
- Cost files written before 2026-10-26 used a Sonnet 5.5 cache-read price of
  $0.20 per million tokens. The right price is $0.10. `afk-round1/teamwire/cost-repriced.json`
  holds the corrected round 1 teamwire cost.
