# Research: sets of read-only members on several model families

Written 2026-11-19, from a scouting round run on 2026-11-18 and 2026-11-19.
The note adds to `2026-11-03-elephant-goldfish-model.md` and
`2026-11-03-model-evidence.md` and replaces neither. Section 7 lists the
sentences of the elephant-goldfish note that the scouting showed to be out
of date.

The note reports what the scouting found. teamwire names no best model
(architecture decision record, or ADR, 0033). So the model names below are
the models each source measured or priced, and none of them is a pick for a
set.

## Words

| Word | Meaning in this note |
| --- | --- |
| Family | The vendor line of a model, such as Claude, GPT or Gemini. |
| Set | Two or three read-only members that do the same judging job on the same brief. Each member runs a model of a different family. The members never talk to each other. |
| Reviewer set | A set of `reviewer` members that read one finished diff. |
| Plan-reviewer set | A set of `plan_reviewer` members that read one plan and its briefs. |
| Planner set | A set of planners that each write a plan for one task. A planner is an `investigator` with `"quiet": true`. |
| Reconciler | A new member kind, `reconciler`. The reconciler reads the set's reports, checks each finding in the code and writes the one report the next step acts on. |
| `quiet` | The member setting that stops a member from sending messages. |
| Joined plan | One plan that a member wrote from the plans of several planners. |

Step 1, built now, is the reviewer set and the plan-reviewer set. The
planner set is a later step.

## How the scouting ran

Five members each wrote one report. The lead read all five in full. The
lead also opened the two main papers, arXiv 2610.01471 and 2606.01490, and
both say what the report says.

| Report | What it covered | Read on |
| --- | --- | --- |
| `piconf` | A pi configuration that already runs a planner set and a reviewer set, read from a clone at commit `761cbc3`. | 2026-11-18 |
| `twpath` | How teamwire carries a model from the spec to the running member, and every written rule a set meets. Read from code at `e427e22d`, release v0.28.1. | 2026-11-18 |
| `evidence` | 24 graded outside sources on several families in one judging job. | 2026-11-18 |
| `ownruns` | The user's 196 archived teams, with the cost of each member. | 2026-11-18 |
| `latest-models` | Measured results for the newest Claude, GPT, Gemini and other models in judging seats. | 2026-11-19 |

The `evidence` report graded each source as "measured, on point",
"measured, nearby", "field report" or "opinion". The `latest-models` report
graded each source as `benchmark`, `vendor`, `field report` or `opinion`.
Section 6 keeps each report's own grade.

## 1. What the outside evidence says

No study of several families covers the newest models. The newest models in
any such study are Opus 4.6 and 4.7, GPT-5.4 and 5.5, and Gemini 2.5.

### 1.1 One reviewer on another family

Song (arXiv 2610.01471) is the only controlled study that holds the author
fixed and changes the reviewer's family. Claude Opus 4.6 wrote 30 artifacts
with 150 planted errors, and 900 review sessions followed. F1 below is the
harmonic mean of precision and recall.

| Reviewer | F1 | Precision | Recall |
| --- | --- | --- | --- |
| GPT-5.4, given the request | 32.3 | 28.6 | 38.4 |
| GPT-5.4, artifact only | 32.1 | 27.5 | 39.8 |
| Gemini 2.5 Pro, artifact only | 29.0 | 29.2 | 29.3 |
| Claude, fresh session, artifact only | 28.6 | 31.5 | 27.1 |
| Claude, same session | 27.1 | 26.2 | 28.3 |
| Gemini 2.5 Flash, artifact only | 27.0 | 34.2 | 23.9 |

- The paper finds the GPT-5.4 reviewer "not significantly different in F1
  from same-model review in a fresh session". The paper adds that the result
  "does not establish equivalence".
- The GPT-5.4 reviewer found the most planted errors and raised more false
  findings.
- The paper names its own limits. Reviewer strength and model family are
  confounded, and nobody annotated the planted errors independently.
- 187 of 1,995 cross-family false findings came from model bias. In 136 of
  those, the reviewer claimed that a tool or feature the author used does not
  exist. The paper sorted the false findings with keywords, and it says that
  "the classification is keyword-based and was not validated".

Greptile ran 1,000 real pull requests and measured the recall of
high-severity bugs. Each model's recall was higher in the other model's pull
requests. The post's chart puts the gap at about 2.0 percentage points for
GPT-5.5 on Claude Code's pull requests, and 3.2 for Opus 4.7 on Codex's pull
requests. Greptile sells a
review tool, reports no variance and builds its ground truth from a
heuristic. A design-document study (arXiv 2606.01490) scored a reviewer from
another family at 4.606 of 5, against 4.493 for a same-family reviewer.
Model judges gave every score, with no human check.

Three sources now test a reviewer on another family. None of the three found
a large effect.

### 1.2 A second reviewer

In Song's study, a fresh Claude review plus a GPT-5.4 review matched 85 of
150 planted errors (56.7%). Two fresh Claude reviews matched 64 (42.7%). The
mixed pair's gain over the two Claude reviews has a Holm-adjusted p of .006.
Two GPT-5.4 reviews that saw only the artifact were compared with the mixed
pair on the 29 artifacts with no failed session. The mixed pair scored 4.1
points higher, with an adjusted p of .184, so it did not beat them. So the
gain may come from GPT-5.4 being the stronger reviewer, and not from its
family.

