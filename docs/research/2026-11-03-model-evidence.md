# Research: public evidence on which model suits which task class

Written 2026-11-03. Every page below was opened and read on 2026-11-03, and
every number in `examples/models/evidence.json` was read off that page on that
day. The file is a dated snapshot. Leaderboards change monthly, so a reader
should treat `as_of` as the age of every claim in it.

## What the file holds

`examples/models/evidence.json` has the `Evidence` shape from
`crates/teamwire-core/src/models.rs`. It holds 180 entries from 28 named
sources on 21 pages. `teamwire models --for <class>` shows these entries to a
lead. The command never picks a model. The lead picks.

Each entry is one claim from one source about one model on one task class.
Four fields need a note:

| Field | How the file fills it |
| --- | --- |
| `rank` | Ranks compare entries of one source in one class only. Rank 1 is the best that source found among the entries kept here. Entries with the same score share a rank, and the next rank follows with no gap. A source's own position for a model can differ, because the file drops models a lead cannot reach (see "Models covered"). |
| `published` | A leaderboard row carries the date the board shows for that row. A paper or report carries its publication date. A vendor docs page shows no date, so its entries carry 2026-11-03, the day it was read. |
| `effort` | The effort the source states, in the source's own words, such as `xhigh`, `32k thinking tokens` or `reasoner (thinking mode)`. The field is empty when the source does not say. |
| `kind` | `benchmark` for a public leaderboard or a paper with a fixed task set. `vendor` for a vendor's docs, and for benchmark numbers a vendor ran and reported about its own models. `user_report` for a measured result from a user. |

One source can inform more than one class. Such a source appears once per
class, each with its own ranks.

## Models covered

The file covers the models a lead reaches through Claude Code, Codex and pi:

- Anthropic: Claude Fable 5.1, Opus 5.5, Sonnet 5.5 and Haiku 5.5, the 5.0
  models (Fable 5, Opus 5, Sonnet 5), and the 4.x models that sources still
  report (Opus 4.8, 4.7, 4.6, 4.5, 4.1, 4; Sonnet 4.6, 4.5, 4; Haiku 4.5).
- OpenAI: GPT-6 Astra, GPT-6.1 Sol, GPT-6 Sol, GPT-6.1 Terra, GPT-6 Luna, the
  GPT-5.x models, and the Codex variants (`gpt-5.3-codex`, `gpt-5.2-codex`).
- Open models on Amazon Bedrock: DeepSeek and Amazon Nova. The Bedrock
  DeepSeek page lists DeepSeek V3.2, V3.1 and R1
  (<https://docs.aws.amazon.com/bedrock/latest/userguide/model-cards-deepseek.html>).
  The file also keeps DeepSeek V4 rows, because pi can reach DeepSeek through
  other providers. Whether V4 is on Bedrock was not confirmed.

The file leaves out Gemini, Muse, Kimi, GLM, Qwen, MiniMax, Grok and other
models, even where a source ranks them above a covered model. A lead on these
three harnesses cannot start a member on them. The claim text gives the
number, so a reader can still compare across sources.

Model names follow each vendor's API id where the vendor publishes one, such
as `claude-opus-5-5` and `gpt-6.1-sol`. The 4.5 Claude models use their short
aliases (`claude-opus-4-5`). The two Claude 4 models from May 2025 keep their
dated ids. Amazon's Nova 2 models are named `nova-2-pro` and `nova-2-lite`, as
the Nova 2 report names them, because no Bedrock id for them was confirmed.

## Sources

All pages were read on 2026-11-03.

### Public leaderboards

**Aider polyglot leaderboard.** <https://aider.chat/docs/leaderboards/>.
The page says it was last updated 2025-11-20. Rows are dated 2025-05-24 to
2025-10-03.
- Measures: 225 Exercism exercises in six languages. Each row gives the share
  solved and the share of responses in the correct edit format.
- Informs `mechanical_edit`. The edit-format column measures whether a model
  can express a change as a well-formed edit, which is the core of a
  mechanical change.
- Limits: the exercises are small and public, so contamination is likely. The
  newest covered models are GPT-5 and DeepSeek V3.2-Exp. No Claude 5.x or
  GPT-6 model is on it. The harness is aider itself.

**Terminal-Bench 4.0 leaderboard.** <https://www.tbench.ai/> (the
`/leaderboard` path redirects there). Rows dated 2026-05-28 to 2026-09-29.
- Measures: multi-step command-line tasks, 330 trials per row, with a 95%
  interval of about ±3 points and the total cost of each run.
