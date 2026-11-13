# Research: refresh of the Terminal-Bench rows and three Fable 5.1 entries

Written 2026-11-14. Every page below was opened and read on 2026-11-14. This
note adds to `docs/research/2026-11-03-model-evidence.md` and
`docs/research/2026-11-13-fable-5-2-evidence.md`. The 2026-11-03 note still
explains how the evidence file fills `rank`, `published`, `effort` and `kind`,
and this note follows the same rules. Where those two notes describe an entry
this note changes, this note is the current account.

## What changed

`examples/models/evidence.json` now holds 247 entries, 19 fewer than the 266
it held on 2026-11-13.

| Change | Entries |
| --- | --- |
| The source "Terminal-Bench 4.0 leaderboard" is gone | 29 removed |
| New rows in "Terminal-Bench 4.1 leaderboard" for (model, effort) pairs that only the 4.0 group held | 12 added |
| Fable 5.1 entries on the choosing-a-model page | 2 removed |
| The Fable 5.1 defect-search entry | 1 reworded |

The file's `as_of` moved from 2026-11-13 to 2026-11-14. Only the entries named
in this note were read again on 2026-11-14. Every other entry keeps the
reading date its own note gives, 2026-11-03 or 2026-11-13. So `as_of` is the
age of the newest claims only.

No AFK table or preset changed.

## Sources

All pages were read on 2026-11-14.

### Terminal-Bench 4.1 leaderboard

<https://www.tbench.ai/>. The table is in the page's embedded data. The
page's only leaderboard is titled "Terminal-Bench 4.1", and its data says the
board was last updated 2026-11-13. The page shows no Terminal-Bench 4.0 table.

- The 22 rows the file took on 2026-11-13 read the same today: the same
  score, margin, cost and row date for each.
- Every one of the 29 (model, effort) pairs of the old 4.0 group still has a
  row on the board. So no pair was removed for being off the board.
- 17 of the 29 pairs already had a 4.1 row in the file. Each of them keeps
  that one 4.1 entry, unchanged, and loses its 4.0 entry.
- The other 12 pairs had only a 4.0 entry. Each now has a 4.1 entry with the
  board's current numbers, read off the board on 2026-11-14. Every row has 330
  trials, and `published` is the row's date on the board.

| Model | Effort | 4.0 score in the file | 4.1 score today | 95% margin | Run cost | Harness | Row date |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Fable 5.1 | `max` | 57.88% | 57.27% | ±3.83 | $6,243.50 | Claude Code | 2026-09-01 |
| Fable 5.1 | `high` | 54.55% | 54.85% | ±3.41 | $3,985.38 | Claude Code | 2026-09-01 |
| Fable 5.1 | `medium` | 53.94% | 53.94% | ±3.39 | $2,832.90 | Claude Code | 2026-09-01 |
| Fable 5.1 | `low` | 43.33% | 43.33% | ±3.61 | $2,358.72 | Claude Code | 2026-09-01 |
| GPT-6 Astra | `high` | 57.88% | 60.00% | ±2.79 | $2,269.42 | Codex | 2026-09-03 |
| GPT-6 Astra | `xhigh` | 57.88% | 59.70% | ±2.69 | $2,350.51 | Codex | 2026-09-03 |
| GPT-6 Astra | `medium` | 54.24% | 55.15% | ±2.66 | $1,914.80 | Codex | 2026-09-03 |
| GPT-6 Astra | `low` | 50.61% | 50.91% | ±2.79 | $1,557.30 | Codex | 2026-09-03 |
| Opus 5 | `max` | 51.82% | 52.12% | ±3.39 | $5,969.11 | Claude Code | 2026-07-24 |
| Opus 5 | `high` | 50.30% | 49.70% | ±3.69 | $4,662.27 | Claude Code | 2026-07-24 |
| Opus 5 | `medium` | 44.85% | 45.15% | ±3.85 | $3,191.59 | Claude Code | 2026-07-24 |
| Opus 5 | `low` | 34.85% | 34.85% | ±3.92 | $2,393.88 | Claude Code | 2026-07-24 |