Kumar et al. (arXiv 2606.15689) found that every two-model union scored a
lower F1 than the best single model. Those union numbers do not add up,
because a union of findings cannot lower recall. The result needs care.
Song's "More Rounds, More Noise" (arXiv 2603.16244) found that a reviewer
that reviews again raises recall by 0.08 and raises false findings by 62%.

### 1.3 A third reader

No source measured a third reviewer for code review, with any family mix.
Song stops at two reviews, and Kumar et al. stop at pairs. Cursor's Bugbot
ran eight passes with a majority vote, but the post names no model and no
family. The case for a third reader rests on no measurement.

### 1.4 Joining the findings

Three ways to join findings appear in the sources. The table gives what each
way rests on.

| Rule | What the evidence says |
| --- | --- |
| Act on any finding | Bare model reviewers show low precision on planted errors, 21% to 35% in Song. Song counts only planted errors, so some "false" findings may be real problems. |
| Act only when two readers agree | No source tested the rule on code review. Two models that are both wrong pick the same wrong answer about 60% of the time, also across vendors (Kim et al.). On a maths question set, wrong models chose the same wrong answer 53% of the time (Denisov-Blanch et al.). |
| A judge confirms each finding in the code | Cursor runs a validator model after its vote and gives no separate number. Anthropic's Code Review reports under 1% incorrect findings after a verify step. JevCoder's abstract reports that a separate cheap judge cut repair tokens by 21.6% to 50.6%. |

No source compared the three rules on the same findings. No source measured
what agreement between reviewers predicts about a finding being true.

Two more results bear on the judge. Pombal et al. found judges "more than 50%
more likely" to wrongly pass their own output. Roytburg et al. found that only
51% of earlier self-preference findings survive a control for the judge's
own accuracy. Kim et al. found that same-company pairs agree a little more,
which weakly supports the 2026-11-03 guess that one vendor's models share
blind spots.

### 1.5 Joined plans

The evidence for joined plans is thin, and the evidence that exists points
against them.

- The design-document study (arXiv 2606.01490) is the only source that
  compares joined plans with one planner and one plan reviewer. Joined
  parallel plans scored worst of 12 setups, at 3.655 to 4.326 of 5.
- In the same study, one planner with a reviewer from another family scored
  second, at 4.606 and $0.27 a run. Model judges gave every score, and nobody
  built a plan.
- The paper blames the merging member's output budget. The merger read about
  25,000 tokens and wrote 4,200 to 5,200. Nobody tested a larger budget.
- Zhang et al. (arXiv 2502.08788) found that debate often fails to beat one
  model sampled several times. Mixing GPT-4o-mini and Llama 3.1 70B beat the
  best single family in 1 of 8 code cells.
- Wynn et al. (arXiv 2509.05396) found that debate lowered group accuracy in
  21 of 30 setups. Mixed groups with weaker members lost most.
- Li et al. (arXiv 2502.00674) found that joining outputs of one strong model
  beat a mixture of different models in most scenarios.
- Trae Agent mixed three families for patches. The mixture scored 65.67%,
  against 66.40% for Claude 3.7 Sonnet alone with the same selector.
- Aider paired a planner model with an editor model and gained. Same-family
  pairs gained too, so the gain is not clearly a family effect.

No source built and tested a joined plan. The pi configuration's own
backtest points the same way, and section 3.1 gives it.

### 1.6 The newest models

The `latest-models` report found these points.

- pi reaches every newest family through OpenRouter. The list includes
  Gemini 4 Carbon, Argon and Hydrogen, GPT-6.2 Astra and Sol, and Claude
  Fable 5.2.
- Gemini 4 Neon and Qwen4 Max are on openrouter.ai but not on pi's list on
  this machine.
- No source measures a set of several families on the newest models. Arbiter
  issue #829 plans such a readout and holds no result.
- No source measures any newest model on writing or reviewing a plan.
- Code-review results exist for Opus 5.5, GPT-6.1 Sol and Gemini 4 Argon. No
  code-review result exists for Fable 5.2, GPT-6.2 Astra, GPT-6.2 Sol, or
  Gemini 4 Carbon, Hydrogen or Neon.

The sources that measured more than one family in a judging seat give the
results below. F2 weighs recall above precision. The sources disagree on the
order of the families. IronBee tells readers to compare its groups, not its
ranks.

| Source | Claude | GPT | Gemini | Other |
| --- | --- | --- | --- | --- |
| IronBee, F2 on 50 pull requests | Sonnet 5.5 low 64.4, Opus 5.5 low 62.1 | GPT-6.1 Sol xhigh 47.5 | Gemini 4 Argon high 56.6 | Grok 4.7 low 61.9 |
| Kodus CodeReviewBench, F2 balanced | Opus 5.5 43.9 | GPT-6.1 Sol 45.9 | Gemini 3.8 Flash 18.2 | Kimi K3.1 Pro 63.5 |
| Qodo, F1 | Opus 5.5 high 56.8% | GPT-6 Sol medium 48.7%, GPT-6 Astra high 46.6% | Gemini 3.8 Flash high 47.0% | none listed here |
| IronBee, each judge's agreement with the panel, Kendall's tau | Opus 5.5 0.96 | GPT-6.1 Sol 0.94 | Gemini 4 Argon 0.94 | Grok 4.7 0.93 |

