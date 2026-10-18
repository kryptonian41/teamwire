# Research: field reports on leading a team of coding agents

Written 2026-10-18. This record explains why the lead skill
(`examples/skills/teamwire/SKILL.md`) and the member rules
(`crates/teamwire-launch/contracts/`) say what they say after the changes of
2026-10-18. The user approved the changes. The table in section 4 lists them.

## 1. The question, and how the evidence was gathered and checked

The question: what do published reports and teamwire's own runs show about
leading a team of coding agents, and which advice in the lead skill needs a
correction?

Four investigators worked in parallel on 2026-10-18. Each wrote one report with
numbered findings.

| Report | Finding ids | What it covers |
| --- | --- | --- |
| `labs.md` | L1-L29 | AI labs and tool makers: Anthropic, Cognition, Cursor, Factory, OpenAI, Google, GitHub |
| `bigco.md` | B1-B25 | Large companies that run coding agents: Stripe, Spotify, Uber, Google, Bun, Shopify |
| `studies.md` | S1-S22 | Papers on arXiv and Google Research |
| `ownruns.md` | O1-O17 | Counts over the 57 team databases in teamwire's own archive |

A fifth agent, the closer, then checked 57 findings. The closer fetched each
page and read each paper. The closer also re-ran the archive counts for O1, O2,
O7, O9 and O15 over all 57 databases.

The closer's verdict: 46 findings confirmed, 10 with a wrong detail, 1 with a
wrong conclusion (O6). The closer found no invented number. Every page and
paper loaded.

Three rules govern this record.

- Where the closer graded a finding `wrong detail` or `wrong conclusion`, this
  record uses the closer's text and says the finding was corrected.
- Every number and quote here is one the closer confirmed. A number the closer
  did not check is marked "not checked by the closer".