Most margins changed too. The cost of each run is the same as in the 4.0
entry, which fits the board re-scoring the same runs rather than running them
again. The board does not say which it did.

The 17 pairs that keep their existing 4.1 entry, with the 4.0 score each
entry had and the 4.1 score it keeps:

| Model | Effort | 4.0 score | 4.1 score |
| --- | --- | --- | --- |
| Opus 5.5 | `max` | 64.85% | 65.76% |
| Sonnet 5.5 | `max` | 61.82% | 63.94% |
| GPT-6.1 Sol | `max` | 58.18% | 60.91% |
| GPT-6 Astra | `max` | 58.18% | 60.30% |
| Fable 5.1 | `xhigh` | 57.88% | 57.58% |
| Opus 5 | `xhigh` | 53.94% | 53.33% |
| GPT-6 Sol | `max` | 49.39% | 50.30% |
| Fable 5 | `max` | 44.55% | 44.85% |
| GLM-5.3 | `max` | 41.82% | 41.21% |
| Grok 4.7 | `xhigh` | 37.58% | 38.79% |
| GPT-5.6 Sol | `max` | 37.27% | 38.48% |
| Opus 4.8 | `max` | 23.64% | 23.64% |
| GPT-5.6 Terra | `max` | 21.52% | 21.52% |
| Grok 4.6 | `high` | 20.30% | 19.70% |
| GPT-5.6 Luna | `max` | 17.27% | 17.58% |
| GPT-6 Luna | `max` | 16.36% | 15.15% |
| Sonnet 5 | `max` | 12.42% | 12.12% |

The other five rows of the 4.1 group (Fable 5.2, GPT-6.1 Terra, Haiku 5.5,
Kimi K3 and GPT-5.4 Mini) had no 4.0 entry and are unchanged.

The board's other rows stay out of the file for the reasons the 2026-11-13
note gives: models no harness lists here, versions pi does not list, and
DeepSeek V4. That includes Kimi K3.1 Pro at 59.70%, which pi does not list.

Each new claim ends with a comparison only where the board's numbers support
it today. The old 4.0 claim for GPT-6 Astra at `high` said "the same score as
xhigh and max for less". That is no longer true, so the new claim says it is
0.30 points below `max` for about 69% of its cost. The old claims that Opus 5
at `max` scored below Opus 5 at `xhigh`, and that Fable 5.1 at `low` scored
about 11 points below `high`, still hold. The new claims keep them, the second
as "about 11.5 points".

### Anthropic: Choosing a model

<https://platform.claude.com/docs/en/about-claude/models/choosing-a-model>.
Undated.

- Says: "If your evals at xhigh or max effort still fall short on demanding
  reasoning or long-horizon agentic work, move to Claude Fable 5.2."
- The model matrix names Fable 5.2 for "The highest available capability".
- The page names Fable 5.1 once, in the list of models whose default effort
  is `high`. It no longer names Fable 5.1 as the model to move to.
- The file already has a Fable 5.2 entry with this advice in `cross_cutting`
  and in `concurrency_data_loss`. So both Fable 5.1 entries were removed,
  under the lead's rule for a page that now names only Fable 5.2.

### Anthropic: Optimizing for cost and intelligence

<https://platform.claude.com/docs/en/about-claude/models/optimizing-for-cost-and-intelligence>.
Undated.

The defect-search figures come from the page's reference for the internal
defect-search benchmark. The corpus is about 22 million tokens of public
Python source in 201 parts, with 130 planted defects and 10 runs per
configuration. The results give the mean F1, its range and the mean cost per
run:

| Configuration | Mean F1 | Range | Cost per run |
| --- | --- | --- | --- |
| Fable 5.1 alone at `low` | 0.829 | 0.807 to 0.841 | $527 |
| Fable 5.1 alone at `medium` | 0.839 | 0.817 to 0.856 | $603 |
| Fable 5.1 alone at `high` | 0.847 | 0.829 to 0.861 | $716 |
| Fable 5.1 with Sonnet 5.5 subagents | 0.825 | 0.812 to 0.858 | $172 |

