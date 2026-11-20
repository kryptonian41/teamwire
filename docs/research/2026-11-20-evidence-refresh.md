# Research: code review, judging and codebase-reading evidence for Gemini 4 and the newest models

Written 2026-11-20. Every page below was opened and read on 2026-11-20. This
note adds to `docs/research/2026-11-03-model-evidence.md`,
`docs/research/2026-11-13-fable-5-2-evidence.md` and
`docs/research/2026-11-14-evidence-refresh.md`. The 2026-11-03 note still
explains how the evidence file fills `rank`, `published`, `effort` and `kind`,
and this note follows the same rules. Where an older note describes an entry
this note changes, this note is the current account.

## What changed

`examples/models/evidence.json` now holds 326 entries. On 2026-11-14 it held
247.

| Change | Entries |
| --- | --- |
| The old rows of the source "CodeReviewBench leaderboard (Kodus)" | 4 removed |
| The same source, as the board shows it today | 12 added |
| "IronBee raw LLM code review benchmark (Martian Code Review Bench offline set)" | 19 added |
| The same benchmark's 10-PR frontier set, its own source | 3 added |
| "IronBee code review benchmark: agreement of each judge with the panel" | 4 added |
| "Qodo: Which LLM should review your code?" | 10 added |
| "Artificial Analysis Coding Agent Index: SWE-Atlas-QnA-AA" | 31 added |
| A Gemini 3.8 Flash row in "SWE Atlas Codebase QnA leaderboard (Scale)" | 1 added |
| A GPT-6.2 Astra row in "OpenAI reasoning models guide" | 1 added |
| "Gemini API thinking guide" | 2 added |

So 247 − 4 + 83 = 326. The 83 added entries are the draft entries of the
2026-11-19 research report on the newest models. Each draft was checked
against its page on 2026-11-20 before it went in. None of the 83 was left
out: every page opened, and every model name, number and date in a draft
matched its page. Seven drafts were reworded or corrected, and the table under
"Drafts changed before they went in" lists them.

The file's `as_of` moved from 2026-11-14 to 2026-11-20. Only the entries
named in this note were read on 2026-11-20. Every other entry keeps the
reading date its own note gives. So `as_of` is the age of the newest claims
only.

Two classes gained entries. `judgement` went from 18 to 65 entries, and
`read_only_survey` went from 21 to 53.

## Which models are in scope now

The 2026-11-03 rule stands: an entry is in scope when a harness on this
machine reaches its model. On 2026-11-20, `teamwire models --harness pi`
listed 519 models, from `amazon-bedrock` and from `openrouter`. The
2026-11-03 note saw pi reach the open models through Amazon Bedrock only.
Through OpenRouter, pi now reaches Gemini, Muse, Inkling and the open models
of the newest boards.

The 2026-11-03 note names model versions and families that it dropped
because no harness reached them. This refresh fetched none of those dropped
rows again. The table lists each one that pi now reaches, as work for a later
refresh.

| Dropped on 2026-11-03 | pi id today | Rows seen on 2026-11-20 that a later refresh could add |
| --- | --- | --- |
| GLM 5.2 | `openrouter/z-ai/glm-5.2` | Scale SWE Atlas Codebase QnA: 48.12% ±5.07 in mini-SWE-agent, 5th on the board, above the new Gemini 3.8 Flash row |
| Kimi K2.7 Code | `openrouter/moonshotai/kimi-k2.7-code` | none on the pages read here |
| MiniMax M3 | `openrouter/minimax/minimax-m3` | IronBee and CodeReviewBench rows are now in the file; the 2026-11-03 sources were not re-read |
| Qwen3.8 | `openrouter/qwen/qwen3.8-27b`, `openrouter/qwen/qwen3.8-2.4t-a95b` | the CodeReviewBench Qwen3.8 27B row is now in the file; the 2026-11-03 sources were not re-read |
| Qwen3 235B A22B, the original checkpoint | not listed; pi lists only the 2507 checkpoints | none, still out of scope |
| Gemini | `openrouter/google/gemini-4-carbon`, `-argon`, `-hydrogen`, `-neon`, `gemini-3.8-flash`, `gemini-3.6-flash`, `gemini-3.1-pro-preview`, `gemini-3-flash-preview` | Gemini 4 Neon at Xhigh on SWE-Atlas-QnA-AA (67.7%); Gemini 3.1 Pro (13.50% ±3.90) and Gemini 3 Flash (8.20% ±3.30) on Scale's QnA board |
| Muse | `openrouter/meta/muse-spark-1.3`, `muse-wonder`, `muse-ember`, `muse-spark-open` | Muse Spark 1.3 and Muse Wonder rows on IronBee (6 rows) and on SWE-Atlas-QnA-AA (Muse Wonder Full, Muse Wonder, Muse Spark 1.3, Muse Ember) |
| Inkling | `openrouter/thinkingmachines/inkling` and two smaller ids | none on the pages read here |