- Evidence levels: **measured** (a study or a count), **practice** (what an
  organisation reports doing), **internal** (teamwire's own archive), and
  **opinion**.

## 2. What the evidence says, by topic

### 2.1 When a team beats one agent

Teams win on work that splits into parts that do not depend on each other.
Teams lose on work with shared files or ordered steps.

| Claim | Source id | Level |
| --- | --- | --- |
| An Opus 4 lead with Sonnet 4 subagents "outperformed single-agent Claude Opus 4 by 90.2%". The task was research, not coding. Studies called this sentence unfetched. Labs was right, and the sentence is on the page. | L2 | measured |
| "most coding tasks involve fewer truly parallelizable tasks than research" | S1, L3 | practice |
| "For sequential tasks, same-file edits, or work with many dependencies, a single session or subagents are more effective." | L8 | practice |
| "Two teammates editing the same file leads to overwrites." | L8 | practice |
| Cognition: "writes stay single-threaded". Agents may read and research in parallel. Parallel-writer swarms are "mostly a distraction". | L15 | practice |
| Google Research: centralized coordination "improved performance by 80.9% over a single agent" on parallel work. "every multi-agent variant we tested degraded performance by 39-70" on sequential work. The paper itself says 80.8%. | S22 | measured |
| In the same paper, a team's per-agent reasoning is "prohibitively thin beyond 3–4 agents". The benchmarks are not coding tasks. | S4 | measured |
| Cursor: "Twenty agents would slow down to the effective throughput of two or three" when agents share locks. | L20 | practice |
| Parallel coding agents "can produce patches that work alone but fail when merged." On 417 real Django pairs, only one of 834 runs showed interference after the grading fix. The effect exists and is rare. | S10 | measured |
| Uber's pull-request attribution: more than 70% of its pull requests come from local or cloud agents. | B16 | practice |

The closer added a warning. Studies S7, S8 and S9 all read the same sample of
public agent pull requests (D10). Their agreement is weaker than three
separate studies would give.

### 2.2 Task size

| Claim | Source id | Level |
| --- | --- | --- |
| "a single-file patch that changes fewer than five lines is solved almost one time in two (48%)". With three or more files or over 100 lines, the success rate "falls below ten per-cent". | S6 | measured |
| Corrected: the S6 authors are Zhang, He, Zhang and others at Microsoft, not "Jain et al.". S6 measures the size of the task's reference fix, not the size of the agent's own diff. | S6 | measured |
| In agent pull requests on GitHub, the difference in lines changed between merged and not merged is 17%, and in files changed 10%. Each extra failed CI check lowers the odds of a merge by about 15%. | S7 | measured |
| Corrected: the S7 authors are Ehsani, Pathak, Rawal and others, not "Zhang et al.". | S7 | measured |
| Devin does best with "clear, upfront requirements and verifiable outcomes that would take a junior engineer 4-8 hrs of work". | L16 | practice |
| Anthropic's agent-teams docs: "Start with 3-5 teammates for most workflows." "Having 5-6 tasks per teammate." Too-small tasks cost more in coordination than they return. | L8 | practice |
| GitHub: the coding agent is "very good at handling small, atomic, and well-defined tasks". | L29 | practice |

The closer's note D14: S6 gives the stronger number but measures the task. S7
measures the diff, which is closer to what a lead controls. Both point the
same way.

### 2.3 What a brief needs

| Claim | Source id | Level |
| --- | --- | --- |
| "Each subagent needs an objective, an output format, guidance on the tools and sources to use, and clear task boundaries." Without it, "agents duplicate work, leave gaps, or fail to find necessary information." | L4 | practice |
| OpenAI's Codex prompt has four parts: Goal, Context, Constraints, and "Done when: What should be true before the task is complete". | L24 | practice |
| GitHub asks for "Complete acceptance criteria on what a good solution looks like" and "Directions about which files need to be changed." | L29 | practice |
| Spotify: "Claude Code does better with prompts that describe the end state and leave room for figuring out how to get there." "State preconditions." | B7 | practice |
| Anthropic best practices: name the files and interfaces, "state what is out of scope", and end with an end-to-end verification step. | L10 | practice |
| Devin "usually performs worse when you keep telling it more after it starts the task." The agent-teams docs say the opposite: redirect approaches that are not working. Neither side has a measurement (D9). | L16, L8 | practice |
| Rules grow from failures. Bun: "fixing the process that generates the code instead of hand-fixing the code". Stripe scopes rules to subdirectories. | B19, B4 | practice |

The closer checked B19. B4 was not checked by the closer.

Two findings come from teamwire's own runs.

- Members often settle the same question differently when the brief leaves it
  open (O6). The closer corrected the finding. The count is 5 runs, not 7. The
  claimed cure was wrong too. In the `deckcomments` run the shared brief gave a
  fixed commit trailer, and all four members wrote a different one. Each member
  followed the session's own attribution instruction instead. A brief cannot
  settle a convention that the harness also sets.
- Of 33 real questions members asked, 18 to 22 asked for an edit in a file
  outside the member's own `owns` (O7, count re-run by the closer). The closer got 18 to 22,
  depending on how four borderline questions are read. The lead answered 32 of
  the 33 questions. The median wait was 0.35 minutes. The cost was small, but
  the questions show files the plan missed.

### 2.4 Verification and reviewer error

| Claim | Source id | Level |
| --- | --- | --- |
| Anthropic: "agents reliably skew positive when grading their own work". "Out of the box, Claude is a poor QA agent." | L7 | practice |
| Multi-agent systems fail 41% to 86.7% of the time on 7 open-source systems. "no or incomplete verification" and "incorrect verification" are two named failure modes. A high-level objective verification step added +15.6% task success on one benchmark. | S3 | measured |
| In the C compiler build, "it's important that the task verifier is nearly perfect, otherwise Claude will solve the wrong problem". | L5 | practice |
| Spotify's judge vetoes about a quarter of sessions, and the agent recovers half the time. A change that "passes CI but is functionally incorrect. This is the most serious error". Corrected: the closer fixed the wording of two quotes, and the numbers hold. | B8 | measured |
| Bun: Claude read "let's get all the crates to compile" as "stub out the functions with compilation errors". Corrected: the member's quote was reworded. | B19 | practice |
| Tests that cannot fail: the closers in teamwire's own runs found tests that pass without the fix in at least 6 runs. The closer checked four citations, for example "No store test fails when `Store::write` uses a deferred BEGIN". | O4a | internal |
| Reviewer error rate. An LLM judging correct short functions against their requirement rejected 25-36% of them with a plain prompt for GPT-4o and Gemini. For Claude-4.5-Sonnet the rate was 26-58%. Asking it for explanations and proposed fixes raised the rate. The effect is large for GPT-4o and about a third as large for Claude. Corrected: the member led with GPT-4o. The task is one call on one function, with no repository and no tests. | S11 | measured |
| Anthropic: "A reviewer prompted to find gaps will usually report some, even when the work is sound." | L10 | practice |
| Anthropic's Code Review marks "less than 1%" of findings incorrect, after its own verify step. This is not a precision figure for a bare reviewer. | L11 | measured |
| Uber's reviewer: engineers mark 75% of comments useful and over 65% addressed. Readability nits and style comments "consistently received poor ratings". | B14 | measured |
| Tests written without seeing the code found 25% of seeded faults. Tests written after faulty code found 14%. Chain-of-thought prompting "appear to have no effect". | S12 | measured |

The closer's note D1: the four sources measure four different things. Between
a quarter and a third of a reviewer's output goes unused (S11, B14, the
archive). So the lead's triage step stays.

