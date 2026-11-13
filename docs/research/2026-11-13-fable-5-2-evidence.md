# Research: public evidence on Claude Fable 5.2

Written 2026-11-13. Anthropic released Claude Fable 5.2 (`claude-fable-5-2`)
on 2026-11-09. Every page below was opened and read on 2026-11-13. This note
adds to `docs/research/2026-11-03-model-evidence.md` and does not replace it.
That note still explains how the evidence file fills `rank`, `published`,
`effort` and `kind`, and this note follows the same rules.

The 222 entries from the 2026-11-03 note were read on 2026-11-03 and were not
re-read. Some of their pages have changed since then (see "Older entries that
are now out of date"). The file's `as_of` moved to 2026-11-13 because the new
entries carry dates up to that day. So `as_of` is now the age of the newest
claims only. The older claims are ten days older than it.

## What was added

`examples/models/evidence.json` now holds 246 entries, 24 more than before.

- 16 entries are Fable 5.2 rows.
- 8 entries are comparison rows for other models. A comparison row is added
  only in a source that is new to the file. It is the single best row that
  source's page gives for a model the 2026-11-03 note already covers. Without
  them, a new source would rank Fable 5.2 first against nothing. Each
  comparison row's claim ends with "Added for comparison with Fable 5.2."
- In a source group that already existed, only Fable 5.2 rows were added, and
  the group was renumbered.

The Claude Code model list in `crates/teamwire-harness-claude/src/models.rs`
now starts with `claude-fable-5-2`, which carries the `fable` alias, a 1M
window and the five effort levels. `claude-fable-5-1` and `claude-fable-5`
stay on the list with no alias.

## Sources

All pages were read on 2026-11-13.

### Claude Code: Model configuration

<https://code.claude.com/docs/en/model-config>. Undated.

- The `fable` alias resolves to Fable 5.2 everywhere except Claude apps
  gateway sessions, where it resolves to Fable 5. The version table says this
  holds from Claude Code v2.1.321, and Fable 5.2 needs v2.1.321 or later.
- The effort table gives "Fable models" the levels `low`, `medium`, `high`,
  `xhigh` and `max`.
- Fable 5.2 defaults to `medium` in Claude Code, like Opus 5.5, Sonnet 5.5 and
  Haiku 5.5.
- "Fable models" run with the 1M window by default, with no `[1m]` suffix.
- Thinking cannot be turned off on the Fable models.
- Used for: the model list, not the evidence file.

### Anthropic: Models overview

<https://platform.claude.com/docs/en/about-claude/models/overview>. Undated.

- API id and alias `claude-fable-5-2`. Bedrock id
  `global.anthropic.claude-fable-5-2`.
- 1M context window, 128K max output, adaptive thinking always on.
- Default effort on the API: `high`.
- Price: $8 per million input tokens and $40 per million output tokens.
- Retirement not sooner than 2027-11-09.
- It says: start with Opus 5.5 for most workloads, and use Fable 5.2 for
  demanding reasoning and long-horizon agentic work, or when evals on Opus 5.5
  at higher effort still fall short.
- Used for: checking the model list. The file cites the choosing-a-model page
  for the same advice, so the overview adds no entry.

### Anthropic: Pricing

<https://platform.claude.com/docs/en/about-claude/pricing>. Undated.

- Fable 5.2: $8 input, $40 output, $10 for 5-minute cache writes, $16 for
  1-hour cache writes, $0.20 for cache hits, all per million tokens.
- Fast mode: $16 input and $80 output per million tokens.
- Used for: this note only. Price is not a task-class claim.

### Introducing Claude Fable 5.2 and Claude Mythos 5.2

<https://www.anthropic.com/claude-fable-and-mythos-5-2> (also served at
`/news/claude-fable-and-mythos-5-2`). Dated 2026-11-09, with an edit note of
2026-11-11 about the formalization section.