Effort changed the results.

- IronBee measured Gemini 4 Argon with no effort setting at 6 points of F2
  below Argon at `high`. Google's thinking guide says Carbon and Argon
  default to `medium`.
- IronBee found low effort as good as any higher level for its five top-group
  models. The highest level cost up to 3.5 times as much.
- Qodo found that "the best reasoning level is different for every model".

Gemini outside Google's own harness has few results and some failures.

- FrontierCS 2 ran Gemini 4 Argon at a 54% pass rate in Google's
  Antigravity harness and 40% in pi. The page does not explain the gap.
- OpenCode issue #60940 reports Gemini 4 Argon failing long tool-heavy turns
  mid-stream after about 140 to 173 seconds.
- Google's model cards list "occasional slowness or timeout issues" for
  Argon, Carbon and Hydrogen.

teamwire's evidence file holds no Gemini entry today. The `latest-models`
report drafted 83 entries for the file, 51 for `judgement` and 32 for
`read_only_survey`.

### 1.7 What nobody measured

No source found by the scouting measured any of the following for coding
agents.

1. Planner sets of several families whose plans were then built and tested.
2. A third reviewer, by family mix.
3. "Act only when two reviewers agree" on code findings.
4. The three joining rules compared on the same findings.
5. What agreement between reviewers predicts about a finding being true.
6. The wall-clock time of a set of three against one reader.
7. Any study of several families on Claude 5.x, GPT-6 or Gemini 4 models.
8. Agent-sized diffs under a control.
9. A plan review by another family, built and tested.

## 2. The user's own runs

### 2.1 What past teams show

The archive holds 182 reviewers, 14 plan reviewers and 101 closers. Every
one ran on Claude except one reviewer on GPT-6 Astra, in the trial team
`xvtrial-2026-11-18`.

Plan review leaves gaps today. Each of the 17 teams with a plan review later
hit a problem that the plan caused and the plan reviewer had not reported.
The count per team runs from 2 to 9, with a median of 4. Plan reviews
reported a median of 13.5 findings each, and leads rejected 5 of 246.

| Kind of miss | Teams |
| --- | --- |
| A file the fix must change, but no member owns it | 7 |
| Text written while fixing the review's findings caused the next problem | 6 |
| A check command or base ref did not hold once the work ran | 5 |
| The reviewer checked a premise and cleared it, but the premise was wrong | 3 |

The grouping of misses is the helper's judgement, and each team's file marks
its judgement calls.

Findings that reach an owner mostly hold. In a hand-checked sample, 39 of 40
held. The lead had already dropped some findings before sending, so the rate
comes after the lead's triage.

Extra readers have paid off before. Ten teams put two or more Claude readers
on one diff. The 12 extra readers cost 129 reader-minutes, added 37 findings
that no other reader had, and the lead acted on at least 18. Extra readers
added most when they read a different part, such as the tests, the docs or
the hand-off between two repositories. Three extra readers added nothing
unique.

No team reviewed the same plan twice. teamwire stores no record of which
reader raised which finding, or whether the finding held. The PASS, FLAGGED
and FAIL columns of `teamwire models --for judgement` read 0 for every
judging seat.

### 2.2 What each set would cost

The medians below come from past members on Opus 5.5. The dollars are
list-price equivalents with cache writes at the 1-hour rate. Cache reads make
up about 97% of each token total.

| Seat | Median tokens | Median dollars | Median minutes |
| --- | --- | --- | --- |
| Reviewer | 8.87M | $4.47 | 10.2 |
| Plan reviewer | 10.72M | $4.68 | 10.9 |
| Closer | 15.66M | $6.37 | 17.4 |
| Planner, to its first completion | 27.66M | $12.12 | 26.9 |

The `ownruns` report priced each non-Claude member at OpenRouter rates from
pi's bundled catalog. The report used GPT-6 Astra, GPT-6.1 Sol and Gemini 4
Argon as pricing examples. The planner set's joining member was priced as one
plan reviewer.

| Set | All Opus 5.5 | Opus 5.5, GPT-6 Astra, Gemini 4 Argon | Opus 5.5, GPT-6.1 Sol, Gemini 4 Argon | Added wall time |
| --- | --- | --- | --- | --- |
| Reviewer set, 3 reviewers | $13.41 | $19.68, of which $15.21 goes to OpenRouter | $7.85, of which $3.38 goes to OpenRouter | about 10 to 15 minutes |
| Planner set, 3 planners and 1 joining member | $41.04 | $62.19, of which $45.39 goes to OpenRouter | $26.80, of which $10.00 goes to OpenRouter | about 38 minutes |

The figures rest on assumptions that the report states.

- A non-Claude member is assumed to use the same tokens as the median Claude
  member in its seat. On the one shared diff in `xvtrial`, GPT-6 Astra used
  0.42 times Opus's tokens, cost 1.66 times as much and took 1.96 times as
  long.
- GPT-6 Astra costs 7 to 9 times as much per member as GPT-6.1 Sol or Gemini
  4 Argon. Its price doubles above 272,000 input tokens, and the reviewer set
  then comes to $32.11.
- A model that does not cache bills its cache reads as input. For GPT-6 Astra
  that comes to about $86 per reviewer.