- Informs `contained_feature`. Each task is one self-contained job in a
  container, checked by tests.
- Limits: each vendor ran in its own harness (Claude Code, Codex), so the
  harness differs between vendors. The tasks are not all code changes. It
  carries effort sweeps for Fable 5.1, Opus 5 and GPT-6 Astra, which makes it
  the best public source on effort. Haiku 5.5 and Opus 5.5 below max are not
  on it.

**SWE-Bench Pro V2 HARD and Full (Scale).**
<https://labs.scale.com/leaderboard/swe_bench_pro_public_v2>. Released
2026-09-22. Rows dated 2026-09-22 to 2026-11-02.
- Measures: 642 issue-fix tasks from copyleft repositories, with web tools
  turned off and every diff re-graded on a clean image. HARD is a harder
  subset, and the page shows it by default.
- Informs `contained_feature`. These are single-issue fixes checked by tests.
- Limits: the Full set is near its ceiling (87% to 99%), so it separates
  models poorly. HARD separates them better. Rows mix Claude Code, Codex and
  mini-swe-agent. Sonnet 5.5 and Haiku 5.5 ran in mini-swe-agent, not Claude
  Code, so their rows understate them against Claude Code rows. Scale's
  re-grade caught Opus 5 forging a Go module checksum. Opus 5.5 is not on it.

**SWE-Bench Pro public leaderboard, v1 (Scale).**
<https://labs.scale.com/leaderboard/swe_bench_pro>. Rows dated 2025-09-19 to
2026-07-09.
- Measures: 731 issue-fix tasks from copyleft repositories, with a 95%
  interval of about ±3.6 points.
- Informs `contained_feature`.
- Limits: superseded by V2. Starred rows ran in mini-swe-agent and the others
  in the SWE-Agent scaffold. The newest covered rows are GPT-5.4 and Opus 4.6.

**SWE-bench Verified, bash-only view.** <https://www.swebench.com/>. The
covered rows are dated 2026-02-17 to 2026-02-19.
- Measures: 500 human-checked Python issues from 12 repositories. The file
  keeps only rows run in mini-SWE-agent 2.0.0, so every row used the same
  harness.
- Informs `contained_feature`.
- Limits: the issues are public and old, so contamination is likely. The
  newest rows are from February 2026, with no Claude 5.x or GPT-6 model. Each
  row is one attempt.

**SWE Atlas Refactoring (Scale).**
<https://labs.scale.com/leaderboard/sweatlas-refactoring>. Rows dated
2026-05-06 to 2026-09-09.
- Measures: 70 refactoring tasks in 10 repositories. A task passes when every
  existing test still passes and an LLM judge (Claude Opus 4.5) finds every
  must-have rubric item met. Scale says the tasks change about 2 times the
  lines and 1.7 times the files of SWE-Bench Pro.
- Informs `cross_cutting`. Twenty of the 70 tasks change a public interface
  and every call site. The rest extract, split or move code across files.
- Limits: the interval is wide, about ±6.7 points. The page's summary text
  still names Opus 4.7 as the leader, while the table shows GPT-6 Astra on
  top. Opus 5, Opus 5.5, Sonnet 5.5 and Haiku 5.5 are not on it.

**SWE Atlas Codebase QnA (Scale).**
<https://labs.scale.com/leaderboard/sweatlas-qna>. Rows dated 2026-02-25 to
2026-09-09.
- Measures: 124 questions on 11 repositories. The agent must run and trace
  the code, writes nothing, and an LLM judge (Claude Opus 4.5) grades the
  answer against an expert rubric. A task counts only when every rubric item
  is met.
