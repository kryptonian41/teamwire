<!-- teamwire:generated completion=01M5YW3YW6PKYSJG62N9PKQ3P5 -->
# Result from revskill (done)

- Team: afkmode
- Status: done
- Generation: 1
- Completion: 1
- Time: 2026-10-27T14:27:07.270Z
- Final message from: hook
- Member report: /private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/reports/revskill.md

## Summary

Reviewed afkmode/skill: sound code and tests; 1 medium doc finding (cli.md/failure-modes say close ends a failed-start team, close refuses it), 1 low-medium (ADR 0025 older-binary claim false), 5 low doc mismatches; report reports/revskill.md

## Task (first 300 characters)

# Brief: reviewer of branch afkmode/skill in AFK run afkmode You are a reviewer. You edit nothing and commit nothing. No human is present; the lead decides. The diff is `git diff afk/base...afkmode/skill` in `/private/tmp/claude-501/bnruns/bn4tu2e1us/clone`. Read, in this order: the implementer's b…

## Final message

I've finished the review of `afkmode/skill`, and the branch is sound. All the problems are in the docs.

The skill keeps every rule of design section 6. The install code is correct. The five named tests exist with the plan's exact names, and each one fails without its change. Every test name that `docs/failure-modes.md` cites exists on the branch the plan assigns it to.

I re-ran the branch's own check on its worktree, and it passed:
- **Tests:** 440 passed, 0 failed.
- **`scripts/check-docs.sh`:** `docs check: ok`.
- **`scripts/gate.sh fast`:** exit 0. The two tidy notes were expected: the new ADR is a protected file, and the other note is the lead's stub command.

**Findings, most harmful first:**
1. **Medium.** `docs/api/cli.md:140` and `docs/failure-modes.md:155` say that `teamwire close <team>` ends a team left behind by a failed `afk start`. Close on the closestat branch refuses that team with `afk_report_incomplete`, and no flag overrides it.
2. **Low-medium.** `docs/adr/0025-afk-runs.md:100` says an older binary opens the database read-only. ADR 0015 and the store code refuse a newer schema even on a read-only open. The ADR folder is protected, so fixing this after merge needs a new ADR.
3. **Low, five items.**
   - The skill paraphrases the budget-spent stop note instead of quoting D31. It also says the note starts with `budget spent at`, but the real note starts with the team tag.
   - `docs/failure-modes.md` says doctor warns about an edited AFK skill. Doctor fails instead.
   - The skill says every `failed:` line names a team that now exists. Two of the start branch's `failed:` cases happen before any team row exists.
   - Five sentences in `docs/api/cli.md` differ from the code the other branches built. One is a third close refusal form that closestat added after the skill branch was written.
   - ADR 0025 cites the wrong ADR, and one failure-modes row describes the taken-name check wrongly.

The full report, with file:line and a concrete failing case for each finding, is at `/private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/reports/revskill.md`. I recorded completion with `teamwire done`.

I compared the docs against each branch's head at review time. If those branches change again, the closer needs to repeat that comparison on the joined branch.