The page no longer gives the run hours (15.2, 17.5 and 19.9) or the "10 to 12
points lower" figure for delegation that the 2026-11-03 entry quoted. With
Sonnet 5.5 subagents, Fable 5.1 now scores 0.825, close to Fable 5.1 alone.
So the reworded entry drops both and gives the current figures.

The Fable 5.2 defect-search figures on the page match the three Fable 5.2
entries from 2026-11-13, so those entries are unchanged.

## The three Fable 5.1 entries

| Class | Old text | New text |
| --- | --- | --- |
| `cross_cutting` | Anthropic says to move to Fable 5.1 when evals on Opus 5.5 at xhigh or max still fall short on long-horizon agentic work. | Removed. The Fable 5.2 entry carries the advice. |
| `concurrency_data_loss` | No benchmark measures current models on races or data loss; Anthropic names Fable 5.1 for demanding reasoning when Opus 5.5 at xhigh or max still falls short. | Removed. The Fable 5.2 entry carries the advice. |
| `read_only_survey` | On Anthropic's defect-search benchmark larger than one context window, Fable 5.1 alone cost $468 to $552 a run and took 15.2, 17.5 and 19.9 hours at low, medium and high; delegating parts to cheaper workers cost about half and scored 10 to 12 points lower. | On Anthropic's defect-search benchmark larger than one context window (about 22 million tokens, 130 planted defects, 10 runs per setting), Fable 5.1 alone scored a mean F1 of 0.829 (0.807 to 0.841) at low for $527 a run, 0.839 (0.817 to 0.856) at medium for $603, and 0.847 (0.829 to 0.861) at high for $716; with Sonnet 5.5 subagents it scored 0.825 for $172. |

The reworded `read_only_survey` entry now has `published` 2026-11-14, the day
its undated page was read. Its rank stays 4, below the three Fable 5.2
entries.

## Groups re-ranked

### Terminal-Bench 4.1 leaderboard, `contained_feature`

The group now has 34 entries, ranked 1 to 32. Two pairs of entries share a
rank. "Old 4.1 rank" is the entry's rank in this group on 2026-11-13. "Old 4.0
rank" is its rank in the removed 4.0 group. A dash means the entry was not in
that group.

| New rank | Model | Effort | Score | Old 4.1 rank | Old 4.0 rank |
| --- | --- | --- | --- | --- | --- |
| 1 | Fable 5.2 | `max` | 65.76% | 1 | - |
| 1 | Opus 5.5 | `max` | 65.76% | 1 | 1 |
| 2 | Sonnet 5.5 | `max` | 63.94% | 2 | 2 |
| 3 | GPT-6.1 Sol | `max` | 60.91% | 3 | 3 |
| 4 | GPT-6 Astra | `max` | 60.30% | 4 | 3 |
| 5 | GPT-6 Astra | `high` | 60.00% | - (new) | 4 |
| 6 | GPT-6 Astra | `xhigh` | 59.70% | - (new) | 4 |
| 7 | Fable 5.1 | `xhigh` | 57.58% | 5 | 4 |
| 8 | Fable 5.1 | `max` | 57.27% | - (new) | 4 |
| 9 | GPT-6 Astra | `medium` | 55.15% | - (new) | 6 |
| 10 | Fable 5.1 | `high` | 54.85% | - (new) | 5 |
| 11 | Fable 5.1 | `medium` | 53.94% | - (new) | 7 |
| 12 | Opus 5 | `xhigh` | 53.33% | 6 | 7 |
| 12 | GPT-6.1 Terra | `max` | 53.33% | 6 | - |
| 13 | Opus 5 | `max` | 52.12% | - (new) | 8 |
| 14 | GPT-6 Astra | `low` | 50.91% | - (new) | 9 |
| 15 | GPT-6 Sol | `max` | 50.30% | 7 | 11 |
| 16 | Opus 5 | `high` | 49.70% | - (new) | 10 |
| 17 | Opus 5 | `medium` | 45.15% | - (new) | 12 |
| 18 | Fable 5 | `max` | 44.85% | 8 | 13 |
| 19 | Fable 5.1 | `low` | 43.33% | - (new) | 14 |
| 20 | GLM-5.3 | `max` | 41.21% | 9 | 15 |
| 21 | Haiku 5.5 | `max` | 40.00% | 10 | - |
| 22 | Grok 4.7 | `xhigh` | 38.79% | 11 | 16 |
| 23 | GPT-5.6 Sol | `max` | 38.48% | 12 | 17 |
| 24 | Opus 5 | `low` | 34.85% | - (new) | 18 |
| 25 | Opus 4.8 | `max` | 23.64% | 13 | 19 |
| 26 | Kimi K3 | `max` | 23.03% | 14 | - |
| 27 | GPT-5.6 Terra | `max` | 21.52% | 15 | 20 |
| 28 | Grok 4.6 | `high` | 19.70% | 16 | 21 |
| 29 | GPT-5.6 Luna | `max` | 17.58% | 17 | 22 |
| 30 | GPT-6 Luna | `max` | 15.15% | 18 | 23 |
| 31 | Sonnet 5 | `max` | 12.12% | 19 | 24 |
| 32 | GPT-5.4 Mini | `xhigh` | 1.82% | 20 | - |