- Informs `read_only_survey` directly. It also informs `root_cause`, because
  30% of the questions ask why something behaves the way it does. The board
  gives no score per category, so the `root_cause` rows repeat the overall
  score and say so.
- Limits: interval about ±5 points. Three rows (Fable 5.1, Fable 5, GPT-5.6
  Sol) carry a record date of 2026-03-02, before those models were released.
  The file dates those rows 2026-09-09, the newest date on the board. Opus 5.5,
  Sonnet 5.5 and Haiku 5.5 are not on it.

**CodeReviewBench (Kodus).** <https://www.codereviewbench.com/leaderboard>.
Modified 2026-09-11.
- Measures: 30 merged pull requests with 95 human-found bugs. A model's review
  is scored by F1 against those bugs, judged by Claude Haiku 4.5.
- Informs `judgement`.
- Limits: a small sample, with 95% intervals about 24 points wide. It has
  measured no Claude or GPT model yet, only open models. The file keeps the
  three DeepSeek rows.

### Papers

**Multi-file change localization study.** arXiv 2606.11976,
<https://arxiv.org/abs/2606.11976>, 2026-06-10.
- Measures: whether an agent finds the files a fix must change, on the
  SWE-Bench Pro ansible tasks and on newer 2025 to 2026 pull requests.
- Informs `root_cause`. Finding the files to change is the localisation step
  of finding why something fails.
- Limits: 19 instances and one repository. The paper compares harnesses more
  than models. Its "Codex 5.5 High" is the Codex CLI. The file records it as
  `gpt-5.5` at `high`, which is an inference from that label.

**CONCUR.** arXiv 2603.03683, <https://arxiv.org/abs/2603.03683>,
2026-03-04.
- Measures: 115 concurrent Java problems. The checker, Java PathFinder,
  explores thread schedules for deadlocks and races.
- Informs `concurrency_data_loss`. It is the only measurement found on
  concurrency bugs.
- Limits: it measures writing new concurrent code, not finding a race in
  existing code. The newest covered models are GPT-5 and Claude Opus 4.1.

**Reliability without Validity.** arXiv 2606.19544,
<https://arxiv.org/abs/2606.19544>, 2026-06-17.
- Measures: 21 LLM judges on MT-Bench, JudgeBench and RewardBench. The file
  uses JudgeBench, whose 350 items include coding and carry objective
  correctness labels. It reports Cohen's kappa and position bias.
- Informs `judgement`. It measures how often a model's verdict on another
  model's answer is correct.
- Limits: JudgeBench is not code review. The authors show that judge rankings
  move by up to 14 places between benchmarks. The newest covered models are
  Opus 4.6, Sonnet 4.6 and GPT-5.4.

### Vendor pages

**Anthropic: Optimizing for cost and intelligence.**
<https://platform.claude.com/docs/en/about-claude/models/optimizing-for-cost-and-intelligence>.
Undated.
- Measures: Anthropic's own runs on a subset of SWE-bench Pro across effort
  levels for Opus 5.5, Sonnet 5.5 and Fable 5.1, and a defect-search task
  larger than one context window.
- Informs `contained_feature` (the SWE-bench Pro subset) and
  `read_only_survey` (the defect search, which reads a large corpus).
- Limits: internal and run by the vendor. Anthropic says the subset's scores
  are not comparable to the public board, and that Opus 5.5 and Fable 5.1
  "largely saturate" it. Several rows are a single run.

**Anthropic effort documentation.**
<https://platform.claude.com/docs/en/build-with-claude/effort>. Undated.
- Says: the effort levels each model takes, each model's default (`medium` on
  Opus 5.5 and Haiku 5.5, `high` on the others), and where to start for each
  model.
- Informs `mechanical_edit`, `contained_feature` and `read_only_survey`.
- Limits: guidance, not a measurement.

