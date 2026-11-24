# Research: evidence for Gemini 4 Neon and Gemini 4 Carbon

Written 2026-11-24. Every page below was opened and read on 2026-11-24. This
note adds to `docs/research/2026-11-20-evidence-refresh.md` and follows the
rules of `docs/research/2026-11-03-model-evidence.md` for `rank`,
`published`, `effort` and `kind`. Where an older note describes an entry this
note changes, this note is the current account.

## What changed

`examples/models/evidence.json` now holds 401 entries. On 2026-11-20 it held
326. No entry was removed, and no existing claim was reworded.

| Change | Entries |
| --- | --- |
| "CursorBench 4.1 leaderboard (Cursor)", a new source, every row | 53 added |
| "Terminal-Bench 4.1 leaderboard": Carbon at `high`, Neon at `xhigh` | 2 added |
| "Artificial Analysis Coding Agent Index: SWE-Atlas-QnA-AA": Neon at `xhigh` | 1 added |
| "Introducing Claude Fable 5.2: FrontierCode 2.0": Carbon at `low`, `medium`, `high` | 3 added |
| "Gemini API thinking guide": Neon | 1 added |
| "Gemini API: Choosing a model", a new source | 4 added |
| "Introducing Gemini 4 Neon: Terminal-Bench 4.1", a new source | 3 added |
| "Introducing Gemini 4 Neon: FrontierCode 2.0", a new source | 3 added |
| "Introducing Gemini 4 Carbon: Terminal-Bench 4.0 (AA)", a new source | 5 added |

So 326 + 75 = 401. The file's `as_of` moved to 2026-11-24. Only the entries
named here were read on 2026-11-24.

## Both models are in scope

On 2026-11-24, `teamwire models --harness pi` lists
`openrouter/google/gemini-4-carbon` and `openrouter/google/gemini-4-neon`.
The 2026-11-19 note found Neon on openrouter.ai but not yet on pi's list;
it is on the list now.

pi's catalog (`pi-ai`, `providers/data/openrouter.json`) maps the levels as
Google's thinking guide does:

| Model | Levels pi sends | Google's default | pi price, input / output per 1M tokens |
| --- | --- | --- | --- |
| `gemini-4-carbon` | `low`, `medium`, `high` | `medium` | $2 / $10 |
| `gemini-4-neon` | `low`, `medium`, `high`, `xhigh` | `high` | $3 / $15 |

So every Neon row below, `xhigh` included, is a level a pi member can run.
Carbon has no `xhigh`.

Carbon's $2 / $10 is an introductory price that ends on 2026-11-30. From
2026-12-01 Google charges $4 / $20 (Gemini API "Choosing a model" page). pi's
catalog holds the introductory price, and `teamwire metrics` shows the cost a
harness recorded (ADR 0060). A Carbon member run after 2026-11-30 on this pi
release will show half its real cost. Neon has no introductory price.

## What a lead sees now

| Class | Neon | Carbon |
| --- | --- | --- |
| `contained_feature` | Terminal-Bench 4.1 board: `xhigh` 61.2%, rank 4. Google's table: 65.2% (Terminal-Bench 4.1), 46.2% at `high` (FrontierCode 2.0), each 2nd behind Opus 5.5 | Terminal-Bench 4.1 board: `high` 64.5%, rank 2, behind only Opus 5.5 and Fable 5.2 at `max`. Fable 5.2 page, FrontierCode 2.0: `medium` 41.1% beats `high` 38.4%. Google's table: 65.2% on Terminal-Bench 4.0 (AA), 1st |
| `cross_cutting` | CursorBench 4.1: `xhigh` 65.4% rank 1, `high` 63.7% rank 2, `medium` 57.7% rank 9, `low` 45.5% rank 26. Google's guide names Neon for multi-file refactoring | CursorBench 4.1: `high` 61.5% rank 3, `medium` 60.4% rank 6, `low` 55.0% rank 14. Google's guide: escalate to Carbon when Neon gets stuck |
| `read_only_survey` | SWE-Atlas-QnA-AA: `xhigh` 67.7%, rank 1 of the kept rows | SWE-Atlas-QnA-AA: `high` 63.7%, rank 5 (entry from 2026-11-20) |
| `root_cause` | Google's guide, guidance only | Google's guide, guidance only |
| `judgement` | Thinking guide, guidance only | Thinking guide, guidance only (entry from 2026-11-20) |

Three findings matter for picking a seat:

- **Neon at `high` beats Carbon at every level on CursorBench 4.1, for less
  money.** Neon `high` scored 63.7% at $6.54 a task, Carbon `high` 61.5% at
  $8.52. Neon `medium` scored 57.7% at $3.07, with 41 steps a task against
  Carbon `high`'s 97. Cursor's costs use Carbon's published price; the page
  does not say whether that is the introductory one.