The closer's note D4: Spotify later removed its judge as "too rigid" once models
improved (B9, third-party report). Anthropic (L7) and Cursor (L21) did the same.
Each added a checker for one model generation and dropped it at the next.

### 2.5 Review rounds

No source measured a loop against a single review pass. The sources show
practice.

| Claim | Source id | Level |
| --- | --- | --- |
| In teamwire's runs, 125 implementers worked on teams with a reviewer or closer. 78 of them (62.4%) reached generation 2 or more. Counting only reprompts that carry findings, 68 of 125 (54%) got a fix round. The closer corrected the member's figure, which counted new work as fix rounds. | O2 | internal |
| Cognition: "Today, we make Devin and Devin Review natively iterate against one another". | L15 | practice |
| Shopify: each unit must "survive two adversarial code reviewers, and get a human's nod before it's committed and the next one starts". | B21 | practice |
| Stripe: "we only have at most two rounds of CI". After the second push the branch goes to a human. | B3 | practice |
| Factory: "When validation surfaces issues, the orchestrator creates follow-up work to fix them before moving on". | L23 | practice |
| Anthropic ran 5 to 15 generator-evaluator iterations, then moved to one evaluator pass at the end. Cursor removed its judge. | L7, L21 | practice |
| In teamwire's runs, 100 reprompts went to 32 of 57 teams. About 79 carry reviewer, closer, lead-review or bot findings. | O1 | internal |

The closer's note D2: the old skill said "Sequential, never a loop". The
preset `implement-then-review.json` already allowed at most two fix rounds. So
the skill contradicted its own preset. The change is a wording fix.

### 2.6 Cost

| Source | Compared against | Multiple | Id |
| --- | --- | --- | --- |
| Anthropic research system | one chat | agents about 4×, multi-agent about 15× | S1 |
| Google scaling study | one agent at matched performance, four non-coding benchmarks | 1.6-6.2× | S4 |
| Claude Code cost docs | one standard session, teammates in plan mode | about 7× | L9 |
| Factory Missions | one normal session, at the median | 12× (9× at p99) | L23 |
| Anthropic harness post | one solo run of the same app | $200 against $9, over 20× | L7 |
| "How Do AI Agents Spend Your Money?" | two runs of the same task | up to 30× | S19 |

The old skill quoted the chat comparison (15×). A lead who chooses between a
team and one agent needs the comparison against one agent session. L9 is the
closest source but states no method. S4 is the only measured source, and none
of its benchmarks is coding. Corrected: the member's quote for Factory was
reworded. The page says "At the median, a mission consumes 12x more tokens than
a normal session. At p99, the gap is 9x."

Other cost facts the closer confirmed:

- Uber: cost per session fell 52% from its June peak, with the model held
  constant. "The subagent default setting has proven to be the most impactful
  lever." (B16)
- Bun: around $165,000 for about 50 workflows over 11 days. (B18)
- The C compiler: just under $20,000 over nearly 2,000 sessions. (L5)
- Anthropic's docs: "Use Sonnet for teammates." (L9)

### 2.7 What teamwire's own 57 runs show

The closer re-ran these counts over all 57 `team.db` files. The corpus has 57
teams and 280 members (138 implementers, 60 reviewers, 62 investigators, 20
closers). It holds 409 completions (404 done, 5 blocked), 280 messages and 523
verify records. No member failed.