- Planners run in parallel, so the wall time counts one planner plus the
  joining member. The slowest of three planners runs longer than the median,
  so 38 minutes is a low bound.
- The reviewer-set rows include no joining member, because the report assumed
  the lead joined the findings.
- The prices come from pi's catalog dated 3 November. GPT-6.2 Astra, released
  2026-11-17, lists at $8 per million input tokens and $40 per million output
  tokens on openrouter.ai. Nobody priced the sets again with it.
- At scouting time teamwire recorded tokens and no money. pi already wrote
  each reply's money into its session file.

Today one reviewer costs about $4.47 and 10 minutes. One plan reviewer costs
about $4.68 and 11 minutes.

## 3. The pi configuration, and what pi reaches

### 3.1 How the pi configuration runs sets

The `piconf` report read a pi configuration that already runs both kinds of
set. A parent pi session follows prose playbooks, and no program runs the
loop.

The planner set works as follows.

- The parent sizes the task from a recon map and recommends a path. The user
  picks the path.
- A full debate is recommended for more than about 5 chunks, a migration, a
  contract shared across repositories or an open design question.
- One planner and a plan review are recommended for about 5 chunks or fewer.
  No plan is made for a change to one or two files.
- In a full debate, three planners on a Claude, a GPT and a Gemini model read
  one shared context file and get the same task text.
- A reconciler on Claude Opus 5.5 reads the three plans. The reconciler
  settles a dispute about a fact by reading the code and citing `file:line`.
  A dispute about judgment goes to the human.
- The reconciler's rule on agreement reads "Agreement is signal, not proof".
- The cap is 3 rounds. The playbook does not say what happens at the cap.

The reviewer set works as follows.

- Each built chunk gets one fresh reviewer, always on the Gemini model.
- After every chunk is clear, three reviewers on Opus 5.5, GPT-6 Astra and
  Gemini 4 Argon read the whole diff once, in parallel.
- The parent acts on a finding when two or more reviewers raised it. The
  parent also acts on a finding that one reviewer raised, if the parent
  confirms it and says why.
- Later rounds use one Opus reviewer that checks only the fix. The cap is 5
  rounds.

The configuration has weak points of its own.

| Weak point | What the report found |
| --- | --- |
| Nothing checks that a set holds three vendors | The code checks only that the model strings in a set differ. Two Claude models would pass. |
| Planners lose their independence | From round 2 the parent resumes each planner with the competing positions. |
| The joining step is not blind to the model | The plan files are numbered in the order of the models. The reviewer-set join names the model behind each finding. |
| The reconciler shares a family with one planner | The reconciler runs on Opus 5.5, and the first planner runs on Claude Fable 5.2. |
| Fallback models are not written | The router's README says fallback lists are written, but the code deletes them. |
| A failed model can repeat a family | If Opus is demoted, two of the three final-review slots resolve to GPT-6 Astra. The report inferred this from code and did not run it. |

Only one measurement of the sets exists in the configuration. A backtest
dated 2026-11-03 found that one plan reviewer on one plan caught 12 of the 14
fact errors that the three-planner debate and its reconciler found. The plan
reviewer also caught the plan's largest risk, which the reconciler missed.
The 14 errors are the debate's own findings, so the debate is the reference.
The clone holds no raw data for the backtest and no dollar, token or time
figure per set.

### 3.2 What pi reaches, and how a pi member differs

On 2026-11-18, `teamwire models --harness pi` listed 310 models, all through
`openrouter`, from 44 vendor prefixes. No `claude-bridge` row remained. The
user's `~/.teamwire/config.toml` excludes `amazon-bedrock` through
`TEAMWIRE_PI_EXCLUDE_PROVIDERS`.

| Point | Claude Code member | pi member |
| --- | --- | --- |
| `plugins` map | Written into the run's `settings.json`. | Ignored. |
| Memory | The skill turns off `claude-mem` for read-only members. | teamwire has no switch. Nobody checked the user's pi extensions for a memory feature. |
| Token counts | Read from the Claude transcript. | Read from pi's session file since 2026-11-10. |
| Effort levels | `low` to `max` | `off` to `max`, passed as `--thinking`. pi's catalog maps Gemini 4 Carbon and Argon to `low`, `medium` and `high` only. |
| Context handoff | teamwire sets a compaction backstop. | teamwire sets no compaction, so the lead hands the member off. |
| A listed model that cannot be called | Launch cannot tell from the built-in list. | Launch passes, and no code sees the provider error. The watch reports the member only as `SILENT` or `LATE`. The report inferred this from code and did not run it. |

So the independence of a pi member in a set rests on the user's pi setup
having no memory extension.

## 4. How teamwire picks a model, and which rules a set meets

A model reaches a member only as free text in the spec's `model` and
`effort`. Launch checks both against the harness's model list. Launch refuses
an unknown effort and an unknown provider prefix, and only warns about a
model the list cannot match.

At scouting time no code knew a model's family.

- `ModelInfo.provider` holds the route, such as `openrouter`, not the vendor.
- `normalise_model` drops the vendor segment that a family rule would need.
- ADR 0053 lived in skill text plus one test that pins the text. ADR 0053
  rejected having teamwire pick the harness from the model id.
- A family rule needs a table of vendor prefixes. The pi ids hold awkward
  cases, such as `~openai/gpt-sol-latest`, router ids like `openrouter/auto`,
  both `meta` and `meta-llama`, and `google/gemma-*`, which is Google but not
  Gemini.