- **Carbon's default level, `medium`, is not worse than `high` on the two
  sources that measured both.** On FrontierCode 2.0 Carbon `medium` scored
  41.1% and `high` 38.4%. On CursorBench 4.1 `high` led `medium` by 1.1
  points, which Cursor says may not be meaningful. Neither source supports
  paying for Carbon at `high` by default.
- **Every Gemini row on the public boards ran in Google's Antigravity CLI.**
  Terminal-Bench 4.1 and SWE-Atlas-QnA-AA have no Gemini row in another
  harness. CursorBench does not name its harness. No source measured Neon
  or Carbon in pi, the harness a teamwire member would use. The 2026-11-19
  note records FrontierCS 2, where Gemini 4 Argon passed 54% of tasks in
  Antigravity and 40% in pi, so a harness gap of that size is possible.

## Sources

### CursorBench 4.1 leaderboard (Cursor)

<https://cursor.com/cursorbench>. The board's table has 53 rows; the file
keeps all 53, because every model on it is one a harness here reaches (the
Gemini 4 models, Fable 5.2, Opus 5.5, Sonnet 5.5, Haiku 5.5, Grok 4.7, Grok
4.6, GPT-5.6 Sol and Muse Spark 1.3).

- Measures: Cursor's tasks, which CursorBench 4.0 made long-horizon: edit,
  refactor, investigation, intent understanding, managing jobs and design
  adherence. Version 4.1, dated 2026-11-18 in the changelog, changed the
  instruction-following problems and raised the agent's time limit. Each row
  gives score, average cost, tokens and steps per task.
- Informs `cross_cutting`, as CursorBench 4.0 already does through the Opus
  5.5 and Sonnet 5.5 pages.
- `published` is 2026-11-18 for every row: the board shows no row dates, and
  4.1 is the version every row was scored on.
- `effort` maps the board's labels: Extra High to `xhigh`, Max, High,
  Medium, Low and Minimal to their lower-case names.
- Ranks: the board numbers its rows 1 to 53. The file gives equal scores one
  rank, as the 2026-11-03 rule says, so Fable 5.2 `high` and Argon `high`
  (both 56.5%) share rank 12, and the file's ranks run 1 to 52.
- Limits: Cursor sells an editor and wrote the tasks. Its cost column applies
  each model's list price to the tokens used. The page says small differences
  may not be statistically meaningful, and gives no error bars.

### Terminal-Bench 4.1 leaderboard

<https://www.tbench.ai/>. Two rows added:

| Model | Level | Score | Board's release date | Cost, as the board rounds it |
| --- | --- | --- | --- | --- |
| Gemini 4 Carbon | `high` | 64.5% ±3.2 | 2026-10-28 | $1.4k |
| Gemini 4 Neon | `xhigh` | 61.2% ±2.8 | 2026-11-18 | $1.9k |

The older rows give two decimals and exact costs. The board's table shows
one decimal and rounded costs, and the two new claims give what the table
shows.

The board also has rows this refresh did not add: Copilot Fusion at `max`
(71.2%, now first), GPT-6.2 Astra `max` (62.1%), GPT-6.2 Sol `max` (61.5%),
Gemini 4 Hydrogen `high` (56.4%) and Gemini 4 Argon `high` (55.5%). A later
refresh could add the last four; Copilot Fusion is GitHub Copilot's router,
not a model a harness here lists.

### Artificial Analysis Coding Agent Index, SWE-Atlas-QnA-AA

<https://artificialanalysis.ai/agents/coding-agents>. The page's embedded
data now says it was materialized 2026-11-24, so the Neon entry carries
2026-11-24. The older entries keep 2026-11-19; the Carbon score (63.7%) is
unchanged.

- Neon: mean reward 0.677 (67.7%) on 372 attempts in Antigravity CLI, none
  refused. The page lists four Neon levels by name but measures only `xhigh`.
- Muse Wonder Full at `max` (68.8%) scores above Neon on the board. The file
  has never kept Muse Wonder Full, so Neon is rank 1 of the kept rows.
- The same rows hold the index's two other parts, FrontierCode 2.0 and
  Terminal-Bench v4. Neon `xhigh` scored 43.0% and 65.2% on them. The file
  does not hold these parts for any model; a later refresh could add them to
  `contained_feature`.

### Introducing Claude Fable 5.2: FrontierCode 2.0

<https://www.anthropic.com/claude-fable-and-mythos-5-2>. The chart data has
three Carbon points: `low` 32.4% at $5.49, `medium` 41.1% at $5.16, `high`
38.4% at $5.83 a task. The table on the same page gives 41.1% at medium.

The file kept only the best level for the other rival models on this chart
(GPT-6 Astra, Fable 5.1). It keeps all three Carbon levels, because the
fall from `medium` to `high` is evidence on which level to run. The page
does not say who ran Carbon or in which harness.

### Gemini API thinking guide