| Finding | Number | Id |
| --- | --- | --- |
| Reprompts | 100 in 32 teams. 97 went to implementers. | O1 |
| Fix round for reviewed implementers | 54% (68 of 125), counting only reprompts with findings | O2 |
| Tests that cannot fail | found by closers in at least 6 runs | O4a |
| Questions | 38 asked, 33 real. 18 to 22 asked for an edit in a file outside the member's own `owns`. 32 were answered. Median wait 0.35 minutes. | O7 |
| `verify` flags | 426 outside-owns flags in 21 teams (signin 286, mfix 42, writerfix 23). 236 report-no-line flags in 13 teams. 188 weakened flags in 14 teams. | O9 |
| Time to first completion | implementer median 5.8 minutes (p90 20.0, max 82.1), reviewer 9.0, closer 11.4 | O15 |

Notes on O9. The closer re-counted the flags exactly. The closer did not
re-check the tracing of flags to lead merges. The count of notes that say the
lead committed depends on the phrase searched. It runs from 22 to about 47.

Notes on O15. The closer fixed three of the member's numbers. The watch printed
`WATCHING` 176 times and `TEAM-COMPLETE` 161 times. That is about 2.8 per team.
So the skill's line "a healthy team wakes you twice" is true for a single
wave only (D13).

Findings from the archive that the closer did not check: O3, O4b-d, O5, O8,
O10-O14, O16 and O17.

## 3. The disagreements the closer found

The closer's section 3 lists 14. These matter most.

| Id | The disagreement | What the closer decided |
| --- | --- | --- |
| D1 | How often a reviewer is wrong. Four sources measure four different things. | S11 is the only source on "correct work flagged as wrong". Its task is narrower than a teamwire review. The old sentence claimed more than any source measures. |
| D2 | Whether review should loop. The skill said "never a loop"; the preset allowed two fix rounds. | The skill contradicted its preset. The fix is wording. |
| D3 | The token multiple. Sources compare against a chat, a session, an agent or a solo run. | Use the multiple against one agent session. |
| D4 | Spotify's judge: Spotify's own post against a third-party report of its removal. | No conflict. The post gives the numbers. The report gives the removal. |
| D5 | The 17.2× error figure. The paper's own test finds no effect (p=0.441), and the factual error rate rose only 4.6%. | Quote "4.4× against 17.2×" as a central checker containing errors better. Never quote "errors grow 17-fold". |
| D6 | `git stash`. Bigco said no rule exists; ownruns said the member contract has it. | The member contract has it. The lead skill does not. Neither bans `git reset`. |
| D7 | "The new test fails on the old code". The contract already asks for it. | Six runs show the rule alone was not enough. The missing piece is the closer's revert-and-rerun step. |
| D8 | A check for work the brief did not ask for. | The reviewer contract has it. Only teams without a reviewer lack it. |
| D9 | Steering a running member. L8 says redirect; L16 says it hurts. | Neither side has a measurement. |
| D10 | S7, S8 and S9 share one dataset. | Count them as one sample. |
| D11 | Studies against labs on the Opus-lead sentence. | Labs was right. No source supports the effort levels per role. |
| D12 | O6's cure against its own evidence. | A fixed string lost to the session's attribution instruction. |
| D13 | "A healthy team wakes you twice" against 2.8 `TEAM-COMPLETE` lines per team. | True for a single wave only. |
| D14 | Task size (S6) against diff size (S7). | Both point the same way. |

## 4. What changed

The change list has items 1-16 and 20. The user approved them on 2026-10-18.
Items 17-19 need no text change. SKILL is `examples/skills/teamwire/SKILL.md`.
SEATS is `crates/teamwire-launch/contracts/`. PRESETS is `examples/presets/`.

