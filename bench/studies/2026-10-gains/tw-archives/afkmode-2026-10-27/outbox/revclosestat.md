<!-- teamwire:generated completion=01M5YVV3P4XZAZJW73ZSPWA879 -->
# Result from revclosestat (done)

- Team: afkmode
- Status: done
- Generation: 1
- Completion: 1
- Time: 2026-10-27T14:22:17.284Z
- Final message from: hook
- Member report: /private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/reports/revclosestat.md

## Summary

afkmode/closestat reviewed: sound, no correctness/security/data-loss defect; 2 low findings (full-report close test passes on afk/base but is not a guard; an unreadable REPORT.md is reported as missing). Report: reports/revclosestat.md

## Task (first 300 characters)

# Brief: reviewer of branch afkmode/closestat in AFK run afkmode You are a reviewer. You edit nothing and commit nothing. No human is present; the lead decides. The diff is `git diff afk/base...afkmode/closestat` in `/private/tmp/claude-501/bnruns/bn4tu2e1us/clone`. Read, in this order: the impleme…

## Final message

I found no correctness, security or data-loss problem on the `afkmode/closestat` branch. I did not build it or run any test: I reviewed by reading the diff and the code it calls. The full report is at `/private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/reports/revclosestat.md`, and I recorded completion as done.

Every name, refusal code, message and field matches the task, the brief and the agreed decisions. Each `--json` change shares its commit with its dashboard schema copy. Status, teams and close all work on an AFK team with zero members, and tests cover each of those cases.

There are two low findings, and neither blocks a merge:

1. **A close test that will pass on the base branch.** `close_of_an_afk_team_with_a_full_report_archives_the_afk_files` (`crates/teamwire-close/tests/afk_report.rs:50`) passes on afk/base even without the report check. The test is not marked as a guard, so the closer's check that each test fails without its change will flag it. Expect that result, or list the test as a guard.
2. **A report that exists but cannot be read is called "missing".** Close gives that message for any read error (`crates/teamwire-close/src/afk_report.rs:26-31`). One example is a stray byte that is not valid UTF-8. The status view treats only a truly absent file as missing. This case is unlikely, because the reporter writes the file as UTF-8.

The implementer made two other choices, and I accepted both as they stand:
- **Milliseconds in the status line.** The status line prints times like `…00.000Z`, which differs from the brief's example. Every other line and the JSON use the same form, and the docs use a placeholder there.
- **The report is checked once, before the close lock.** A report deleted in the instant between that check and the lock would go unnoticed.

I checked zero-member handling only for status, teams and close, plus the watch's team-complete rule. I did not look at reopen, verify or metrics, because this branch does not touch them.
