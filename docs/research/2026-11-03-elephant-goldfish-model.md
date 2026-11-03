# Research: the Elephant-Goldfish Model and teamwire teams

Written 2026-11-03, revised 2026-11-04 after the `models` team's work was
merged (ADR 0033). This note is exploration. One decision followed from it:
ADR 0034 adds a member kind, `plan_reviewer`, so a plan review needs none of
the workarounds in sections 5 and 7.

The question: Google has published a working method for AI-assisted design,
the Elephant-Goldfish Model (EGM), with a plugin for coding harnesses. Which
of its ideas should teamwire use when a lead plans and runs a team?

## 1. Sources

Every source below was read on 2026-11-03.

| Source | What it is |
| --- | --- |
| [Google Open Source Blog, 2026-10-22](https://opensource.googleblog.com/2026/10/introducing-elephant-goldfish-model-plugin.html) | The announcement of the open-source plugin. |
| [`google/elephant-goldfish-model`](https://github.com/google/elephant-goldfish-model) | The plugin: 7 skills, 9 reviewer prompts, 5 review standards, and design docs. Apache 2.0. The files cited below are paths in this repository. |
| [Dave Rensin, "Elephants, Goldfish, and the New Golden Age of Software Engineering"](https://medium.com/@drensin/elephants-goldfish-and-the-new-golden-age-of-software-engineering-c33641a48874) | The original essay, April 2026. Medium blocks scripts, so the text was read from the author's RSS feed (`https://medium.com/feed/@drensin`). |

The blog says thousands of Google engineers use the plugin every day. No
source gives a measured gain. The plugin's own FAQ (`docs/faq.md`) says that
finishing the workflow "does not demonstrate a productivity improvement".

## 2. What the model says

EGM splits the work between two kinds of AI session:

- **The Elephant** is one long session with a lot of context. The human
  argues the design out with the Elephant, and the Elephant writes no code.
  The Elephant is told to challenge the human, not agree. Its output is a
  design document saved in the repository.
- **A Goldfish** is a fresh session that has never seen that conversation.
  A Goldfish reads only the design document and the files the document links
  to. A Goldfish does not share the Elephant's blind spots, because it never
  saw the discussion that produced them.

Rensin's main claim is "design is the new code". A human who writes code
makes many small decisions while typing. An AI that writes the code makes
those decisions instead, and nobody reviews them. So every such decision has
to be written into the design document before coding starts. A second claim
supports this: design documents are much smaller than the code they describe.
A fresh session gets up to speed faster and more reliably from the documents.

The essay gives the method as a sequence of prompts:

1. Load the system context into a new session, and correct its first summary.
2. Hold a design discussion with a "no code" rule for 20 to 30 minutes. Push
   the model back into critic mode when it starts to agree too readily.
3. Ask the model to propose the first design, so its gaps in understanding
   show.
4. Write the design doc one section at a time: the problem, the technical
   plan, the alternatives that were rejected, and a detailed plan that lists
   every file to create or change.
5. Give the doc to a fresh session and ask it to explain the system back.
   Rensin calls this the comprehension test.
6. Give the doc to another fresh session acting as a critic. Rensin estimates
   that about 30% of its suggestions are valuable.
7. Give the doc to another fresh session and ask whether it could implement
   the feature on its first pass. Rensin calls this the readiness test.
8. Implement from the doc. When a session fails, a new session reads the doc
   and continues.
9. Ask a fresh session to tear the finished code apart, and repeat until its
   complaints are trivial.

### 2.1 The plugin

The plugin turns the method into skills:

| Skill | What it does |
| --- | --- |
| `egm-brainstorm` | The Elephant asks the human questions over many turns. Then six fresh reviewers each read the idea from one angle: simplicity, cost, ergonomics, migration, observability and long-term maintenance. |
| `egm-draft` | The Elephant writes the design doc one section at a time and stops after each section for the human's comments. |
| `egm-review` | Five review stages, each run by fresh reviewers (section 2.2). |
| `egm-resolve` | The Elephant sorts each review finding into one of four groups (section 2.3). |
| `egm-review-loop` | Runs a chosen set of reviewers in parallel on one part of a document. |
| `egm-drift-report` | Compares the design docs with the code and lists where they no longer match. The plugin suggests running it weekly. |
| `egm-bootstrap` | Writes a `README.md` in every directory, from the leaves up, so a fresh session can learn a large codebase from the READMEs. |

### 2.2 The five review stages

`skills/egm-review/SKILL.md` runs these stages in order:

| Stage | The fresh reviewer checks | Standard it applies |
| --- | --- | --- |
| Zoom out | That the design solves the root problem in the simplest way and reuses what exists. | `zoom-out-standard.md` |
| Comprehension | That a newcomer can follow the document. The reviewer first writes what the document proposes in its own words. A wrong paraphrase counts as a finding. | none |
| Critic | Gaps, faulty assumptions, edge cases and failure modes. Three reviewers run here: design, structure and style. | `feature-spec-standard.md` |
| Second zoom out | That the fixes from the two stages before did not add needless complexity. | `zoom-out-standard.md` |
| Readiness | That an engineer could implement the plan with no further questions. | `implementation-readiness-standard.md` |

Two rules from these files matter most for teamwire:

- **The zero-decision rule** (`implementation-readiness-standard.md`). A plan
  fails if the implementer must make any design choice, such as a schema, an
  API path, a dependency or the handling of an edge case.
- **The paraphrase test** (`agents/comprehension.md`). A list of undefined
  terms cannot catch a document whose terms are all defined but whose parts do
  not connect. A paraphrase written by a fresh reader does catch that.

### 2.3 How the Elephant answers findings

`skills/egm-resolve/SKILL.md` puts each finding in exactly one group:

| Group | When |
| --- | --- |
| PROPOSE FIX | The finding is valid. The Elephant writes the exact edit. |
| CONTEST | The finding is wrong, and the Elephant cites evidence the reviewer could have read: a file and a symbol, or a decision written in the document. |
| ESCALATE | The finding is wrong, but the evidence is outside every file the reviewer could read, such as a verbal decision. The human decides. |
| DECLINE | The finding is a nit. The Elephant records it and changes nothing. |

Two of its rules guard against a reviewer that invents problems. "This was
intentional" is not evidence. And the Elephant adds context only when a human
reader needs that context, never only to satisfy a reviewer.

## 3. Where teamwire already follows the model

- **Every member starts as a Goldfish.** A member's first context is its
  brief and nothing else. So every brief is tested by a fresh reader. The
  test happens after launch, while every member is already spending tokens.
- **The lead skill already separates planning from leading.** The section
  "Plan in one session, lead from a fresh one" has the lead write the plan
  and the brief to a file, then lead from a clean session.
- **teamwire enforces the list of files a member may change.** EGM's plan
  lists the files the implementer may write. EGM's own architecture doc
  (`docs/designs/architecture.md`) says "nothing enforces it". In teamwire,
  the `owns` list and `teamwire verify` enforce it.
- **teamwire's reviewers and closer are Goldfish reviewers of code.** They
  send no messages, and only their own lead may message them. The lead skill
  turns off claude-mem for them so that they stay independent.
- **Members already record the decisions a brief left open.** Every member
  lists them under **Decisions I made**. EGM's zero-decision rule says that
  list should be empty. So the length of the list measures how far a brief
  fell short of that rule.

## 4. Where the model adds something teamwire lacks

teamwire has no fresh review of the plan and the briefs before launch. Today
the lead runs the planning check on its own plan. In EGM's terms, the
Elephant reviews its own work, which is the failure EGM was built to prevent.
teamwire's own records say this is where the cost is:

- **The brief mattered more than the setup.** In
  `2026-10-26-gains-complex-task.md`, a solo run with a brief that left out
  two interfaces passed 147 of 191 hidden tests. With the complete brief,
  every setup passed between 177 and 182.
- **One lead added a Goldfish review without being told to.** In round 2 of
  the same study, the lead started a plan reviewer. The plan reviewer found 9
  gaps, and the planner reworked the plan over three sessions. Round 2's team
  was the only run graded 3/3/3 by the blind grader. Planning cost about $28
  in round 2, against about $10 in round 1. One run cannot show that the plan
  reviewer caused the better grade.
- **Member questions show what plans missed.** In
  `2026-10-18-lead-field-reports.md`, 18 to 22 of 33 real member questions
  asked to edit a file outside the member's `owns`. In five runs, members
  answered the same open question in different ways. The readiness test with
  the zero-decision rule targets both failures.

## 5. What does not carry over

- **Members cannot run the plugin as it is.** Every EGM skill sets
  `disable-model-invocation`. The skills stop at questions that wait for a
  human. They also write files such as `concepts_brief.md` into the working
  directory, outside any member's `owns`. A teamwire member runs unattended,
  so it would stop at the first question. The lead skill already forbids
  `grilling` in members for the same reason.
- **The cost is real, and nobody has measured the gain.** A team already
  costs 7 to 18 times a solo run in teamwire's studies. EGM's full review
  runs about 9 reviewers over five stages, and its combining prompt
  (`agents/synthesizer.md`) can run two copies of each. That could double the
  cost of planning.
- **EGM needs a human to decide.** The human approves the design and decides
  each ESCALATE finding. An AFK run (ADR 0025) has no human present. An AFK
  team would need a rule for which findings may stop a launch.
- **The reviewer contract does not fit a plan review.**
  `crates/teamwire-launch/contracts/kind_reviewer.md` tells a reviewer to
  read "the diff your brief names", and every finding must give a concrete
  failure. A plan review has no code diff, and its findings are gaps in a
  brief. Section 7 gives a way to run a trial without changing the contract.

## 6. Model choice after ADR 0033

ADR 0033 gives each member a `task_class` and has the lead pick each model
from evidence with `teamwire models --for <class>`. That ADR changes three
points.

**A plan reviewer's class is `judgement`.** The class table in the lead skill
defines `judgement` as judging other agents' work. A plan is the lead's
work. The lead picks the reviewer's model with
`teamwire models --for judgement`.

**A Goldfish on another model family is out of reach on this machine.** EGM
argues that a fresh reviewer helps because it does not share the author's
blind spots. A reviewer on another vendor's model might share even fewer.
No source tested that idea. On 2026-11-03, `teamwire models` from a pi
session listed 14 models, all Claude models through `claude-bridge`. The
user excludes `amazon-bedrock`, because the account's AWS policy denies
Bedrock. The user has no Codex subscription or API key. So every reviewer
on this machine runs a Claude model. A reviewer on a different Claude model
from the lead, such as Sonnet reviewing Opus's plan, is possible. Its blind
spots probably overlap more, because the models share a vendor and training
methods. That is also untested. Fresh context alone is EGM's actual claim,
and fresh context is available here.

**The outcome report would mix two jobs.** `crates/teamwire-otel/src/outcomes.rs`
groups members by harness, model, effort, class and kind. A plan reviewer
with kind `reviewer` and class `judgement` lands in the same row as code
reviewers. A later change could separate them, with a new class or a
team-level record that a plan review ran. That choice should wait until
trials show whether the plan review finds anything.

## 7. A trial with no code change

The steps below test the idea on real teams before teamwire changes.

1. The lead writes the plan and every brief, as today.
2. The lead commits the plan and the briefs on a branch of their own, such
   as `<team>/plan`. The reviewer contract then has a diff to read, and each
   finding can name a brief's file and line.
3. The lead launches one member with kind `reviewer`, mode `pane` and class
   `judgement`, before any implementer. The lead picks its model with
   `teamwire models --for judgement`.
4. The reviewer's task applies two EGM rules to each brief. First, write in
   two to five sentences what the brief asks for. Second, list every design
   choice the member would have to make. Each such choice is a finding. Its
   concrete failure is "two members could choose differently here", or "the
   member would need to edit a file outside its `owns`".
5. The lead sorts each finding into EGM's four groups (section 2.3), and
   fixes the briefs.
6. The lead launches the implementers from the fixed briefs.

To judge the trial, compare these numbers with past teams of a similar size:

| Number | Where it comes from |
| --- | --- |
| Entries in **Decisions I made**, per implementer | Each member's report |
| Questions from members, and how many ask to edit outside `owns` | `teamwire log` |
| Implementers that needed a fix round | The fix-round rate in `bench/mine.py` |
| Extra cost of the plan review | `teamwire metrics` for the reviewer |

A few trials cannot prove a gain. They can show whether the plan reviewer
finds gaps the lead fixes, and whether the members then make fewer
decisions of their own.

## 8. Later ideas

- **Drift reports.** `docs/design/` and `docs/adr/` are teamwire's Elephant
  memory. A check that the code still matches them could become one more
  thing the closer checks. This idea has less value until the trial in
  section 7 shows a gain.
- **A preset.** If trials show a gain, a `plan-review` preset could set up
  step 3 above, and the reviewer contract could name a plan as well as a
  code diff. Both live in files that ADR 0033 just rewrote, so any change
  starts from the merged versions.

## 9. Open questions

- Should a plan review run before every team, or only from a team size or
  task class up? EGM's FAQ says to use it only for changes that need design
  decisions, not for small clear fixes.
- In an AFK run, which findings may stop a launch with no human present?
- Is a plan reviewer on a different Claude model from the lead's worth its
  cost over one on the same model? Only a comparison on the bench tasks can
  tell.
