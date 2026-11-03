# Brief: reporter — write the user's report for team closecache

You are the reporter (kind investigator) for team `closecache`. The user was away for the whole run
and reads only your report. Write it in plain words, for a strong engineer new to this code.

Read these files, all under /private/tmp/claude-501/bnruns/bngcwrebut/twhome/teams/closecache:
- afk/task.md (the user's task, word for word)
- afk/decisions.md (D1 to D9; D1 is the roster)
- afk/briefs/ (impl.md, review.md, closer.md)
- reports/impl.md, reports/review.md, reports/closer.md
- afk/final-checks/exit-codes.md (the lead's checks on the final commit cf45e6a)
Run `teamwire metrics closecache` for the tokens per member, and `git -C /private/tmp/claude-501/bnruns/bngcwrebut/clone log --oneline bench..afk/closecache` for the commits.
There is no afk/understanding.md and no afk/plan.md: the run had no analyst or planner (D1).

Facts the reports do not hold:
- The closer checked commit 024f865. Fix round 2 (commit cf45e6a) came after the closer, for its
  finding F1. Nobody re-ran the closer on cf45e6a. The lead re-ran the checks listed in
  afk/final-checks/exit-codes.md on cf45e6a, and `teamwire verify closecache --role impl` passed
  there. The full gate (scripts/gate.sh without fast) and scripts/test-changed.sh were last run by
  the closer, on 024f865. Say this plainly under Verified and Risks.
- The finished work is on branch `afk/closecache` (same commit as closecache/impl), based on
  `bench` at f1ac22c. Nothing was pushed or merged into bench.

Write afk/REPORT.md in /private/tmp/claude-501/bnruns/bngcwrebut/twhome/teams/closecache with exactly these ## headings, this wording, this order:
## What you asked
## What I understood
## The result
## Verified
## Decisions made for you
## Not done, and why
## Risks
## Cost
"What you asked" quotes the task. "Verified" gives each command, where it ran, the commit, and its
exit code. "Decisions made for you" covers the ones that change the result most (D4, D6, D7, D8,
D9) and points at afk/decisions.md, and adds every member's **Decisions I made** items that matter.
"Risks" includes review finding 2 (D8, cross-filesystem link) and whatever the closer flagged that
is still open after cf45e6a. "Cost" gives wall time and tokens per member.

Write no other file, edit nothing in the repository, then end your own report with a
**Decisions I made** list and record done.