- The evidence file holds 247 entries, no Gemini entry and no family field.

Several readers on one piece of work meet no code limit.

- Launch counts no reviewers, plan reviewers or investigators.
- Each reviewer gets its own clone folder, named by its role. Two closers
  would share one clone.
- The trial `xvtrial` ran a Claude reviewer on Claude Code and a GPT reviewer
  on pi on one diff, with no code change.

A joining member works today as an `investigator`. Every member may read
another member's report by path, and the lead adds the joining member with
`launch --add` after the set reports. An investigator may send messages,
though, and its results share a row in the outcome report with every other
investigator. ADR 0025 already used the `investigator` seat for the
away-from-keyboard (AFK) planner and rejected a `planner` kind.

A new kind costs more than a new preset. The `plan_reviewer` kind touched 33
files, added 1,498 lines and rebuilt the `members` table. A new preset touched
4 files, plus count words in 3 documents.

The `twpath` report also described a Codex adapter and a Codex lead. Codex
support was removed in v0.29.0 (ADR 0054), so those parts no longer apply.

## 5. Conflicts with existing rules

| Existing rule | Applies to | The conflict |
| --- | --- | --- |
| "ONE independent reviewer" in the `implement-then-review` preset, and "never a second review of the same diff" in the skill | Reviewer set | The preset text contradicts a set word for word. The skill text speaks of a review after a review, but its reason, more findings and more fixes, applies more strongly to a set. |
| "Never review the same plan twice" | Plan-reviewer set | The same question of wording as the row above. |
| The lead plans the work and writes every brief | Planner set | A planner set contradicts the rule for an ordinary team. The AFK skill already has a written exception. |
| No always-on coordinator | Reconciler | No conflict if the reconciler runs once, sends nothing and only reports. |
| teamwire names no model and no best model (ADR 0033), and a test fails any preset that sets a model | All sets | A set that names models contradicts the rule. A set described by families does not. |
| No code knows a model's family | All sets | A family check needs skill text or a table of vendor prefixes in code. |
| A pi member ignores the `plugins` map | All sets | A pi member's independence rests on the user's pi setup. |
| A pi member that cannot reach its model raises no error | All sets | The lead needs a rule for a set that lost a member. |
| An investigator may send messages and shares outcome rows with other investigators | Reconciler and planner set | A quiet seat needs a new kind or a `quiet` setting. The last new kind touched 33 files and rebuilt a table. |
| The reviewer gets "the stronger model and the higher effort of the two" | Reviewer set | No evidence row compares strength across families. |
| The evidence file has no Gemini entry and no family field | Model choice | A Gemini choice rests on no evidence row today. |
| teamwire stores no record of which reader raised which finding, or whether it held | Measuring sets | Nothing would show what each family added. |
| Every lead feature ships for every lead harness, Claude Code and pi | All sets | A set built from ordinary members meets the rule. A set that needs a Claude Code subagent does not. |

## 6. Sources

### 6.1 Sources on several families, sets and joining

From the `evidence` report. Every source was opened on 2026-11-18. In the
titles below, LLM stands for large language model, AI for artificial
intelligence, SOTA for state of the art, CLI for command-line interface and
SWE for software engineering.