### Anthropic: Choosing a model

| Group | Old ranks | New ranks |
| --- | --- | --- |
| `cross_cutting` | 1 Opus 5.5; 2 Fable 5.1; 2 Fable 5.2 | 1 Opus 5.5; 2 Fable 5.2 |
| `concurrency_data_loss` | 1 Fable 5.1; 1 Fable 5.2; 2 Opus 5.5 | 1 Fable 5.2; 2 Opus 5.5 |

No remaining entry changed rank.

### Anthropic: Optimizing for cost and intelligence, `read_only_survey`

No entry changed rank. Fable 5.2 holds 1 to 3 (`high`, `medium`, `low`), and
Fable 5.1 stays at 4. Its best mean, 0.847 at `high`, is below Fable 5.2's
lowest, 0.866 at `low`.

## What the refresh changes in the older notes

- **Effort on Terminal-Bench.** The 2026-11-03 note says GPT-6 Astra scored
  57.88% at `high`, `xhigh` and `max`. On the 4.1 board it scores 60.00%,
  59.70% and 60.30%, all within each other's 95% margins. The note also says
  GPT-6.1 Sol at `max` matched GPT-6 Astra at `max` (58.18%). On the 4.1 board
  GPT-6.1 Sol scores 60.91% and Astra 60.30%, still within the margins, and
  Sol still costs about a fifth as much. Opus 5 still scores lower at `max`
  (52.12%) than at `xhigh` (53.33%). Fable 5.1 now scores lower at `max`
  (57.27%) than at `xhigh` (57.58%), where the 4.0 board had them level.
- **"Older entries that are now out of date" in the 2026-11-13 note.** The
  Terminal-Bench, choosing-a-model and defect-search items listed there are
  now refreshed.
- **"Where sources disagree" in the 2026-11-03 note** compares Anthropic's
  Terminal-Bench 4.0 numbers with the public board's 4.0 numbers. The public
  board's figures are now 63.94% for Sonnet 5.5 at `max` and 65.76% for Opus
  5.5 at `max`.

## Open question for the lead

The vendor groups "Introducing Claude Haiku 5.5: Terminal-Bench 4.0" and
"Introducing Claude Sonnet 5.5: Terminal-Bench 4.0" still carry Anthropic's
Terminal-Bench 4.0 numbers from its dated release pages. This refresh left
them as they are. Their claims compare with the public board's 4.0 numbers.
For example, the Sonnet 5.5 entry says 70.6% is "higher than the 61.82% the
public leaderboard shows". The board now shows 63.94% for that row. Whether
those groups, or the wording of their comparisons, should move to 4.1 is not
decided here.
