# Review of afkmode/budget (reviewer: revbudget)

Diff: `git diff afk/base...afkmode/budget`, three commits (a54e5d6, 669b8bd, 34098e1), head 34098e1.
Files: `crates/teamwire-launch/src/member.rs`, `crates/teamwire-launch/src/plan_rules.rs`,
`crates/teamwire-launch/tests/afk_budget.rs`. The worktree is clean at 34098e1.

## Summary

The work is sound. Both rules match the brief, D14 and D15 word for word, and I found no
correctness, security or data-loss defect that needs a fix round.

- The budget refusal text matches D14 exactly. The check uses `ctx.clock.now() >= ends`, so "at or
  past" holds. The refused member skips `launch_member` entirely, so no member row, event,
  worktree or place is made for it.
- The warning text matches D15 and both worked examples in the brief. Effort uses whole-word
  matching, so `xhigh` is not misread as `high`. Model uses case-insensitive substring matching.
  A missing model or effort counts as below. One warning per member.
- All five tests exist with the brief's exact names. The verify log shows 408 passed, which is the
  base's 403 plus these 5, then gate fmt, tidy and clippy ok.
- There are no new `pub` items, no new dependencies, and no file outside the three owned files.
- Nothing in this diff breaks on an AFK team with zero members. `launch_all` reads only the team
  row. The open-implementer count in `planning_warnings` is 0 for an empty member list.

Two low-severity findings and three notes follow. Neither finding blocks the merge.

## Findings, ranked by harm

### 1. Low: every AFK launch that refuses an implementer loses its launch span in the telemetry export

- Where: `crates/teamwire-launch/src/member.rs:35` sets `requested = plan.members.len()`, which
  counts the refused member. `crates/teamwire-launch/src/member.rs:70-78` refuses that member
  without writing an event. `crates/teamwire-otel/src/spans.rs:189` writes the `teamwire.launch`
  span only when the count of `member.started` and `member.failed` events for that launch id
  equals `requested`.
- Input: an AFK team past its budget, and `teamwire launch --add` with one implementer and one
  reviewer.
- Wrong result: `requested` is 2 in the reviewer's `member.started` data, and only one terminal
  event carries that launch id. So the count is never 2, and the export never writes a
  `teamwire.launch` span for this launch.
- Context: the brief requires "No member row, event, worktree or place is made for it", so the
  missing event is by design. The brief does not mention `requested`. Today's `team_closed`
  refusal and the failures before the member is recorded have the same gap. The budget refusal
  turns a rare path into a routine one in every AFK run that ends past its budget. I found this by
  reading the code and did not run it.

### 2. Low: the budget refusal lets the implementer through when the team row read fails

- Where: `crates/teamwire-launch/src/member.rs:64-69`. The read is
  `ctx.store.read(|tx| tx.team()).ok().flatten()`, so any `StoreError` turns into "no budget".
- Input: an AFK team past its budget, and a store read in `launch_all` that returns an error. One
  cause is `SQLITE_BUSY` past the busy timeout. Another is a team row that fails to parse.
- Wrong result: `budget_ends` is `None`, and `launch_member` starts the implementer past the
  budget with no refusal and no warning. The member's own write (`member.rs:285`) uses
  `BEGIN IMMEDIATE` with a busy timeout, so after a short busy spell that write succeeds.
- Likelihood: low. `validate` read the same row with `?` moments earlier and would have refused
  with `store_unreadable` on a lasting error, so only a short error between the two reads triggers
  this. The `.ok().flatten()` shape copies `crates/teamwire-cli/src/cmd/hook.rs:224`.

## Notes, not defects

- `before_the_budget_ends_an_implementer_starts` passes on afk/base. The brief's "Done when" reads:
  "Each test of new behaviour fails without its change; a test marked (guard) pins today's
  behaviour". The brief and D33 do not mark this test as a guard, yet no change can make it fail
  on the base. The test still has value. It fails if the refusal ignores the clock, and the
  past-the-budget test pins `>=` at the exact end time. The implementer's report claims the
  revert check only for the first test, which is accurate.
- The report paraphrases the check instead of giving its real output. The brief's "Done when"
  reads: "The check in section 12 passes on your last commit, with its real output in your
  report." The report says "every suite ok" and "fmt ok, tidy ok, clippy ok" with no counts. The
  verify log `verify/budget.log` holds the real output: 41 `test result: ok` lines, 408 passed in
  total, 0 failed, then `gate 1/3 fmt: ok`, `gate 2/3 tidy: ok`, `gate 3/3 clippy: ok`.
- tidy prints `WEAKENED` for `member.rs:60` and `tests/afk_budget.rs:4`. Both lines are
  `#[expect(...)]` attributes that copy existing patterns. The first is the
  `clippy::redundant_closure_for_method_calls` expect that ten other `store.read(|tx| tx.team())`
  sites carry, for example `crates/teamwire-launch/src/reopen.rs:112`. The second is the
  `dead_code` expect on `mod common` that every launch test file carries, for example
  `tests/refusals.rs:3`. No test was loosened, so if `teamwire verify` flags this branch, the flag
  is safe to waive.

## Checked and found correct

| Brief requirement | Where | Result |
| --- | --- | --- |
| Refusal text `afk_budget_spent: the AFK run's budget ended at <ts>; past it only reviewers, investigators and closers may join` | member.rs:73-75 | matches, `<ts>` is the `Timestamp` Display |
| Refuse only `implementer`, at or past the end, only when the column is set | member.rs:72 | correct |
| Nothing recorded for the refused member | member.rs:70-78 | `launch_member` never runs |
| CLI prints `<role>: refused: ...` with no change to `cmd/launch.rs` | cmd/launch.rs:133 | unchanged and correct |
| Validation and `--dry-run` unchanged by the refusal | lib.rs:182 | the refusal lives only in `launch_all` |
| Warning text and `none` for a missing value | plan_rules.rs:116-122 | matches both brief examples |
| Effort floor high for implementer and investigator, xhigh for reviewer and closer | plan_rules.rs:16-21 | correct |
| A model or effort word outside the table is not judged | plan_rules.rs:25-37 | correct |
| AFK only when `cx.existing` has `afk_budget_ends_at` set | plan_rules.rs:108-111 | correct |
| Time from the `Clock` trait | member.rs:72 | `ctx.clock.now()` |
| No unwrap, expect or panic outside tests | member.rs, plan_rules.rs | none |
| `new_team` and `lead_cwd` left alone (D29) | member.rs:108-140 | unchanged |
| Five test names | tests/afk_budget.rs | all present, exact names |
| Test of the refusal fails without it | tests/afk_budget.rs:68-91 | yes: `api` would be `Started` |
| Test of the warnings fails without them | tests/afk_budget.rs:153-181 | yes: the filtered list would be empty |

I did not run any tests myself. The pass counts above come from the verify log.

## Decisions I made

- I report the missing launch span (finding 1) even though the brief requires that no event is
  written. The finding is about the `requested` count, which the brief does not cover. I am not
  reopening the "no event" decision.
- I treat tidy's two `WEAKENED` lines as false positives, not findings, because both attributes
  copy patterns already in the codebase and loosen no test.
- I did not run a revert check by building afk/base code against the new tests. The code makes the
  result certain for both tests of new behaviour, so the build was not worth the time.
