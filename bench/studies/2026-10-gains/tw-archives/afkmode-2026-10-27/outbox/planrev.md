<!-- teamwire:generated completion=01M5YSZEW1XPES6H17KB5EE50Q -->
# Result from planrev (done)

- Team: afkmode
- Status: done
- Generation: 1
- Completion: 1
- Time: 2026-10-27T13:49:42.657Z
- Final message from: hook
- Member report: /private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/reports/planrev.md

## Summary

Plan review written: 9 ranked gaps; top one is the closer's full gate stopping at the tmux test so e2e, crash and cargo doc never run

## Task (first 300 characters)

# Brief: plan reviewer for AFK run afkmode You are a reviewer. You edit nothing and commit nothing. No human is present in this run; the lead decides. Read, in this order: the task `$TEAMWIRE_HOME/teams/afkmode/afk/task.md` (its "Names the design does not give" list is binding), `docs/design/2026-1…

## Final message

I reviewed the AFK plan and found nine gaps. One is serious, two are medium, and six are low. The full report is at `/private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/reports/planrev.md`.

Most of the plan holds up against the code. The amendment patch applies cleanly to afk/base. No file has two owners. Every name on the task's binding list reaches a brief spelled correctly.

The top three gaps:

1. **High: the closer's full gate cannot pass inside tmux, and the plan says to accept that.** The full gate's test step stops at the first failing test. Here that is `herdr_env_wins_over_tmux`, which fails whenever tmux is among the parent processes. After it, the e2e tests, the crash tests, `cargo doc` and the docs check never run. The plan relies on that gate as the only check for those (plan.md:165-167, plan.md:180).
2. **Medium: the success output of `teamwire afk start` has no test.** Nothing tests the five success lines or the exit-2 message, yet the skill and `docs/api/cli.md` quote those lines word for word (start brief:42-58).
3. **Medium: the start unit is told to reuse code in a file it does not own.** The code that records the lead's working directory is inline in a private function in `member.rs`, which belongs to the budget unit. Start must either edit that file, which causes a merge conflict, or copy the code, which breaks the AGENTS.md rule against parallel helpers.

The lower-ranked gaps:
- The closestat brief commits its JSON changes and the dashboard schema separately, which breaks the same-commit rule.
- The past-the-budget Stop hook note drops the "decide it yourself and log it" instruction.
- No test checks that doctor checks the AFK skill.
- Several required tests already pass on afk/base, so they cannot meet the briefs' "fails without the change" rule.
- Two member checks skip the crates that depend on the crate they change.
- A decision log indented like the design's example counts as zero decisions.

I ran no tests or gate. To check file sizes I ran tidy once, which only reads, and `git apply --check` on the patch.