DeepSeek V4 was the 2026-11-03 exception: the file kept V4 rows although pi
listed no V4 model. pi now lists `deepseek-v4-pro`, `deepseek-v4-flash` and
`deepseek-v4.1-flash`, so the V4 rows are in scope by the rule itself.

The pages read for this refresh also show rows for models a harness reaches
that no draft held. The file does not add them, because this refresh adds only
the 83 drafts. A later refresh could add them:

- IronBee's 50-PR table has 52 rows, and the file keeps 19. Of the other 33,
  30 are for models a harness here reaches. The 30 are Claude Opus 5 and
  Sonnet 5 at three levels each, the six Muse rows, Sonnet 5.5 at default and
  xhigh, Grok 4.7 at default and xhigh, Kimi K3 at default and low, Haiku 5.5
  at low and xhigh, MiniMax M3 at low and xhigh, GPT-6 Luna at default and
  low, GPT-5.6 Terra at three levels, and DeepSeek V4.1 Flash, DeepSeek V4
  Pro and GLM 5.3 at low. The other three are Qwen 3.8 Max, which pi does not
  list.
- SWE-Atlas-QnA-AA also has Mistral Large 4 at Medium (44.6%), Step 5 Preview
  in Claude Code (58.6%) and Beam 1.0 in Mirror (37.6%). pi lists both
  `openrouter/stepfun/step-5-preview` and
  `openrouter/reflection/beam-1.0-501b-a23b`.
- Qodo's table also has Claude Haiku 4.5 at high (30.4% F1) and Mistral
  Medium 3.5 at "medium (as sent)" (24.0%). Both are reachable.

## Sources

All pages were read on 2026-11-20.

### CodeReviewBench (Kodus), the board as it is today

<https://www.codereviewbench.com/leaderboard>, with the same numbers in its
`/api/leaderboard.json` (generated 2026-11-09) and on each model's page.

- Measures: 30 merged pull requests with 121 confirmed bugs (gold set
  `gold-v3`), judged by Claude Haiku 5.5. Every model runs in the Kodus
  harness in two modes. "Balanced" is one generalist agent. "Heavy" adds 9
  specialist agents. The board ranks by F2, which weighs recall twice as much
  as precision.
- Informs `judgement`.
- Limits: Kodus sells a review tool. The sample is small, and the 95%
  intervals on recall are 9 to 25 points wide. Each row is one run.

The 2026-11-03 note read a different board: 95 human-found bugs, a Claude
Haiku 4.5 judge and F1 ranking. Its four rows were DeepSeek V4 Pro, DeepSeek
V4.1 Flash, Kimi K3 and DeepSeek V4 Flash. The board today shows no Kimi K3
or DeepSeek V4 Flash row, so those two models have no CodeReviewBench entry
now.

| Old rank | Model | Old claim (F1, 95 bugs) | New rank | New claim (balanced mode, 121 bugs) |
| --- | --- | --- | --- | --- |
| 1 | DeepSeek V4 Pro | F1 43.9 | 8 | F2 34.5, F1 38.4, at `medium` |
| 2 | DeepSeek V4.1 Flash | F1 41.5 | 5 | F2 41.0, F1 41.9, at `medium` |
| 3 | Kimi K3 | F1 39.9 | - | not on the board |
| 4 | DeepSeek V4 Flash | F1 39.3 | - | not on the board |