**Anthropic: Choosing a model.**
<https://platform.claude.com/docs/en/about-claude/models/choosing-a-model>.
Undated.
- Says: start most work on Opus 5.5, and move to Fable 5.1 when Opus 5.5 at
  `xhigh` or `max` still falls short on demanding reasoning. It names Opus 5.5
  for large-scale refactoring.
- Informs `cross_cutting` and `concurrency_data_loss`.
- Limits: guidance. The `concurrency_data_loss` rows map "demanding
  reasoning" onto concurrency. That mapping is this note's inference.

**Anthropic model announcements.** Opus 5.5
(<https://www.anthropic.com/claude-opus-5-5>, 2026-09-22), Sonnet 5.5
(<https://www.anthropic.com/claude-sonnet-5-5>, 2026-09-28), Haiku 5.5
(<https://www.anthropic.com/claude-haiku-5-5>, 2026-10-07) and Fable 5.1
(<https://www.anthropic.com/claude-fable-and-mythos-5-1>, released
2026-09-01 according to the Fable 5.1 docs page).
- Measures: vendor-run scores on FrontierCode (whether a change would be
  merged), Terminal-Bench 4.0, CursorBench (ambiguous multi-file tasks from
  real Cursor sessions), and an internal HAProxy C-to-Rust migration. The
  Opus 5.5 page also quotes Deloitte's code-review measurement.
- Informs `contained_feature` (FrontierCode, Terminal-Bench),
  `cross_cutting` (CursorBench), `mechanical_edit` (the migration) and
  `judgement` (Deloitte).
- Limits: run and chosen by the vendor. Competitor numbers are "as reported
  by OpenAI" or Zapier. Each benchmark on each page is its own source in the
  file, so ranks never mix two benchmarks. The Deloitte number is a customer's
  report with no sample size, recorded as `user_report`.

**Amazon Nova 2 technical report.**
<https://cdn.amazon.science/c5/3d/84514a224666b5be6de4b43ef4aa/nova-2-0-technical-report2.pdf>,
2025-12-15 (the PDF's creation date).
- Measures: SWE-bench Verified for Nova 2 Pro, Nova 2 Lite and Nova 1 Premier
  next to Claude 4.5 and GPT-5 models.
- Informs `contained_feature`. It is the only source with Nova scores.
- Limits: vendor-run. Nova 2's higher scores (70.0% and 64.5%) used
  inference-time scaling in Amazon's own scaffold. The other models' numbers
  come from their providers.

**OpenAI Codex models guide.** <https://learn.chatgpt.com/docs/models> (the
`developers.openai.com/codex/models` path redirects there). Undated.
- Says: Astra for the hardest work with "stronger judgment", 6.1 Sol for
  near-Astra work at lower cost, Luna for clear, repeatable, high-volume
  tasks. Start Luna at High and Astra at Light (`low`), and raise effort for
  work that needs more checking.
- Informs `mechanical_edit` and `judgement`.
- Limits: guidance. Its effort advice for Low in the CLI is general. The file
  attaches it to 6.1 Sol, the model the guide recommends for Codex.

**OpenAI reasoning models guide.**
<https://developers.openai.com/api/docs/guides/reasoning>. Undated.
- Says: `medium` is the default and the balanced point for agentic coding.
  `high` is for hard reasoning and complex debugging. `xhigh` is for security
  and code review, used only when evals show a clear benefit. `max` is for
  the most complex tasks.
- Informs `contained_feature`, `root_cause`, `concurrency_data_loss` and
  `judgement`.
- Limits: guidance for all its reasoning models. The file attaches it to
  `gpt-6.1-sol`, which defaults to `medium`.

## How each class was mapped

| Class | Sources in the file | Why they fit |
| --- | --- | --- |
| `mechanical_edit` | Aider polyglot; HAProxy migration; Anthropic effort docs; OpenAI Codex guide | Aider measures well-formed edits. The migration is a migration case study. The guides say which model and effort to use for routine, well-specified work. |
| `contained_feature` | Terminal-Bench 4.0; SWE-Bench Pro V2 HARD and Full; SWE-Bench Pro v1; SWE-bench Verified; Nova 2 report; Anthropic's SWE-bench Pro subset; FrontierCode and Terminal-Bench in the 5.5 announcements; Anthropic effort docs; OpenAI reasoning guide | Each task is one issue or job, checked by its tests. |
| `cross_cutting` | SWE Atlas Refactoring; CursorBench 4.0 and 3.2.0 in the announcements; Anthropic: Choosing a model | Refactoring changes interfaces and their call sites across files. CursorBench is multi-file work from real sessions. |
| `root_cause` | Localization study; SWE Atlas Codebase QnA; OpenAI reasoning guide | Localisation finds where a fix belongs. 30% of QnA asks why code behaves as it does. |
| `concurrency_data_loss` | CONCUR; Anthropic: Choosing a model; OpenAI reasoning guide | CONCUR checks generated code for deadlocks and races. No benchmark measures finding races or data loss in existing code. |
| `read_only_survey` | SWE Atlas Codebase QnA; Anthropic's defect-search measurement; Anthropic effort docs | QnA asks the agent to map how code works and forbids edits. The defect search reads a corpus larger than one context window. |
| `judgement` | JudgeBench study; CodeReviewBench; Deloitte via the Opus 5.5 page; OpenAI reasoning guide; OpenAI Codex guide | These measure or advise on judging other work: review findings and verdicts on answers. |

## What the sources say about effort

- **Effort moves scores as much as the model does.** On Terminal-Bench 4.0,
  Fable 5.1 scored 43.33% at `low` and 54.55% at `high`. GPT-6 Astra scored
  50.61% at `low` and 57.88% at `high`.
- **The top level often buys nothing.** GPT-6 Astra scored 57.88% at `high`,
  `xhigh` and `max` on Terminal-Bench, with cost rising from $2,269 to $3,267.
  Opus 5 scored lower at `max` (51.82%) than at `xhigh` (53.94%). Sonnet 5.5
  scored lower at `max` than at `xhigh` on FrontierCode, because it started
  extra review rounds that went out of scope.
- **A cheaper model at the same effort can match a dearer one.** GPT-6.1 Sol
  at `max` matched GPT-6 Astra at `max` on Terminal-Bench (58.18%) for $634
  against $3,267.
- **Running low and re-running failures high is cheapest where tests check
  the work.** On Anthropic's SWE-bench Pro subset, Opus 5.5 at `low` with
  failures re-run at `high` solved about 97% at about $0.17 a task. Opus 5.5
  at `high` for everything solved 95.3% at $0.29.
- **Defaults differ by model.** Opus 5.5 and Haiku 5.5 default to `medium`.
  Fable 5.1 and Sonnet 5.5 default to `high` on the API. Claude Code and the
  Claude apps run Sonnet 5.5 at `medium`. GPT-6.1 Sol, GPT-6 Sol and GPT-6
  Luna default to `medium`. GPT-6 Astra and 6.1 Sol do not accept `none`.
- **Low effort skips checks.** Anthropic warns that Haiku 5.5 at `low` is more
  likely to skip a search, stop early or skip a check in long agent prompts.

## Where sources disagree

The file keeps both sides of each disagreement.

| Claim | One source | The other source |
| --- | --- | --- |
| Sonnet 5.5 on Terminal-Bench 4.0 | 70.6% in Anthropic's announcement, effort not stated | 61.82% ±2.94 at `max` on the public leaderboard |
| Opus 5.5 on Terminal-Bench 4.0 | 66.4% at `xhigh` in Anthropic's announcements | 64.85% ±3.11 at `max` on the public leaderboard, within the two intervals |
| Sonnet 4.5 on SWE-bench Verified | 77.2% in Amazon's Nova 2 report, and 82.0% with the provider's test-time scaling | 71.4% at `high` in the mini-SWE-agent view on swebench.com |
| Haiku 4.5 on SWE-bench Verified | 73.3% in the Nova 2 report | 66.6% at `high` on swebench.com |
| Haiku 4.5 on Terminal-Bench | 0.0% on Terminal-Bench 4.0 in Anthropic's Haiku 5.5 announcement | 41.8% on Terminal-Bench 1.0 in the Nova 2 report, an older and easier version |

The SWE-bench Verified gaps most likely come from the harness. The Nova 2
report does not say which scaffold produced each competitor's number. The
swebench.com view runs every model in one small scaffold, mini-SWE-agent.

## Pages opened and not used

- The openai.com news posts (GPT-6 Astra, GPT-6 Sol and Luna, GPT-6.1 Sol,
  GPT-6.1 Terra) and OpenAI's post on why it stopped reporting SWE-bench
  Verified. Each returned HTTP 403 to a plain request, so no number from them
  was read, and the file cites none.
- Martian's Code Review Bench (<https://codereview.withmartian.com/>, README
  at <https://github.com/withmartian/code-review-benchmark>). The benchmark
  ranks review products such as CodeRabbit and Claude Code, not models, so it
  cannot rank models.
- GitHub's ReviewBench post
  (<https://github.blog/ai-and-ml/github-copilot/reviewbench-an-open-benchmark-for-ai-code-review/>,
  2026-10-05). The post describes the benchmark and gives no per-model
  scores. The leaderboard page renders in the browser and was not read.
- Context Arena (<https://contextarena.ai/>) and Microsoft's debug-gym
  ProgramDistill board. Both render their tables in the browser, so no number
  was read.
- Aggregator sites such as benchlm.ai and llm-stats.com. They copy numbers
  from other pages, so the file cites the pages they copy from instead.
- A 2026 OpenReview study of data-race detection. The page returned a
  challenge page instead of the paper.

## Gaps

- **Race and data-loss review.** No benchmark found measures whether a model
  finds a race, a crash-safety bug or a data-loss path in existing code.
  CONCUR measures writing new concurrent code, and only for GPT-5 and Opus
  4.1. The other `concurrency_data_loss` rows are vendor guidance on hard
  reasoning, not measurements. A lead should weight the user's own team
  outcomes for this class above anything in the file.
- **Code review for current models.** No public benchmark scores Opus 5.5,
  Sonnet 5.5, Haiku 5.5, Fable 5.1 or any GPT-6 model on code review. The
  JudgeBench study stops at Opus 4.6 and GPT-5.4. CodeReviewBench covers only
  open models. The one current-model number is Deloitte's, with no sample
  size.
- **Root cause for current models.** The only direct measurement, the
  localisation study, has 19 instances and stops at GPT-5.5 and Sonnet 4.6.
  The QnA rows count all questions, not only the root-cause ones.
- **Long-context retrieval over code.** No long-context leaderboard could be
  read. `read_only_survey` rests on SWE Atlas Codebase QnA alone among
  benchmarks.
- **Mechanical edits for current models.** Aider's leaderboard stops at
  GPT-5 and Claude 4. No public benchmark of rename, move or
  dependency-bump work covers Claude 5.x or GPT-6.
- **Missing models on the newest boards.** Opus 5.5 is not on SWE-Bench Pro
  V2, SWE Atlas Refactoring or SWE Atlas QnA. Haiku 5.5 is not on
  Terminal-Bench 4.0. GPT-6.1 Terra appears on no board found.
- **Amazon Nova.** Nova appears only in Amazon's own report on SWE-bench
  Verified. No public leaderboard covers Nova 2, and no source covers Nova on
  any class but `contained_feature`.
- **DeepSeek on Bedrock.** Bedrock lists DeepSeek V3.2, V3.1 and R1. The
  newest boards report DeepSeek V4, so the evidence for the version pi reaches
  on Bedrock is older (SWE-bench Verified, SWE-Bench Pro v1, Aider, JudgeBench).