<https://ai.google.dev/gemini-api/docs/thinking>. Undated, so the new entry
carries 2026-11-24. It says `high` is Neon's default and recommended level
"for most tasks, including complex coding", and `xhigh` is for "hard,
multi-step coding and agentic problems where `high` falls short". The entry
shares rank 1 with the Carbon and Argon entries, since each gives one
model's advice.

### Gemini API: Choosing a model

<https://ai.google.dev/gemini-api/docs/choosing-a-model>. Undated, so the
entries carry 2026-11-24. Its "When to choose each model" section says:

- Neon: "Choose it for multi-file refactoring, repository-wide debugging,
  and analytical tasks where a developer or coding agent is waiting on the
  response."
- Carbon: "Carbon spends more thinking tokens per turn to test hypotheses,
  isolate root causes across repositories, and check fixes against tests.
  Choose it for multi-hour background runs, deep research, formal proofs, and
  security audits, or escalate to Carbon when Neon at `high` or `xhigh`
  thinking gets stuck."

The file reads the first as `cross_cutting` advice and the second as
`root_cause` advice, and ranks each model 1 in the class the guide names it
for and 2 in the other. `effort` is empty: the guide ties no level to these
choices.

### Google's own benchmark tables

`kind` is `vendor`: Google ran or chose the numbers for its own model.

- <https://blog.google/innovation-and-ai/models-and-research/gemini-models/introducing-gemini-4-neon/>,
  2026-11-18, last updated 2026-11-23. The table is an image. It compares
  Neon with Opus 5.5 and GPT-6.2 Sol. The file keeps two rows of it:
  - Terminal-Bench 4.1, "cheating adjusted accuracy": Opus 5.5 65.8%, Neon
    65.2%, GPT-6.2 Sol 61.5%. No effort is stated.
  - FrontierCode 2.0, main score: Opus 5.5 47.3%, Neon 46.2% at `high` (the
    page's text gives the level), GPT-6.2 Sol 43.8%.
- <https://blog.google/innovation-and-ai/models-and-research/gemini-models/introducing-gemini-4-carbon/>,
  2026-10-28. The table is an image. It compares Carbon with Argon, GPT-6
  Astra, Opus 5.5 and Fable 5.1. The file keeps Terminal-Bench 4.0 as
  Artificial Analysis ran it: Carbon 65.2%, Opus 5.5 59.6%, GPT-6 Astra
  59.1%, Argon 57.1%, Fable 5.1 52.0%. No effort is stated.

## Where sources disagree

- **Neon on Terminal-Bench 4.1.** Google's table gives 65.2%; the public
  board shows Neon `xhigh` at 61.2% ±2.8. Google's label, "cheating adjusted
  accuracy", names a different scoring, and the page does not say which
  level it ran. Artificial Analysis's Terminal-Bench v4 part gives Neon
  `xhigh` 65.2% (0.6515), which matches Google's figure. The file keeps both,
  each under its own source.
- **Neon against Carbon.** Google's guides place Carbon above Neon for the
  hardest work. On the measured boards Neon leads: CursorBench 4.1 (65.4%
  against 61.5%) and SWE-Atlas-QnA-AA (67.7% against 63.7%). Carbon leads on
  Terminal-Bench 4.1 (64.5% against 61.2%).

## Pages opened and not used

| Page | Why not used |
| --- | --- |
| Scale SWE-Bench Pro (public and V2), SWE Atlas Refactoring, SWE Atlas Codebase QnA, swebench.com, Aider, CodeReviewBench | No Gemini 4 Carbon or Neon row on 2026-11-24 |
| FrontierCS 2, <https://frontier-cs.org/> and its Carbon post | The board is drawn by script, with no effort per row. Its Neon row mixes harnesses: on 22 tasks Antigravity cut the run short, and the site shows Neon's run on pi instead. Research-code reproduction fits no class well. |
| Cognition, FrontierCode 2.0 post | The page names no Gemini 4 row |
| Gemini 4 Neon model card (PDF) | Its results page is an image the text extract does not hold; the blog's table gives the coding rows |
| Google Antigravity Neon post | Product news; repeats the blog's scores |
| SWE-sweep and FrontierSWE rows of Google's tables | The pages do not say what these benchmarks measure, so no class can be chosen |
| 9to5Google, VentureBeat, The Register | Secondary reports of Google's numbers |

## Gaps

- **No Neon or Carbon run in pi.** Every measured row ran in Antigravity CLI
  or an unnamed harness.
- **No code review or judging result.** IronBee, CodeReviewBench and Qodo
  still have no Neon or Carbon row. The `judgement` entries are guidance
  only.
- **Neon below `xhigh` on codebase reading.** SWE-Atlas-QnA-AA measures only
  `xhigh`; only CursorBench has Neon at `low`, `medium` and `high`.
- **Carbon's price after 2026-11-30.** The cost figures in every source use
  the introductory price or do not say.
