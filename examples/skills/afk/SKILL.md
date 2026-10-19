---
name: afk
description: Run a teamwire team on one task while the user is away, with no questions asked until the report is written. Use when the user types /teamwire:afk <task>, or says they will be away and want the result when they return.
---

# AFK runs with teamwire

You lead a teamwire team on one task, and nobody will answer you until the
run ends. Read the teamwire skill first. This file lists only what changes,
and where the two disagree, this file wins.

## The one rule

Never wait for a person. When you would ask the user something, answer it
yourself with the answer you would have recommended, write it into the
decision log, and carry on. A turn that ends on a question to the user ends
the run, because nothing will wake you again. Step 1 is the only place you
may stop for the user.

## 1. Preflight, while the user may still be at the keyboard

1. Check that your context says bypass permissions mode or auto mode is
   active. If it does not, tell the user to restart the session with
   `claude --permission-mode bypassPermissions`, and stop.
2. Write the user's task, word for word, to a file. Pick a team name and a
   title from the task. Run
   `teamwire afk start --team <name> --title <title> --task <file>`, with
   `--budget` only when the user named a time limit. When it exits non-zero,
   tell the user what failed and stop.
3. Reply once with the team name, the report path, the time the budget
   ends, and: "Keep the lid open and the machine on power." Then start step 2
   in the same turn. Do not wait for an answer.

The run is this one team. Every member you add later joins it with
`teamwire launch --add`. Never start a second team for this run.

## 2. What every run must have

You choose the team for each task. These five things hold in every run, and
nothing in the task changes them:

1. You never wait for a person (the one rule above).
2. You protect your own context (section 3).
3. Once code exists, the `closer` runs. It runs every claimed check and the
   repository's full gate on each branch and on the joined branch, and
   records each exit code. It is the only member that turns the team's
   claims into evidence before the user reads the report.
4. A `reporter` writes `afk/REPORT.md` (section 7). Close refuses the team
   until the report has every heading.
5. The boundaries hold: never push, open a pull request, merge into the base
   branch, delete a branch, or run `teamwire install` or `cargo install`.
   Never act on a team this session does not lead.

Everything else is yours to decide: which members to add, how many, and in
what order.

## 3. Your context is the run's scarcest resource

You decide. Members research, plan, review, verify and write. Every file you
read stays in your context for the rest of the run, and a lead with a full
context decides worse. So:

- Give research, planning, review, verification and report-writing to
  members. Do one yourself only when it is smaller than writing its brief,
  and never reading more than one or two short files.
- Pass work by path. A brief or a reprompt names files. Never paste a
  report, a diff or a log into a message.
- Read a member's result line and the summary at the top of its report. Read
  further only when two members contradict each other, and then only the
  lines in dispute.
- Never read a raw diff, check output or a gate log. The closer reads those.
- Members read each other's files by path. A planner reads the
  investigators' reports, a plan reviewer reads the plan, and each owner
  reads its own findings.

Each member you add runs once and finishes. None of them sends work to
another member. You make every call that changes the plan.

## 4. Members you can add

These profiles are a reference, not a sequence. Pick the ones the task
needs. When you are unsure whether a member pays for itself, add it. One
more member costs little. A misread task with nobody watching costs the
whole run.

| Member | Kind, model, effort | Use it when | It writes |
| --- | --- | --- | --- |
| analyst | investigator, opus, high | The task leaves room for more than one reading, or touches code you have not read. | `afk/understanding.md`, at most one page: the end state, the inferred acceptance criteria, what is out of scope, each ambiguity with the reading it chose, and the questions that decide the plan, grouped by area |
| scout | investigator, opus, high | An area has questions that decide the plan and nobody has answered them. One scout per area, all in one `--add`. Keep scouts open until the build ends, so implementers can ask them directly. | its report: each answer with file:line |
| planner | investigator, opus, xhigh | The work has more than one unit, open questions, or files that others read or pin. | `afk/plan.md` with a summary of at most 30 lines on top; `afk/decisions.md`; `afk/briefs/<role>.md` per implementer in the main skill's 14 sections; `afk/spec-wave<n>.json`; the result of each item of the main skill's planning check, using a temporary `git clone` where a fresh worktree is needed |
| plan reviewer | reviewer, opus, xhigh | A plan exists and a wrong plan would waste more than one implementer's work. Fix the plan once from its findings, by reprompting whoever wrote it. Never review the same plan twice. | ranked gaps: a misread of the task, a criterion nobody can check, a file nobody owns, a check that misses what CI runs, a wrong shape |
| implementer | implementer, opus, high | Code must change. | its branch |
| reviewer | reviewer, opus, xhigh | A branch is finished. One per branch by default. Skip it only for a change so small that the closer's coverage check reads the whole diff anyway. | ranked findings |
| closer | closer, opus, xhigh | Always, once code exists (section 2). | the integrated report and the exit codes |
| reporter | investigator, opus, high | Always, at the end (section 7). | `afk/REPORT.md` |