The source keeps its old name, "CodeReviewBench leaderboard (Kodus)". The
draft named it "CodeReviewBench leaderboard (Kodus), gold-v3" so that its rows
would not mix with the old ones. The old rows are gone, so the old name
leaves no mix.

The board has 13 rows. The file keeps 12. It leaves out Muse Spark 1.2,
because pi lists Muse Spark 1.3 and not 1.2.

| Rank | Model | Reasoning, as the model page says | F2 balanced | F2 heavy | Run date |
| --- | --- | --- | --- | --- | --- |
| 1 | Kimi K3.1 Pro | vendor-default | 63.5 | 58.4 | 2026-11-07 |
| 2 | GPT-6.1 Sol | vendor-default | 45.9 | 55.4 | 2026-11-07 |
| 3 | Claude Opus 5.5 | vendor-default | 43.9 | 49.4 | 2026-11-09 |
| 4 | GLM-5.3 | vendor-default | 41.7 | 50.2 | 2026-11-08 |
| 5 | DeepSeek V4.1 Flash | explicit (medium) | 41.0 | 48.9 | 2026-11-07 |
| 6 | Claude Sonnet 5.5 | vendor-default | 40.0 | 48.8 | 2026-11-07 |
| 7 | GLM-5.3 Flash | vendor-default | 36.3 | 46.3 | 2026-11-08 |
| 8 | DeepSeek V4 Pro | explicit (medium) | 34.5 | 46.4 | 2026-11-07 |
| 9 | Qwen3.8 27B | vendor-default | 30.9 | 42.3 | 2026-11-09 |
| 10 | MiniMax M3 | vendor-default | 21.9 | 29.0 | 2026-11-08 |
| 11 | Gemini 3.8 Flash | vendor-default | 18.2 | 24.2 | 2026-11-07 |
| 12 | GPT-6 Luna | vendor-default | 8.1 | 15.1 | 2026-11-07 |

The ranks follow balanced mode, as the board's default view does. Each claim
also gives the heavy-mode F2. `published` is the balanced run's date.

### IronBee, "The Honest Raw LLM Code Review Benchmark"

<https://ironbee.ai/blog/the-honest-raw-llm-code-review-benchmark>,
published 2026-10-19.

- Measures: 21 models at up to three effort levels on the 50 pull requests
  of Martian's Code Review Bench, with 158 core human-checked findings. Each
  model gets one call per pull request, with no tools and no repository. Every
  review ran three times. Three judges from families other than the
  reviewer's matched the findings, and the score is F2.
- Informs `judgement`, three times over, as three sources:
  - The 50-PR table. The file keeps 19 of its 52 rows.
  - The 10-PR "frontier set", where Fable 5.1 and GPT-6 Astra also ran. The
    page gives three rows of it in its text: Sonnet 5.5 at low first (F2
    75.8), Fable 5.1 at its best level 5th of 58 (69.0), GPT-6 Astra at its
    best level 48th (42.9). The page does not name those best levels.
  - Each judge's agreement with the panel: ranking the configurations by one
    judge alone gives Kendall's tau 0.96 for Opus 5.5, 0.94 for Gemini 4
    Argon and GPT-6.1 Sol, and 0.93 for Grok 4.7. The judges ran at low
    effort. Grok 4.7 judged only when a judge shared the reviewed model's
    family.
- Limits: IronBee sells a review tool, and publishes its prompts, replies,
  verdicts and costs. The page says to read its groups, not its ranks, since
  a single pass moved a configuration by up to 7.5 points. The ranks in the
  file count only the kept rows, as the 2026-11-03 rule says. Models ran
  through Amazon Bedrock or OpenRouter, not through a coding harness.
- `effort` "default (no effort parameter)" is the page's "default": the model
  was called with no effort setting.

### Qodo, "Which LLM should review your code?"

<https://www.qodo.ai/blog/which-llm-should-review-your-code/>, published
2026-10-12.

- Measures: 125 pull requests with 703 known bugs, traced back from later
  fixes. One reviewer per configuration, judged by Claude Sonnet 4.6. F1 is
  the headline score.
