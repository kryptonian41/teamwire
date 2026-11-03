<!-- teamwire:generated completion=01M5MYF0KCKZ6NFNBZ5G6ASGXD -->
# Result from rev_start (done)

- Team: afkmode
- Status: done
- Generation: 1
- Completion: 1
- Time: 2026-10-23T17:55:42.316Z
- Final message from: hook
- Member report: /private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/reports/rev_start.md

## Summary

Reviewed afkmode/start: sound overall; 1 medium (lead_cwd from shell cwd breaks reopen), 1 low integration note; 18/18 afk tests pass. See reports/rev_start.md

## Task (first 300 characters)

# Brief: reviewer of branch afkmode/start (AFK run "afkmode") You are a reviewer: you read, you never edit, commit or message anyone. The lead runs this team while the user is away. - The user's task, word for word: /private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/afk/task.md. Its "Na…

## Final message

I finished the review of `afkmode/start`. Most of the branch is sound, with one medium finding and one low one. The full report is at `/private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/reports/rev_start.md`.

- **Medium finding.** `teamwire afk start` records `lead_cwd` as the shell's current directory (`crates/teamwire-cli/src/cmd/afk.rs:209`). Launch records the folder the lead session started in. Suppose the lead's shell has moved into a subfolder with `cd`. Then `teamwire reopen` resumes the lead in that subfolder, and the resume finds no session there. Reopen also skips its own fallback, because a folder is already recorded. The implementer listed this choice as their own decision, and I report it because it breaks reopen.
- **Low finding, which only matters once the `endings` branch is merged.** Sometimes the team row is written and then the copy of `afk/task.md` fails. When that happens, the command tells the user to run `teamwire close`, which will not work:
  - Close refuses an AFK team that has no `REPORT.md`.
  - A second `afk start` from the same session refuses with `afk_already_running`.
- **Checked and correct:**
  - Every refusal code from task.md exists, uses the binding name, and comes before anything is written.
  - The store's new fourth parameter, the budget refusal and the model-table warning match the agreed decisions.
  - I found no stubbed or hard-coded test, and no hard-rule violation.
- **Tests:** I ran the three new test files on a scratch copy of the branch, on macOS: 18 passed.

I did not run `scripts/gate.sh fast` or `scripts/test-changed.sh`. The implementer reports both passing.