Use `max` only for one member whose task is hard to reason about, such as a
race or a data-loss path. Set every `deadline_s` to 1.5 times what you would
set otherwise. Write each brief to `afk/briefs/<role>.md` in the team
directory. Tell every member that writes files outside its report file to
put them under `afk/` in the team directory, never in the repository.

Whoever settles an open question, a planner or you, writes it to
`afk/decisions.md` in this form:

    D3  2026-10-19 14:05  Which column marks an AFK team?
    Chose: afk_budget_ends_at on the team row.
    Why: one nullable column says both that the team is AFK and when its budget ends.
    Reverse by: a schema migration that drops the column; only launch, status, close and the Stop hook read it.

Each answer goes into the affected briefs under "Decisions already made". A
brief has no "Open questions you must not answer" for work it must deliver.
When a question cannot be settled from the task, the code or the docs, such
as a credential nobody has or a product choice the task gives no hint of,
cut the units that depend on it and list them as not done.

## 5. Write down your roster before the first launch

Before you add the first member, write entry `D1` in `afk/decisions.md`: the
members you plan to add, the profiles you are skipping, and one line on why
for each. When you change the roster later, add an entry. The reporter puts
this in the report, so the user can see which checks the run skipped.

## 6. While the team works

- Answer a member's question in the turn it arrives. When the plan answers
  it, point at the line. When a scout knows the area, tell the member to ask
  that scout. Otherwise decide, and add a `D` entry. Never leave a question
  `UNANSWERED`.
- Send review findings to the branch's owner by path, all in one turn. Do
  not sort the findings yourself: the owner checks each one against the
  code, fixes what holds, and answers the rest with evidence. Allow at most
  two fix rounds per owner. Never run the closer twice. Run
  `teamwire verify <team> --role <owner>` after each fix round.
- Make the integration branch `afk/<team>` off the base branch, and merge in
  the plan's order. On a conflict, abort that merge. Reprompt the later
  branch's owner to merge the earlier branch into its own branch and resolve
  it, then merge again. Do not resolve conflicts yourself.
- `TEAM-STALLED` is yours to handle. Run `teamwire status`, read the pane,
  and replace the member once. If the replacement stalls too, cut the unit
  and report it.
- `teamwire status <team>` shows the time left in the budget. Launch the
  last implementer no later than two thirds of the way through it, so the
  review, the closer and the report still fit. Past the budget, launch
  refuses new implementers. Finish what is running, verify it, and have the
  reporter list the rest as not done.

## 7. Report and close

Add the `reporter` with the paths of whichever of these exist: `afk/task.md`,
`afk/understanding.md`, `afk/plan.md`, `afk/decisions.md`, the closer's
report, and the team's `reports/` directory. It runs `teamwire metrics <team>` for the cost, and
gathers every member's **Decisions I made** list. It writes `afk/REPORT.md`
in the team directory. It writes each heading below as a `##` heading, with
this wording, in this order:

1. **What you asked**: the task, quoted.
2. **What I understood**: the end state from `understanding.md`.
3. **The result**: the branch, its base commit, the commits, and how to try
   it.
4. **Verified**: each command, where it ran, and its exit code.
5. **Decisions made for you**: the ones that most change the result, with a
   pointer to `decisions.md`.
6. **Not done, and why**.
7. **Risks**: what you could not check, and what the reviewer or closer
   flagged that is still open.
8. **Cost**: wall time, and tokens per member.

Run `teamwire close <team>`. Close refuses until the report has every
heading, archives the team with the `afk/` files, keeps the branches and
releases the sleep hold. When close refuses, reprompt the reporter with the
missing headings. Then call `PushNotification` with the archived report's
path if you have that tool.

A run always ends with a report. When something fails that nobody can fix,
have the reporter write up what exists. A partial result with an honest
report is the goal. A session waiting for a person is the failure.
