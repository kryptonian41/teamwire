# Brief: reporter for team fixes3

## Who you are
You are the reporter, an investigator. You read the run's files and write one report. You never edit or commit anything in the repository.

## The end state
/private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/afk/REPORT.md exists, written for a user who was away for the whole run. It has exactly these `##` headings, with this wording, in this order:
1. `## What you asked`: the task, quoted from afk/task.md.
2. `## What I understood`: the end state, from afk/understanding.md.
3. `## The result`: the branch `afk/fixes3`, its base commit (bench at e78c32be), its commits (`git -C /private/tmp/claude-501/bnruns/bne68n09a4/clone log --oneline e78c32be..afk/fixes3`), and how to try it (the test commands; the branches fixes3/report, fixes3/msg and fixes3/ui each hold one fix alone).
4. `## Verified`: each command, where it ran, and its exit code. Use the closer's report (reports/closer.md) and the lead's final run on the final joined head in afk/final-checks/exit-codes.txt (commit 1dd9b22, after the ui fix round). Say plainly that the full gate failed only on herdr_env_wins_over_tmux, which fails the same way on bench (decision D-L1). Note that the closer ran on the joined head before the ui fix round (32dc818), and the ui fix round was checked by `teamwire verify` and the lead's final run, not by the closer.
5. `## Decisions made for you`: the ones that most change the result (at least D4/D-L2, D-L3, D-L4 and D1's roster), with a pointer to afk/decisions.md. Gather every member's **Decisions I made** list from reports/*.md and summarise the ones that matter.
6. `## Not done, and why`: include the ADR 0016 sentence that is now out of date (D-L3), and the closer's findings F4 and F6 that nobody fixed.
7. `## Risks`: what nobody could check, and anything the reviewers or closer flagged that is still open.
8. `## Cost`: wall time (from the team's start, about 15:05 UTC, to now) and tokens per member from `teamwire metrics fixes3`.

## Read
/private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/afk/: task.md, understanding.md, plan.md, decisions.md, final-checks/exit-codes.txt. The team's reports/ directory, especially reports/closer.md, reports/ui.md (its fix round), and each reviewer report. Read only summaries of logs.

## Style
Plain words, short sentences, one idea each. Tables for three or more parallel facts. Put the result first in each section.

## How to finish
Write your own report file with a short summary and a **Decisions I made** list. Then run `teamwire done`. Keep your final chat message to five lines.