- Measures: vendor-run scores on FrontierCode 2.0, FrontierSWE v2,
  AutomationBench, Terminal-Bench-Science, cybersecurity, robotics, search,
  knowledge work, Humanity's Last Exam and chart reading.
- The page's charts label each point with its effort and cost per task. The
  file takes the FrontierCode 2.0 chart, read from the chart's point labels:

  | Model | Low | Med | High | Xhigh | Max |
  | --- | --- | --- | --- | --- | --- |
  | Fable 5.2 | 34.2% $2.10 | 37.6% $2.48 | 45.3% $6.72 | 49.1% $14.19 | 45.6% $44.70 |
  | Opus 5.5 | 14.7% $0.75 | 30.1% $1.70 | 34.5% $2.42 | 43.4% $5.18 | 45.5% $12.82 |
  | Fable 5.1 | 20.6% $3.01 | 21.5% $3.89 | 25.7% $5.39 | 32.4% $8.68 | 31.7% $11.46 |
  | GPT-6 Astra | 31.1% $2.44 | 36.8% $3.49 | 38.3% $4.28 | 41.4% $4.54 | 42.8% $6.68 |

  The chart also shows Gemini 4 Carbon, which no harness reaches, so the file
  leaves it out.
- It says that in Claude apps and Claude Code `medium` is the default, and
  that at `medium` Fable 5.2 finishes hard tasks with fewer tokens than Fable
  5.1.
- It says to start most coding and knowledge work on Opus 5.5, and to move up
  to Fable for the most ambitious, open-ended technical work. It names Fable
  5.2 as a fit for the Advisor and Orchestrator roles.
- Informs `contained_feature` (FrontierCode 2.0).
- Limits: run and chosen by the vendor. The table's scores use each model's
  highest available effort unless a cell says otherwise. FrontierCode 2.0 is
  a different version from the FrontierCode v1.1 the file already holds, so
  the two stay separate sources.

### Anthropic: Optimizing for cost and intelligence

<https://platform.claude.com/docs/en/about-claude/models/optimizing-for-cost-and-intelligence>.
Undated. Its Fable 5.2 runs are dated 2026-11-03 to 2026-11-09 in its
references, some of them before release.

- **SWE-bench Pro subset.** On the 463 of 478 problems whose reference
  solutions pass without network access, Fable 5.2 at `high` (its API default)
  solved 96.3% for $1.39 per solved task, and at `low` 94.6% for $0.39. Both
  are single runs. On the same 463 problems, Fable 5.1 at its default solved
  92.4% for $1.17, and Opus 5.5 solved 93.1% at `medium`, 95.5% at `high` and
  96.5% at `xhigh` ($0.22, $0.30 and $0.75).
- **Internal agentic-coding benchmark.** 370 repository tasks graded by the
  repositories' own tests, one attempt per task. Fable 5.2 scored 88.9% at
  `low` ($1.05 per attempt), 88.9% at `medium` ($1.34) and 93.5% at `high`
  ($5.39). On the same page, Opus 5.5 scored 74.8% at `low`, 86.6% at
  `medium`, 88.4% at `high` and 91.1% at `xhigh` ($4.11). Fable 5.1 scored
  85.7% at `high`. Sonnet 5.5 scored 77.8% at `high` and 62.7% at `medium`.
- **Defect search.** A corpus of about 22 million tokens with 130 planted
  defects, 10 runs per setting. Fable 5.2 alone scored a mean F1 of 0.866 at
  `low` ($357 a run), 0.877 at `medium` ($378) and 0.909 at `high` ($563).
  Fable 5.1 alone scored 0.829 to 0.847 across the same three efforts. As
  orchestrator in Opus 5.5's place, Fable 5.2 cost 19% more and scored about
  the same.
- **DeepResearch Bench II.** Fable 5.2 scored 54.5%, 55.0% and 57.7% at `low`,
  `medium` and `high`. The page says Fable 5.2 scored below Fable 5.1 and Opus
  5.5 on this research benchmark.