- Informs `judgement`.
- Limits: the data set is private, so the file records the rows as
  `user_report`. Qodo sells a review tool. The page answers a plain request
  with HTTP 403 and opens with a browser's request headers. The post's text
  and table disagree on Sonnet 5.5. The table labels the 54.9% row `high`.
  The text calls 54.9% the `xhigh` score and gives 51.7% at `high`. The entry
  states both.
- The file keeps 10 of the table's 13 rows. It leaves out Claude Haiku 4.5 and
  Mistral Medium 3.5, which no draft held, and Laguna S 2.1, which no harness
  lists.

### Artificial Analysis Coding Agent Index, SWE-Atlas-QnA-AA

<https://artificialanalysis.ai/agents/coding-agents>. The page's embedded
data says it was materialized 2026-11-19, and the page shows no date per row.
So every entry carries `published` 2026-11-19.

- Measures: Scale's 124 Codebase QnA questions, three runs each (372
  attempts), with grading adjusted by Artificial Analysis. Each agent runs
  in its vendor's own harness: Claude Code, Codex, Antigravity CLI, Grok
  Build and others.
- Informs `read_only_survey`, the same way Scale's own QnA board does. It
  stays a separate source, because the grading differs.
- Each score is the page's mean reward times 100, rounded to one decimal. The
  page publishes no error bars.
- Limits: Gemini is measured only in Google's Antigravity CLI. The page's
  harness comparison section says "Coming soon". Every row of a Claude model
  in Claude Code except Haiku 5.5 is labelled "Default Fallback". The page explains that a fallback
  means another model finished the attempt. In those rows another model
  finished 33 to 52 of the 372 attempts, so up to 14% of a Claude score is
  another model's work. Codex and Antigravity rows instead show attempts that
  ended in a refusal, 6 to 25 of 372. Each claim gives its row's count.

### SWE Atlas Codebase QnA leaderboard (Scale)

<https://labs.scale.com/leaderboard/sweatlas-qna>. One new row is in scope:
Gemini 3.8 Flash in mini-SWE-agent, 47.04% ±5.08, created 2026-09-09. The
draft gave it no effort, and the board states none.

### Gemini API thinking guide

<https://ai.google.dev/gemini-api/docs/thinking>. Undated, so the entries
carry 2026-11-20.

- Says: `gemini-4-carbon` and `gemini-4-argon` default to `medium`, support
  `low`, `medium` and `high`, and reach their "Maximum reasoning depth" at
  `high`. `gemini-4-neon` and `gemini-4-hydrogen` default to `high` and also
  support `xhigh`.
- Informs `judgement`, as guidance on the effort for a Gemini judging seat,
  next to the OpenAI guide's advice on `xhigh` for code review.
- Limits: guidance, not a measurement. It is the only source that says what a
  Gemini call without an effort setting runs at. IronBee measured that case:
  Argon with no effort setting scored 6 points of F2 below Argon at `high`.

### OpenAI reasoning models guide

<https://developers.openai.com/api/docs/guides/reasoning>. Undated, so the
entry carries 2026-11-20.

- Says: `xhigh` lists "security and code review" among its uses, "Only use
  when your evals show a clear benefit". GPT-6.2 Astra and GPT-6.2 Sol reject
  an effort of `none`. The guide now says "Start with gpt-6.2-astra for most
  reasoning workloads".
- The new entry is for `gpt-6.2-astra` at `xhigh`. It shares rank 1 with the
  existing `gpt-6.1-sol` entry of the same advice.

## Drafts changed before they went in