| Source | Date | URL | What it found | Grade |
| --- | --- | --- | --- | --- |
| Song, "When Does a Second Model Help? Cross-Model Review in LLM Verification" (arXiv 2610.01471 v2) | 2026-10-01, rev. 2026-10-06 | https://arxiv.org/html/2610.01471v2 | A GPT-5.4 reviewer is not significantly better in F1 than a fresh Claude session. A fresh Claude plus GPT-5.4 pair matched 56.7% of planted errors, against 42.7% for two fresh Claude reviews. | measured, on point |
| Song, "Cross-Context Review" (arXiv 2603.12123 v2) | 2026-03-12, rev. 2026-10-01 | https://arxiv.org/abs/2603.12123 | A fresh session beat a second pass in the same session. A fresh session did not differ significantly from the first same-session review. | measured, nearby |
| Greptile, "Models are worse at reviewing their own code" | 2026-07-21 | https://www.greptile.com/blog/model-inversion | Each model's recall of high-severity bugs was higher in the other model's pull requests, by about 2.0 and 3.2 percentage points on the post's chart. The post reports no variance. | measured, on point (a review-tool maker with a stake, weak controls) |
| Kumar, Bararia, Raj, "Bigger Isn't Always Better" (arXiv 2606.15689) | 2026-04-09 | https://arxiv.org/html/2606.15689v1 | Every two-model union scored a lower F1 than the best single model. The union numbers do not add up. | measured, on point |
| Kanamarlapudi and K, "LLM Consortium for Software Design Refinement" (arXiv 2606.01490) | 2026-05-31 | https://arxiv.org/html/2606.01490v1 | Joined parallel plans scored worst of 12 setups. One planner with a reviewer from another family scored second. | measured, on point (design documents, judged by models, no human check) |
| Kim, Garg, Peng, Garg, "Correlated Errors in Large Language Models" (arXiv 2506.07962, International Conference on Machine Learning 2025) | 2025-06-09 | https://arxiv.org/html/2506.07962v1 | Two wrong models pick the same wrong answer about 60% of the time. Same-company pairs agree a little more. | measured, nearby |
| Pombal, Rei, Martins, "Self-Preference Bias in Rubric-Based Evaluation" (arXiv 2604.06996 v3) | 2026-04-08, rev. 2026-08-03 | https://arxiv.org/abs/2604.06996 | Judges can be more than 50% more likely to wrongly pass their own output. Joining judges reduces the bias and does not remove it. | measured, nearby (abstract only) |
| Roytburg et al., self-preference after an evaluator-quality baseline (arXiv 2601.22548 v4, International Conference on Machine Learning 2026) | 2026-01-30, rev. 2026-06-22 | https://arxiv.org/abs/2601.22548 | Only 51% of earlier self-preference findings keep significance after a control for the judge's accuracy. | measured, nearby (abstract only) |
| Tsui, "Self-Correction Bench" (arXiv 2507.02778 v3, Conference on Language Modeling 2026) | 2025-07-03, rev. 2026-08-02 | https://arxiv.org/abs/2507.02778 | 14 small open models fail to fix an error framed as their own 64.5% of the time. | measured, nearby |
| Song, "More Rounds, More Noise" (arXiv 2603.16244) | 2026-03-17 | https://arxiv.org/abs/2603.16244 | Reviewing again raised recall by 0.08 and false findings by 62%. | measured, nearby |
| Cursor, "Building a better Bugbot" (Jon Kaplan) | 2026-01-15 | https://cursor.com/blog/building-bugbot | Eight passes with a majority vote, then a validator model. The post names no model and gives no control. | field report |
| Denisov-Blanch et al., "Consensus is Not Verification" (arXiv 2603.06612) | 2026-02-20 | https://arxiv.org/html/2603.06612 | Wrong models chose the same wrong answer 53% of the time on maths questions. No way of joining answers consistently beat one sample. | measured, nearby |
| Zhang, Zhang, Huang, "JevCoder" (arXiv 2610.19596) | 2026-10-14 | https://arxiv.org/abs/2610.19596 | A separate cheap judge cut repair tokens by 21.6% to 50.6%. The abstract does not name the families. | measured, nearby (abstract only, families not stated) |
| Zhang et al., "Stop Overvaluing Multi-Agent Debate" (arXiv 2502.08788 v3) | 2025-02-12, rev. 2025-06-21 | https://arxiv.org/abs/2502.08788 | Debate often fails to beat one model sampled several times. A mixed pair beat the best single family in 1 of 8 code cells. | measured, on point for the two code benchmarks, nearby otherwise |
| Wynn, Satija, Hadfield, "Talk Isn't Always Cheap" (arXiv 2509.05396 v2) | 2025-10-13 | https://arxiv.org/html/2509.05396v2 | Debate lowered group accuracy in 21 of 30 setups. Mixed groups with weaker members lost most. | measured, nearby |
| Li, Lin, Xia, Jin, "Rethinking Mixture-of-Agents" (arXiv 2502.00674) | 2025-02-02 | https://arxiv.org/abs/2502.00674 | Joining outputs of one strong model beat a mixture of different models in many scenarios. | measured, nearby (abstract only) |
| Gao et al., "Trae Agent" (arXiv 2507.23370) | 2025-07-31 | https://arxiv.org/html/2507.23370v1 | A mixture of three families with a selector scored 65.67%, below Claude 3.7 Sonnet alone at 66.40%. | measured, on point (patches, not plans) |
| Aider, "Separating code reasoning and editing" | 2024-09-26 | https://aider.chat/2024/09/26/architect.html | A planner model plus an editor model beat the planner alone. Same-family pairs gained too. | measured, on point (older than 2025) |
| Aider, "R1+Sonnet set SOTA" | 2025-01-24 | https://aider.chat/2025/01/24/r1-sonnet.html | R1 planning with Sonnet editing scored 64.0% for $13.29, against o1 alone at 61.7% for $186.50. | measured, on point |
| OpenAI, `codex-plugin-cc` README | undated, read 2026-11-18 | https://cdn.jsdelivr.net/gh/openai/codex-plugin-cc@main/README.md | Codex reviews inside Claude Code, with no measurement. Codex no longer applies to teamwire (ADR 0054). | field report (practice only, no outcome) |
| Amp (Sourcegraph), "GPT-5 Oracle" | 2025-10-01 | https://ampcode.com/news/gpt-5-oracle | A GPT adviser that a Claude main agent calls for planning and debugging, with no numbers. | opinion |
| CodeRabbit, "The more AI writes the code, the more review needs independence" | 2026-06-17 | https://coderabbit.ai/blog/code-review-needs-independence | Argues for a reviewer from a different vendor than the author. Its own claims carry no data. | opinion (vendor with a stake) |
| Anthropic, "The advisor strategy" | 2026-04-09 | https://claude.com/blog/the-advisor-strategy | Opus as adviser to Sonnet gave 2.7 more points on SWE-bench Multilingual and 11.9% lower cost per task, within one family. | measured, nearby, vendor |
| Factory, CLI model settings | undated, read 2026-11-18 | https://docs.factory.com/cli/configuration/mixed-models | Separate model settings per planning and validating seat. The page says nothing about mixing vendors. | field report (practice only) |

The report also opened arXiv 2507.06920 and did not cite it. CodeRabbit
quotes a 9 to 17 point same-family figure from that paper, and the abstract
does not contain the figure.