- It repeats the advice to use Fable 5.2 when evals on Opus 5.5 at higher
  effort still fall short.
- Informs `contained_feature` (SWE-bench Pro subset and the internal
  agentic-coding benchmark) and `read_only_survey` (defect search).
- Limits: internal and run by the vendor. Most Fable 5.2 figures are single
  runs. The SWE-bench Pro subset counts 463 problems for Fable 5.2 and 478 for
  the older entries. The ranks still hold on either count (see "Groups
  re-ranked").

### Anthropic: Choosing a model

<https://platform.claude.com/docs/en/about-claude/models/choosing-a-model>.
Undated.

- Says: "If your evals at xhigh or max effort still fall short on demanding
  reasoning or long-horizon agentic work, move to Claude Fable 5.2."
- Says: on Fable 5.2, start with the default (`high`) and adjust by evals.
- Names Fable 5.2 as "the highest available capability", for agent sessions
  that run for hours and multistep deep research.
- Informs `cross_cutting` and `concurrency_data_loss`, the two classes where
  the file already holds the same advice for Fable 5.1. The
  `concurrency_data_loss` mapping of "demanding reasoning" is this note's
  inference, as it was in the 2026-11-03 note.

### Anthropic effort documentation

<https://platform.claude.com/docs/en/build-with-claude/effort>. Undated.

- Says: Fable 5.2 supports all five effort levels, and the API default is
  `high`. Run an effort sweep on your own evals rather than carrying settings
  over from Fable 5.1.
- Used for: this note only. The page names no task class for Fable 5.2, and
  its advice gives no reason to rank Fable 5.2 against the other models in
  any class.

### Terminal-Bench 4.1 leaderboard

<https://www.tbench.ai/>. The table is in the page's embedded data, read on
2026-11-13.

- The board now names itself Terminal-Bench 4.1 and has re-scored its older
  rows (see "Older entries that are now out of date").
- Fable 5.2 at `max` in Claude Code, dated 2026-11-09: 65.76% ±2.85, 330
  trials, $16,492.76.
- Comparison rows, as the lead asked: Opus 5.5 at `max`, 65.76% ±3.11,
  $4,718.65 (2026-09-22), and Fable 5.1 at `max`, 57.27% ±3.83, $6,243.50
  (2026-09-01).
- Informs `contained_feature`.
- Limits: Fable 5.2 has one row, at `max` only. It ties Opus 5.5 at `max` for
  about 3.5 times the cost.

## Groups re-ranked

| Group | Change |
| --- | --- |
| Anthropic: Optimizing for cost and intelligence, `contained_feature` (SWE-bench Pro subset) | Fable 5.2 `high` (96.3%) enters at rank 2, between Opus 5.5 `xhigh` (96.7%) and Opus 5.5 `high` (95.3%). Fable 5.2 `low` (94.6%) enters at rank 4, between Opus 5.5 `high` and Opus 5.5 `medium` (92.8%). The six entries below them move down: Opus 5.5 `high` 2 to 3, Opus 5.5 `medium` 3 to 5, Fable 5.1 `high` 4 to 6, Sonnet 5.5 `high` 5 to 7, Opus 5.5 `low` 6 to 8, Sonnet 5.5 `medium` 7 to 9. The order is the same on the 463-problem count the page gives for Fable 5.2. |
| Anthropic: Optimizing for cost and intelligence, `read_only_survey` (defect search) | Fable 5.2 takes ranks 1 to 3 (`high`, `medium`, `low`). The Fable 5.1 entry moves from 1 to 4, because every Fable 5.1 setting scored below Fable 5.2 at `low`. |
| Anthropic: Choosing a model, `cross_cutting` | Fable 5.2 shares rank 2 with Fable 5.1. Both entries carry the same advice. |
| Anthropic: Choosing a model, `concurrency_data_loss` | Fable 5.2 shares rank 1 with Fable 5.1. Opus 5.5 stays at 2. |

## New sources

| Source in the file | Class | Entries, rank order |
| --- | --- | --- |
| Terminal-Bench 4.1 leaderboard | `contained_feature` | 1 Fable 5.2 `max` 65.76%; 1 Opus 5.5 `max` 65.76% (comparison); 2 Fable 5.1 `max` 57.27% (comparison) |
| Introducing Claude Fable 5.2: FrontierCode 2.0 | `contained_feature` | 1 Fable 5.2 `xhigh` 49.1%; 2 Fable 5.2 `max` 45.6%; 3 Opus 5.5 `max` 45.5% (comparison); 4 Fable 5.2 `high` 45.3%; 5 GPT-6 Astra `max` 42.8% (comparison); 6 Fable 5.2 `medium` 37.6%; 7 Fable 5.2 `low` 34.2%; 8 Fable 5.1 `xhigh` 32.4% (comparison) |
| Anthropic: Optimizing for cost and intelligence: internal agentic-coding benchmark | `contained_feature` | 1 Fable 5.2 `high` 93.5%; 2 Opus 5.5 `xhigh` 91.1% (comparison); 3 Fable 5.2 `low` 88.9%; 3 Fable 5.2 `medium` 88.9%; 4 Fable 5.1 `high` 85.7% (comparison); 5 Sonnet 5.5 `high` 77.8% (comparison) |

The FrontierCode 2.0 and internal agentic-coding sources are `vendor`, because
Anthropic ran them. The Terminal-Bench 4.1 rows are `benchmark`.

The internal agentic-coding benchmark is its own source in the file, apart
from the page's SWE-bench Pro subset. Ranks then never mix two benchmarks,
as the 2026-11-03 note requires.

## What the sources say about effort on Fable 5.2

- **The best score is not at `max`.** On FrontierCode 2.0, Fable 5.2 scored
  49.1% at `xhigh` and 45.6% at `max`, for $14.19 against $44.70 a task.
- **`high` already matches Opus 5.5's best on FrontierCode 2.0.** Fable 5.2 at
  `high` scored 45.3% for $6.72. Opus 5.5 at `max` scored 45.5% for $12.82.
- **`low` and `medium` tie on the internal agentic-coding benchmark.** Both
  scored 88.9%, for $1.05 and $1.34 an attempt. `high` scored 93.5% for $5.39.
- **The default differs by surface.** Claude Code and the Claude apps run Fable
  5.2 at `medium`. The API default is `high`. A lead who sets no effort gets
  `medium` in a Claude Code member.

## Where sources disagree

| Claim | One source | The other source |
| --- | --- | --- |
| Fable 5.2 default effort | `medium` in Claude Code and the Claude apps (model configuration page; release page) | `high` on the API (models overview; effort docs; choosing-a-model page) |
| Fable 5.2 against Opus 5.5 | "a step above Opus 5.5 in almost all the capabilities we evaluated" (release page) | Tied with Opus 5.5 at `max` on Terminal-Bench 4.1 for 3.5 times the cost; below Opus 5.5 on DeepResearch Bench II; Opus 5.5 at `xhigh` level with Fable 5.2 at `high` on the SWE-bench Pro subset for a little over half the cost (cost page) |

The two default efforts do not conflict. Each source names its surface.

## Older entries that are now out of date

These entries were read on 2026-11-03 and were left as they were. The lead
decides whether to refresh them.

- **Terminal-Bench.** The 29 rows of "Terminal-Bench 4.0 leaderboard" predate
  the board's 4.1 re-score. The board now shows Opus 5.5 at `max` as 65.76%
  ±3.11, where the file's 4.0 row has 64.85%. It shows Fable 5.1 at `max` as
  57.27% ±3.83, where the file's 4.0 row has 57.88%. Other rows moved too, for
  example GPT-6 Astra at `high` from 57.88% to 60.00%.
- **Choosing a model.** The page now names Fable 5.2 where it named Fable 5.1.
  The two Fable 5.1 entries still quote the older wording.
- **Defect search.** The page now prices Fable 5.1 alone at $527 to $716 a run.
  The file's Fable 5.1 entry quotes $468 to $552 from 2026-11-03.

## What sources say about AFK use

This section records evidence only. Whether teamwire's AFK ranking or any
preset should use Fable 5.2 is an open question for the lead.

- The release page says Fable and Mythos are for "the most ambitious,
  open-ended technical work", and recommends starting most coding work on Opus
  5.5. It names Fable 5.2 for the Advisor and Orchestrator roles.
- An Uber engineer quoted on the release page ran a 10 to 12 hour unattended
  security audit with Fable 5.2 and says: "At High effort, I'd use this over
  what I run today for long unattended audits and doc-heavy work." This is a
  user report with no measured number, so the file does not carry it.
- The choosing-a-model page names Fable 5.2 for "agent sessions that run for
  hours".
- On the cost page's defect search, Fable 5.2 as orchestrator cost 19% more
  than Opus 5.5 as orchestrator and scored about the same.

## Pages opened and not used

- **Scale's SWE-Bench Pro V2, SWE Atlas Refactoring and SWE Atlas Codebase
  QnA boards.** Each shows Fable 5.1 and no Fable 5.2 row.
- **SWE-bench Verified (swebench.com), SWE-Bench Pro v1, CodeReviewBench and
  the Aider leaderboard.** None shows any Fable model, as on 2026-11-03.
- **FrontierSWE v2 on the release page.** Fable 5.2 scored 69.9%, Opus 5.5
  62.3%, Fable 5.1 56.3% and GPT-6 Astra 65.5%. Epoch AI
  (<https://epoch.ai/benchmarks/frontierswe>) describes it as 34 tasks with a
  20-hour budget each, covering implementation, performance, scientific
  computing, visual reasoning and AI research. No task class fits work of
  that length and spread, so the file leaves it out.
- **Other rows in the release page's table.** AutomationBench, Terminal-Bench
  -Science, CyScenarioBench, CVE Rediscovery, protein design, WANDR, OSWorld,
  RoboDojo, AA-Briefcase, Remote Labor Index, Humanity's Last Exam and
  Chartography measure no task class the file covers.
- **The release page's Python-to-Rust migration.** It was done by "an earlier
  version of Fable 5.2", and its number (26 times faster start-up) measures
  the result, not the model. The file does not carry it.
- **Customer quotes on the release page.** Snowflake, Fundamental Research
  Labs and others give either no number or a number on a task no class
  covers.
- **DeepResearch Bench II on the cost page.** It measures web research, which
  no class covers.
- **The cost page's output-cap measurement.** Fable 5.2 at `high` solved
  20.3%, 77.4% and 78.2% of about 133 repository tasks with a per-turn cap of
  16,384, 64,000 and 128,000 tokens. It measures the cap, not the model
  against other models, so the file does not carry it.
- **Aggregator and news pages** (SD Times, The New Stack, aimlapi.com,
  cometapi.com, llmreference.com and others found by search). They copy
  numbers from Anthropic's page, so the file cites that page.

## Gaps

- **No public board has measured Fable 5.2 on code review, root cause,
  refactoring or read-only survey.** Its only public-board row is
  Terminal-Bench 4.1 at `max`. Every other Fable 5.2 number comes from
  Anthropic.
- **No Fable 5.2 entry exists for `mechanical_edit`, `root_cause` or
  `judgement`.** No source gives a measured number for Fable 5.2 in those
  classes.
- **Effort sweeps on public boards.** Terminal-Bench 4.1 has Fable 5.2 at `max`
  only. The effort sweeps in the file come from Anthropic's own runs.