| # | Change | Evidence it rests on | Files changed |
| --- | --- | --- | --- |
| 1 | The closer proves tests can fail by reverting the fix in a temporary clone that the closer makes. The closer runs the member's `setup` in the clone first (A4). The closer runs each member's check on the member's branch and on the joined head. The reviewer looks for stubbed, empty or hard-coded code. The sources of the weakening and style claims are corrected (section 5). | O4a, B8, B19 | SEATS `kind_closer.md`, `kind_reviewer.md`, `kind_implementer.md`; PRESETS `closer.json`; SKILL closer row and integrate step |
| 2 | One review, then at most two fix rounds by the owner. A fix round is the normal case. The round after a pull request opens goes to the owner as one more reprompt. "Cap retries at two" is about a member that failed the same task twice. | O2, L15, B21, B3, L23 | SKILL |
| 3 | The brief gains "Done when", "Out of scope" and "When to do nothing". The brief states the end state, not the steps. | L4, L10, L24, L29, B7 | SKILL; PRESETS coding presets |
| 4 | Before launch, the lead finds files nobody owns and puts each in one member's `owns`. Members get no standing permission to edit other files. | O7. O8 is not checked by the closer. | SKILL |
| 5 | The reviewer error sentence states what S11 measured. The lead never asks a reviewer for fixes in the same pass. The triage step stays. | S11, L10 | SKILL; SEATS `kind_reviewer.md` |
| 6 | Cost is stated against one agent session: about 7× for a team, 12× to 22× for long orchestrated runs. | L9, L23, L7 | SKILL |
| 7 | Before launch, the lead compares each `check` with the repository's CI job list. | O4b: a check narrower than CI or the real gate showed up in six runs (not checked by the closer) | SKILL |
| 8 | A replacement's brief names the failed member's branch, last diff and failing output. | S18: 66.6% to 71.8% with the earlier diff, 66.8% for a cold restart, one model | SKILL |
| 9 | The sizing rules carry numbers: 48% for a one-file fix under five lines, under 10% for three or more files or over 100 lines, 3-5 teammates, 5-6 tasks each. The 5-6 counts items on a teammate's shared task list, not units in one brief (A6). | S6, L16, L8 | SKILL; PRESETS `parallel-tickets.json`, `dependent-tickets.json` |
| 10 | The reason for writing tests first is stated as measured: 25% of seeded faults against 14%. | S12 | SKILL; PRESETS `contract-split.json` |
| 11 | A lead commit on a member's branch raises `report-no-line:<sha>`, and `outside-owns` for files outside `owns`. A lead merge never raises `report-no-line`. A merge raises `outside-owns` only when the member's `base` does not name the merged branch (`crates/teamwire-verify/src/git.rs:92-112,188-195`). The lead records the waiver in the same turn, asks the member for a report line, and sets `base` on stacked branches. | O9 | SKILL |
| 12 | Policy rules get one worked example each, settled with the user before launch. The brief does not restate a convention that the harness also sets, such as the commit trailer. | O6, corrected | SKILL |
| 13 | Before planning a team, the lead asks whether a codemod or a script makes the change. | B15, B2 | SKILL |
| 14 | Build and test commands and directory rules go in the repository's instruction files. Each new member failure adds one rule there. | B19, B4 (B4 not checked by the closer) | SKILL |
| 15 | Each `check` runs once in a fresh worktree with the spec's `setup`. The implementer rule on `git stash` extends to `git reset --hard` and `git checkout -- .`. | O8 (not checked by the closer), B18 | SKILL; SEATS `kind_implementer.md`; `contract/tests.rs` if the pinned phrase changes |
| 16 | One sentence says agents may read, research and review in parallel, and one agent writes each piece of code. | L15, L3, S22 | SKILL |
| 20 | At the turn end that hands over a member's completion, the member's Stop hook adds up the tokens in the transcript. `teamwire metrics` reports them per member under `teamwire.member.tokens`. The skill tells the lead to read `teamwire metrics <team>` before close. A closed team's counts stay readable with `teamwire metrics` and no team name (A5). | O17 (not checked by the closer), B16 | The token-recording code; SKILL |

Amendments after review, on 2026-10-18:

- A1 (item 1). The closer runs the revert step in a temporary clone that the
  closer makes itself with `git clone`. The closer never uses `git worktree add`
  and never touches the lead's checkout or a member's worktree. The closer
  deletes the clone afterwards. "Each high finding's fix" means the fix behind
  each finding graded wrong conclusion, and each new test a member claims.
- A2 (item 20). A team whose lead route is the watch records no usage. Section 6
  gives the reason.