| Draft | Change | Why |
| --- | --- | --- |
| All 12 CodeReviewBench rows | Source name "CodeReviewBench leaderboard (Kodus)" instead of "..., gold-v3" | The old rows are removed, so the old name holds only the new board |
| CodeReviewBench, Qwen3.8 27B | The claim adds that AkashML served the model in fp8, 8-bit quantized weights | The board marks the row so. A quantized model can score below the full one |
| CodeReviewBench, Gemini 3.8 Flash | The claim adds that $0.348 is a promotional price valid until 2026-12-31 | The board marks the row so |
| IronBee judge agreement, Grok 4.7 | The claim calls Grok the stand-in judge, not one of three judges | The page says Grok judged only when a judge shared the reviewed model's family |
| All 31 SWE-Atlas-QnA-AA rows | Claude Code claims give the attempts a fallback model finished; others give the attempts that ended in a refusal, where there were any | Read from the page's data; a reader should know when a score includes another model's work |
| SWE-Atlas-QnA-AA, Kimi K3.1 Pro | `effort` is empty instead of "not stated" | The 2026-11-03 rule leaves `effort` empty when the source does not say |
| Gemini thinking guide, OpenAI reasoning guide | `published` 2026-11-20 instead of 2026-11-19 | An undated page carries the day it was read, and this refresh read it on 2026-11-20 |

Every draft's ranks were recomputed from the scores. They came out as the
drafts gave them.

## Groups re-ranked

### SWE Atlas Codebase QnA leaderboard (Scale), `read_only_survey`

The Gemini 3.8 Flash row scores 47.04%, above GPT-5.6 Sol's 46.00%. So it
takes rank 5, and the 12 rows ranked 5 to 16 each move down by one.

| Model | Score | Old rank | New rank |
| --- | --- | --- | --- |
| Opus 5 | 63.17% | 1 | 1 |
| Fable 5.1 | 59.95% | 2 | 2 |
| GPT-6 Astra | 59.14% | 3 | 3 |
| Opus 4.8 | 57.26% | 4 | 4 |
| Gemini 3.8 Flash | 47.04% | - (new) | 5 |
| GPT-5.6 Sol | 46.00% | 5 | 6 |
| GPT-5.5 | 45.43% | 6 | 7 |
| GPT-5.4 | 40.80% | 7 | 8 |
| Opus 4.7 | 40.32% | 8 | 9 |
| Fable 5 | 39.00% | 9 | 10 |
| Opus 4.6 | 33.30% | 10 | 11 |
| GPT-5.3 Codex | 32.60% | 11 | 12 |
| Sonnet 4.6 | 31.20% | 12 | 13 |
| DeepSeek V4 Pro | 27.15% | 13 | 14 |
| GLM-5 | 20.50% | 14 | 15 |
| Kimi K2.5 | 13.10% | 15 | 16 |
| MiniMax M2.5 | 10.30% | 16 | 17 |

The same source's `root_cause` group keeps its four rows and ranks. Those
rows stop at Opus 4.8.

### OpenAI reasoning models guide, `judgement`

`gpt-6.1-sol` and `gpt-6.2-astra` share rank 1, because the guide gives the
same advice for both.

No other existing group changed. Every other new entry belongs to a new
source.

## What a lead sees for Gemini now

`teamwire models --harness pi --for judgement` lists six Gemini 4 entries:
three IronBee rows for Gemini 4 Argon (`high`, `low`, no effort setting), the
IronBee judge-agreement row for Argon, and the thinking guide's entries for
Carbon and Argon. The only measured Gemini 4 results for judging are
IronBee's, and IronBee measured Argon only. No source measures Carbon,
Hydrogen or Neon at code review or judging.

For `read_only_survey`, SWE-Atlas-QnA-AA measures Carbon, Hydrogen and Argon,
in Antigravity CLI only.

## Gaps

- **Gemini outside Google's harness.** IronBee's raw API calls are the only
  measured Gemini 4 result outside Antigravity CLI. No source runs Gemini 4 in
  pi or Claude Code on a judging task.
- **Code review for the newest models.** No source found scores Fable 5.2,
  GPT-6.2 Astra, GPT-6.2 Sol, Gemini 4 Carbon, Hydrogen or Neon at code
  review.
- **Judge benchmarks.** No JudgeBench-style result exists for any Claude 5.x,
  GPT-6.x or Gemini 4 model. IronBee's judge agreement is the nearest.
- **Planning.** No source measures any model at writing or reviewing an
  implementation plan. The file has no class for it.
- **Rows left for a later refresh.** "Which models are in scope now" lists
  the reachable rows that no draft held.