### 6.2 Sources on the newest models

From the `latest-models` report. Every page was opened on 2026-11-19. In the
names below, QnA stands for question and answer, and API for application
programming interface.

| Source | Date | URL | What it found | Grade |
| --- | --- | --- | --- | --- |
| IronBee, "The Honest Raw LLM Code Review Benchmark" | 2026-10-19 | https://ironbee.ai/blog/the-honest-raw-llm-code-review-benchmark | 50 pull requests, one call each, no tools, F2. Claude rows lead, Gemini 4 Argon sits in group 2 and GPT-6.1 Sol in group 4. Three judges from other families agreed at Kendall's tau 0.93 to 0.96. | benchmark (sells a review tool, publishes all data) |
| Kodus CodeReviewBench leaderboard | runs dated 2026-11-07 to 2026-11-09 | https://www.codereviewbench.com/leaderboard | 30 pull requests, 121 confirmed bugs, F2. Kimi K3.1 Pro first, GPT-6.1 Sol second, Opus 5.5 third. The only Gemini row is Gemini 3.8 Flash, 12th of 13. | benchmark (sells a review tool) |
| Qodo, "Which LLM should review your code?" | 2026-10-12 | https://www.qodo.ai/blog/which-llm-should-review-your-code/ | 125 private pull requests, F1. Opus 5.5 high first at 56.8%. GPT-6 Sol 48.7%, Gemini 3.8 Flash 47.0%. | field report |
| CodeRabbit, "Claude Opus 5.5 for code review" | 2026-09-22 | https://www.coderabbit.ai/blog/opus-5-5-model-review | Opus 5.5 caught 51 of 80 common bug patterns at 38.6% precision. No other family was tested. | field report |
| Artificial Analysis Coding Agent Index v2.1 | data materialized 2026-11-19 | https://artificialanalysis.ai/agents/coding-agents | Codebase question answering. Opus 5.5 max 65.9%, Gemini 4 Carbon high 63.7%, GPT-6.2 Astra max 55.1%. Each model ran only in its own vendor's harness. | benchmark |
| Scale SWE Atlas Codebase QnA leaderboard | row created 2026-09-09, page update note 2026-07-28 | https://labs.scale.com/leaderboard/sweatlas-qna | One new Gemini row, Gemini 3.8 Flash at 47.04. No Gemini 4, GPT-6.2, Fable 5.2 or Opus 5.5 row. | benchmark |
| Scale SWE Atlas Refactoring leaderboard | no date in the scouting report | https://labs.scale.com/leaderboard/sweatlas-refactoring | GPT 6 Astra xHigh 59.05, Fable 5.1 56.67, Gemini 3.8 Flash 44.76. No Gemini 4 row. | benchmark |
| FrontierCS 2, Gemini 4 Carbon post | 2026-10-29 | https://frontier-cs.org/blog/gemini-4-carbon/ | Pass rates on 48 research-coding tasks. Opus 5.5 58%, Gemini 4 Carbon 54%, Argon 54% in Antigravity and 40% in pi, GPT-6 Astra 44%. | benchmark |
| Terminal-Bench 4.1 leaderboard | row dates 2026-09-02 to 2026-11-12 | https://www.tbench.ai/ | Opus 5.5 max and Fable 5.2 max 65.8%, Gemini 4 Carbon high 64.5%, GPT-6.2 Astra max 62.1%. | benchmark |
| ARC Prize leaderboard (ARC-AGI-2 and ARC-AGI-3) | Gemini 4 Carbon row 2026-11-10 | https://arcprize.org/leaderboard | Reasoning, not judging. GPT-6.2 Astra first on ARC-AGI-2, Fable 5.2 first on ARC-AGI-3 Standard. | benchmark |
| METR time horizons | no date in the scouting report | https://metr.org/time-horizons/ | The newest models named are from early 2026. | benchmark |
| Scale SWE-Bench Pro public leaderboard | no date in the scouting report | https://labs.scale.com/leaderboard/swe_bench_pro_public | The newest rows are Muse Spark 1.1 and gpt-5.4. | benchmark |
| OpenCode issue #60940 | opened 2026-10-31 | https://github.com/anomalyco/opencode/issues/60940 | Gemini 4 Argon failed long tool-heavy turns mid-stream after about 140 to 173 seconds. | field report |
| Google AI Developers Forum thread on Gemini 4 Carbon tool-turn reasoning | 2026-11-15 to 2026-11-19 | https://discuss.ai.google.dev/t/counter-data-tool-aware-minimal-thinking-is-measurably-degrading-output-quality-on-agy-gemini-4-carbon/200671 | Users measured falling reasoning on tool turns in Antigravity, from 75.8% of Carbon tool steps on 2026-11-11 to 18.8% on 2026-11-18. | field report |
| Gemini API thinking guide | no date in the scouting report | https://ai.google.dev/gemini-api/docs/thinking | Carbon and Argon default to medium. Hydrogen and Neon default to high. | vendor |
| OpenAI reasoning models guide | no date in the scouting report | https://developers.openai.com/api/docs/guides/reasoning | xhigh lists "security and code review" among its uses, "Only use when your evals show a clear benefit". | vendor |
| Arbiter issue #829 | opened 2026-10-15 | https://github.com/serious-alchemy/arbiter/issues/829 | Plans a readout of GPT and Gemini reviewers of Claude work. The page holds no result. | opinion (a plan) |
| Gemini 4 model cards (Argon, Carbon, Hydrogen, Neon) | Argon 2026-10-13, Carbon 2026-11-10, Neon 2026-11, Hydrogen card date not given | no URL in the scouting report | Each card lists "occasional slowness or timeout issues" for Argon, Carbon and Hydrogen. Carbon and Neon cards give Google's own benchmark tables. | vendor |
| Google blog posts announcing Gemini 4 | Argon 2026-09-30, Carbon 2026-10-28, Neon 2026-11-18 | no URL in the scouting report | Carbon is "our strongest model". Neon sits between Carbon and Hydrogen. | vendor |
| Anthropic, Claude Fable 5.2 launch page | 2026-11-09 | no URL in the scouting report | Fable 5.2 sits above Opus 5.5. The page reports FrontierCode 2.0 for Fable 5.2, GPT-6 Astra and Gemini 4 Carbon. | vendor (also reports another lab's model) |
| OpenAI GPT-6.2 Astra model page | no date in the scouting report | no URL in the scouting report | Calls GPT-6.2 Astra "Our most capable model for your hardest projects". | vendor |
| Decrypt, GPT-6.2 launch article | 2026-11-17 | no URL in the scouting report | OpenAI "launched GPT-6.2 Astra and Sol on Tuesday". | no grade in the scouting report |
| openrouter.ai public model list | read 2026-11-19 | `openrouter.ai/api/v1/models` | Prices per model, used for the newest models that pi's bundled catalog lacks. | no grade in the scouting report |

The report did not cite pages it could not read or saw only in search
snippets. The skipped pages include OpenAI's GPT-6.2 launch page, which
refused the request with status 403, and livebench.ai, which served an empty
page.

## 7. Sentences in the 2026-11-03 elephant-goldfish note that are no longer true

Each quote below is copied from
`docs/research/2026-11-03-elephant-goldfish-model.md`. The line numbers are
that file's.

1. Section 3, lines 130 to 131.

   > The lead skill turns off claude-mem for them so that they stay independent.

   The sentence holds for Claude Code members only. A pi reviewer ignores the
   `plugins` map, so teamwire turns off nothing for it.

2. Section 5, line 180.

   > Section 7 gives a way to run a trial without changing the contract.

   The trial in section 7 runs a plan review as kind `reviewer` on a plan
   branch. ADR 0034 rejected that option and added the `plan_reviewer` kind.

3. Section 6, line 193.

   > A Goldfish on another model family is out of reach on this machine.

   ADR 0053 made a member on another family an ordinary setup. The trial
   team `xvtrial` ran a GPT reviewer on pi on 2026-11-18.

4. Section 6, line 196.

   > No source tested that idea.

   Three sources have now tested a reviewer on another family. Song, Greptile
   and the design-document study each found a small effect or none.

5. Section 6, lines 196 to 197.

   > On 2026-11-03, `teamwire models` from a pi session listed 14 models, all Claude models through `claude-bridge`.

   The paragraph uses the sentence as the current state. On 2026-11-18, pi
   listed 310 models, all through `openrouter`, from 44 vendor prefixes, and
   no `claude-bridge` row.

6. Section 6, line 199.

   > The user has no Codex subscription or API key.

   Codex support was removed in v0.29.0 (ADR 0054), so the sentence no longer
   bears on any member.

7. Section 6, lines 199 to 200.

   > So every reviewer on this machine runs a Claude model.

   pi reaches GPT and Gemini models through OpenRouter. The outcome report
   already holds a pi reviewer on `openrouter/openai/gpt-6-astra`.

8. Section 6, lines 207 to 211.

   > A plan reviewer with kind `reviewer` and class `judgement` lands in the same row as code reviewers. A later change could separate them, with a new class or a team-level record that a plan review ran. That choice should wait until trials show whether the plan review finds anything.

   ADR 0034 added the `plan_reviewer` kind on 2026-11-04. Plan reviews now
   have their own row in the outcome report.

9. Section 7, lines 221 to 222.

    > The lead launches one member with kind `reviewer`, mode `pane` and class `judgement`, before any implementer.

    Since ADR 0034 the member is kind `plan_reviewer`, from
    `teamwire preset plan-review`.

10. Section 10, lines 314 to 315.

    > Is a plan reviewer on a different Claude model from the lead's worth its cost over one on the same model?

    The question now covers a plan reviewer on another family as well, not
    only another Claude model.

The `twpath` report also listed section 6, lines 200 to 203: "A reviewer on
a different Claude model from the lead, such as Sonnet reviewing Opus's
plan, is possible." and "That is also untested." Both sentences stay true as
written, so the list above leaves them out. The paragraph around them
presents another Claude model as the only choice for a reviewer on another
model. A reviewer on another family is now a second choice.

## 8. What followed

The user's decisions after the scouting are recorded in decision records
written on 2026-11-20. ADR 0056 has code work out a model's family to label
and warn. ADR 0057 adds the `reconciler` kind and a set label. ADR 0058 adds
the `quiet` setting on any member. ADR 0060 shows in `teamwire metrics` the
money a harness recorded. ADR 0061 states the rules a lead follows for a set
of readers, and the presets `plan-review-set` and `review-set` carry them.