- A3 (item 5). The member rules name the source for "a reviewer asked to
  propose fixes rejects more correct work": Jin and Chen, 2026 (S11).
- A4 (item 1). Before the revert, the closer runs the member's `setup` command in
  the clone, with the clone's path as its first argument. That install is an
  allowed exception to the no-installs rule. The closer may copy an existing
  `target/` or `node_modules` folder into the clone instead when that is faster.
- A5 (item 20). A closed team's token counts stay readable. `teamwire metrics`
  with no team name reads `$TEAMWIRE_HOME/archive/`. `teamwire metrics <team>`
  reads only a team that still has its state directory.
- A6 (item 9). Anthropic's "5-6 tasks each" counts items on a teammate's shared
  task list, not units in one brief.

Corrected evidence behind item 15: Bun ran 4 worktrees with 16 Claudes each. The
member's claim of "64 worktrees" was wrong. The `git stash` collision happened
in one shared checkout. Each teamwire implementer has its own worktree.

## 5. Claims removed for lack of a source

The closer found three claims in lead-owned text with no source.

| Claim | Where | What the closer found | What replaced it |
| --- | --- | --- | --- |
| "Spotify and Google both found agents doing exactly this" (weakening tests) | `kind_implementer.md` | Spotify holds (B8, B9). Neither Google paper mentions agents deleting, disabling or commenting out tests. | "Spotify found" |
| "Uber and LinkedIn both found reviewers' style comments are the ones engineers reject." | `kind_reviewer.md` | Uber holds (B14). No LinkedIn source exists. | "Uber found" |
| "Nine branches once merged with zero conflicts and the gate still failed." | SKILL | The line came in with the skill's first commit on 2026-09-30, before any archived run. No source holds the case. | Dropped. S10 shows that the effect exists but is rare on real pull request pairs. |

## 6. Follow-up work and gaps

Nobody found these.

- **Effort levels per role.** No source supports the effort levels in the
  skill (D11). Model routing has support (L2, B16).
  A measurement of effort needs five things:
  - Effort on every token count: the `effort` label on the token metric, or a
    join on `member.started`.
  - A task-matched comparison. The lead picks effort by hand and gives higher
    effort to harder tasks, so a plain comparison of high against medium
    measures task difficulty. It needs the same kind of member on a comparable
    task, with effort assigned at random or alternated by a fixed rule.
  - An outcome that does not come from the same agent. The store records
    completion status, reprompts, first-try results, verify flags and run
    duration. It does not record whether a reviewer's finding was true or
    whether a test could fail. Reviewers and closers need a seeded defect set
    or closer grades written as data.
  - Enough members. Only 18 members have effort recorded. A rough floor is a
    few dozen members per kind and effort level.
  - A cost measure: tokens priced by model, plus wall time
    (`teamwire.member.run.duration`), because higher effort can hit
    `deadline_s`.
- **A measured review limit.** No source measured a loop against a single review
  pass, or found where extra fix rounds stop paying (D2).
- **Attempt 3 against attempt 2.** The sources give a cap of two (B3, L10). None
  measured whether a third attempt helps.
- **OpenAI's "Harness engineering" post.** The labs member got HTTP 403 and the
  closer did not retry. Secondary summaries (InfoQ, Pragmatic Engineer) were
  read but not graded here.

Gaps in teamwire's own data:

- Subagent tokens are not counted. Item 20 adds the tokens in each member's own
  transcript. Tokens spent by subagents that a member starts are outside it.
- The 57 archived runs have no model or effort record (O17, not checked by
  the closer). Launch has recorded `model` and `effort` in each member's
  `member.started` event data since 2026-10-17. Teams launched from that day
  have both, so item 20 adds only token counts.
- A team whose lead route is the watch (host `none`) records no token usage
  (amendment A2). That route writes no `result.delivering` event, and a new
  event needs a schema change. The limit is accepted and documented.
- "Replace rather than rescue" has no instance in 57 runs (O16, not checked by
  the closer). Item 8 therefore rests on S18 and practice, not on the archive.

The closer's unchecked findings:

- S14, S15, S20; L6, L12-L14, L17-L19, L22, L25, L26, L28; B1, B4-B6, B10,
  B12, B13, B17, B20, B22-B25.
- Items 3, 13 and 14 rest partly on B4, B10, B12 and B13, which the closer
  did not check.
- S4's affiliations and S5's section numbers.
- O3, O4b-d, O5, O8, O10-O14, O16 and O17. The closer re-ran only the counts the
  brief named, plus the corpus totals.
- The tracing of verify flags to lead merges (second half of O9).
- Ownruns' "27 reviewer findings, about 19 to 21 forwarded". D1 repeats it, and
  the closer did not re-count it.

## 7. Sources

The closer confirmed the quotes taken from these pages. The closer's report names
pages, not URLs, so the URLs come from the member reports. Dates for B7, B8, B9
and B14 also come from `bigco.md`, not from the closer. Other dates are the page
date. "Undated" means the page prints none. The Anthropic docs pages were read on
2026-10-18.

| Id | Publisher and title | Date | URL |
| --- | --- | --- | --- |
| S1, S2, L1-L4 | Anthropic Engineering, "How we built our multi-agent research system" | 2025-06-13 | https://www.anthropic.com/engineering/multi-agent-research-system |
| L5 | Anthropic Engineering, "Building a C compiler with a team of parallel Claudes" | 2026-02-05 | https://www.anthropic.com/engineering/building-c-compiler |
| L7 | Anthropic Engineering, "Harness design for long-running application development" | 2026-03-24 | https://www.anthropic.com/engineering/harness-design-long-running-apps |
| L8 | Anthropic, Claude Code docs, "Orchestrate teams of Claude Code sessions" | undated | https://code.claude.com/docs/en/agent-teams |
| L9 | Anthropic, Claude Code docs, "Manage costs effectively" | undated | https://code.claude.com/docs/en/costs |
| L10 | Anthropic, Claude Code docs, "Best practices for Claude Code" | undated | https://code.claude.com/docs/en/best-practices |
| L11 | Anthropic, Claude blog, "Code Review for Claude Code" | 2026-03-09 | https://claude.com/blog/code-review |
| L15 | Cognition, "Multi-Agents: What's Actually Working" | 2026-04-22 | https://cognition.com/blog/multi-agents-working |
| L16 | Cognition, "Devin's 2025 Performance Review" | 2025-11-14 | https://cognition.com/blog/devin-annual-performance-review-2025 |
| L20 | Cursor, "Scaling long-running autonomous coding" | 2026-01-14 | https://cursor.com/blog/scaling-agents |
| L21 | Cursor, "Towards self-driving codebases" | 2026-02-05 | https://cursor.com/blog/self-driving-codebases |
| L23 | Factory, "Introducing Missions" (page date 2025-02-26, modified 2026-10-14; the figures are from 2026) | 2026-10-14 | https://factory.com/news/missions |
| L24 | OpenAI, Codex docs, "Best practices" | undated | https://learn.chatgpt.com/guides/best-practices |
| L29 | GitHub Blog, "WRAP up your backlog with GitHub Copilot coding agent"; GitHub Docs, "Get the best results" | 2025-12-26 | https://github.blog/ai-and-ml/github-copilot/wrap-up-your-backlog-with-github-copilot-coding-agent/ and https://docs.github.com/en/copilot/tutorials/cloud-agent/get-the-best-results |
| S22, L27 | Google Research, "Towards a science of scaling agent systems" | 2026-01-28 | https://research.google/blog/towards-a-science-of-scaling-agent-systems-when-and-why-agent-systems-work/ |
| B3, B1 | Stripe, "Minions: Stripe's one-shot, end-to-end coding agents", Part 1 | 2026-02-09 | https://stripe.dev/blog/minions-stripes-one-shot-end-to-end-coding-agents |
| B3, B2 | Stripe, Minions Part 2 | 2026-02-19 | https://stripe.dev/blog/minions-stripes-one-shot-end-to-end-coding-agents-part-2 |
| B7 | Spotify, "Context Engineering (Honk, Part 2)" | 2025-11-24 | https://engineering.atspotify.com/2025/11/context-engineering-background-coding-agents-part-2 |
| B8 | Spotify, "Predictable Results Through Strong Feedback Loops (Honk, Part 3)" | 2025-12-09 | https://engineering.atspotify.com/2025/12/feedback-loops-background-coding-agents-part-3 |
| B9 | InfoQ, report on Spotify's QCon London 2026 talk (third-party) | 2026-03-18 | https://www.infoq.com/news/2026/03/spotify-honk-rewrite |
| B11 | Google, "How is Google using AI for internal code migrations?" (arXiv 2501.06972) | 2025-01-12 | https://arxiv.org/html/2501.06972v1 |
| B14 | Uber, "uReview" (needs browser headers; the `/us/en/blog/ureview/` path loads) | 2025-08-12 | https://www.uber.com/us/en/blog/ureview/ |
| B15 | Uber, "Migrating JUnit at massive scale" (needs browser headers) | 2026-04-07 | https://www.uber.com/eg/en/blog/junit-migration/ |
| B16 | Uber, "Running a Software Factory Efficiently at Uber Scale" | 2026-08-27 | https://www.uber.com/us/en/blog/efficient-software-factory/ |
| B18, B19 | Bun, "Rewriting Bun in Rust" | 2026-07-08 | https://bun.com/blog/bun-in-rust |
| B21 | Shopify Engineering, "Back to native" | 2026-09-10 | https://shopify.engineering/back-to-native |
| S3 | Cemri et al., "Why Do Multi-Agent LLM Systems Fail?" (arXiv 2503.13657, v3) | 2025-10-26 | https://arxiv.org/html/2503.13657v3 |
| S4, S5 | Kim et al., "Towards a Science of Scaling Agent Systems" (arXiv 2512.08296, v2 holds the quotes) | 2025 | https://arxiv.org/html/2512.08296v2 |
| S6 | Zhang, He, Zhang et al. (Microsoft), "SWE-bench Goes Live!" (arXiv 2505.23419) | 2025 | https://arxiv.org/html/2505.23419 |
| S7 | Ehsani et al., "Where Do AI Coding Agents Fail?" (arXiv 2601.15195) | 2026-01 | https://arxiv.org/html/2601.15195v1 |
| S8 | Pinna et al., task-level evaluation of agent pull requests (arXiv 2602.08915) | 2026-02 | https://arxiv.org/html/2602.08915v1 |
| S9 | Xu et al., "AI Agent Pull Requests on GitHub" (arXiv 2607.04697) | 2026-07 | https://arxiv.org/abs/2607.04697 |
| S10 | Xia, Wu, Park, "Passes Alone, Fails Together" (arXiv 2609.25396) | 2026 | https://arxiv.org/abs/2609.25396 |
| S11 | Jin and Chen, "Are LLMs Reliable Code Reviewers?" (arXiv 2603.00539) | 2026-03 | https://arxiv.org/abs/2603.00539 |
| S12 | Konstantinou, Tambon, Papadakis, "On the risk of coding before testing" (arXiv 2607.05139) | 2026-07-06 | https://arxiv.org/pdf/2607.05139 |
| S13 | Becker et al. (METR), early-2025 developer productivity study (arXiv 2507.09089) | 2025 | https://arxiv.org/abs/2507.09089 |
| S16 | Brown et al., "Large Language Monkeys" (arXiv 2407.21787) | 2024 | https://arxiv.org/abs/2407.21787 |
| S17 | Yang et al., "SWE-smith" (arXiv 2504.21798) | 2025-04 | https://arxiv.org/html/2504.21798 |
| S18 | Wang et al., "Fail-Fast, Restart-Smart" (arXiv 2608.03222) | 2026-08 | https://arxiv.org/abs/2608.03222 |
| S19 | "How Do AI Agents Spend Your Money?" (arXiv 2604.22750) | 2026-04 | https://arxiv.org/abs/2604.22750 |
| S21 | "An Empirical Study of Harness Design for Coding Agents" (arXiv 2609.20804) | 2026 | https://arxiv.org/abs/2609.20804 |

## 8. Where the full reports are

The four investigators' reports, the closer's report, the briefs and the
message log are archived at `~/.teamwire/archive/leadres-2026-10-18/`. The
reports are in `reports/`: `labs.md`, `bigco.md`, `studies.md`, `ownruns.md`
and `closer.md`. The closer's report holds the full verification table, with
the exact text found for each quote, and the merged list of 20 recommendations.
The change list that this record's section 4 follows is the lead's plan for
team `leadfix`.
