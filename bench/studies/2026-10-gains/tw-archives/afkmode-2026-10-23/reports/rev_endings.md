# Review of afkmode/endings (close and status for AFK teams)

## Summary

- Verdict: the branch is sound. It builds everything that design section 5.5, the binding names in task.md, and the endings brief ask for. I found no correctness, security or data-loss problem.
- One finding, low severity: a CLI status test depends on today's date and starts failing on 2027-09-28 with no code change.
- Close refuses an open AFK team before any step runs, with the D5 text. The `--role` removal and non-AFK teams are unaffected.
- `StatusView.afk`, `AfkView` (`budget_ends_at`, `seconds_left`, `spent`, `decisions`), `teams --json` `afk_budget_ends_at`, and the ui zod schemas match task.md word for word.
- I ran the brief's check in a scratch copy of the branch. All Rust tests passed (exit 0), `bun test` passed (163 pass, 3 skip, 0 fail), and `tsc --noEmit` reported no errors. Clippy with `-D warnings` was clean on teamwire-close, teamwire-watch and teamwire-msg.
- After `start` merges, the four-argument `create_afk_team` breaks four test call sites on this branch. D21 expects this. The list is below.

Reviewed: `git diff main...afkmode/endings` (3 commits, aafbbfc, d6ac475, 6b794a2, 19 files). I also read the connected code: close `plan_close_with`/`team_plan`/`close_observed`/`empty.rs`, watch `read_rows`/`status_with`, the CLI's fallback `StatusView` in cmd/status.rs, `TeamEntry` in teamwire-msg, `Timestamp` Display and Serialize, and the skill branch's report headings.

## Findings, ranked by harm

### 1. Low: a CLI status test starts failing on 2027-09-28T12:00Z

- File: `crates/teamwire-cli/tests/status.rs:142`, in `an_afk_team_prints_one_afk_line_after_the_team_line`.
- The test runs the real `teamwire` binary, which reads the real clock. The fixture's fake clock starts at 2026-09-28T12:00Z (`FakeClock::DEFAULT_START_MILLIS`), so the budget `years(10)` ends at 2036-09-25T12:00Z (line 110).
- The assertion at line 142 requires `seconds_left > 9 * 365 * 24 * 3600`, which is 3285 days. The value holds only while the real time is before 2036-09-25 minus 3285 days, which is 2027-09-28T12:00Z.
- Failing scenario: run `cargo test -p teamwire-cli --test status` on or after 2027-09-28T12:00Z. The test fails with no code change, and so does the full gate on every branch.
- Lines 116 and 146 (`starts_with("2036-")`) hold until 2036-09-25. After that date the budget counts as spent and the whole test fails.
- The product code is correct. Only the test depends on the date.

No other findings.

## Checked and correct (no finding)

| What | Where | Result |
| --- | --- | --- |
| The refusal runs only for an open team, inside `team_plan`, and never inside `member_plan` | `crates/teamwire-close/src/plan.rs:73` | Matches D5. `close_observed` returns the refusals before any step runs (`lib.rs:130`). The empty-store shortcut needs a store with no team row, so an AFK team never takes it. |
| Refusal text | `crates/teamwire-close/src/afk.rs:31-61` | Matches D5: `... is missing` and `... lacks: <h>, <h>`. An unreadable file gets its own `cannot be read` text, which the implementer recorded. |
| Heading parser | `afk.rs:69-87` | `## 3. The result`, `3. **The result**`, `**3. The result**`, `## 1. **What you asked**: ...` and the skill's `1. **What you asked**: the task, quoted.` all match. A non-`#` item counts only as `1.` to `8.`. The slices fall on ASCII byte boundaries, so they cannot panic. The eight headings match design section 6 part 7 and the skill branch's text. |
| Team directory | `afk.rs:23`, `watch/src/afk.rs:22` | `home/teams/<name>` is the parent of `database_path`, the same directory status already uses. |
| `seconds_left`, `spent` | `watch/src/afk.rs:31-33` | `ends.saturating_since(now)` gives whole seconds, rounded down, and 0 once spent. `spent` is `now >= ends`, with time from `ctx.clock`. Matches D17. |
| Decision count | `watch/src/afk.rs:44-50` | Matches `^D[0-9]+\s` line by line. A missing file counts 0. Matches D17. |
| `status <team>` text | `cmd/status.rs:94-110,119` | Matches D7 for both the running and the spent form. |
| `StatusView` constructors | `status.rs` error path and the CLI's `all_teams` fallback | Both use `..StatusView::default()`, so `afk` is `None` for an unreadable store. |
| `teams` mark and JSON | `cmd/teams.rs:44-56`, `directory.rs:20,50` | `(<state>, afk)`. `afk_budget_ends_at` is a string or `null`. The MCP `team_teams` tool carries the same key, and a test covers it. |
| ui schemas | `ui/src/shared/teamwire.ts` | `afk: AfkView.nullish()`, `afk_budget_ends_at: z.string().nullish()`. The fixtures cover the object, `null` and an absent key. The change is in the same commit as the JSON (6b794a2). |
| Teams with zero members | watch `an_afk_team_with_no_members_renders`; close `a_full_report_closes`; CLI and MCP teams tests | Status, close and teams all work on an AFK team with no members, which is the design section 7 risk for these readers. |
| Tests fail without their fix | all new tests | Each asserts the exact refusal text, the view values or the JSON key. No test is stubbed or hard-coded to pass. |
| Ownership | 19 files | Every file is in the brief's section 11 list. The branch touches no docs, store, launch, hook or protected file. |

## Notes for the parent (not findings)

- Merging `start` requires the fourth argument (`&serde_json::Value::Null`) at four call sites on this branch:
  - `crates/teamwire-close/tests/afk.rs:33`
  - `crates/teamwire-watch/tests/support/fixture.rs:77`, which the CLI status tests also use
  - `crates/teamwire-cli/tests/teams.rs:92`
  - `crates/teamwire-mcp/tests/teams.rs:76`
- The brief asked for units 3 and 4 as two commits. The implementer made one commit, 6b794a2, because the ui files hold both changes. The implementer recorded this decision. The brief's end-state line "the ui schema change is in the same commit as the JSON change it copies" still holds.
- The implementer reports that `scripts/gate.sh fast` exits 0 but tidy prints WEAKENED lines for the `#![expect(` attributes at the top of the two new test files. I did not run tidy, because the scratch copy has no git history for `--base`. The closer may see the same lines from `teamwire verify`.
- The notice to `skill` was sent (message m006, 17:49:15Z).
- No test runs the `teamwire close` binary on an AFK team. The library tests cover the refusal, and the CLI prints `REFUSED: <text>` through code that already exists. The brief's "Done when" list does not require a binary test.

## Commands I ran

| Command | Where | Exit |
| --- | --- | --- |
| `cargo test -p teamwire-close -p teamwire-watch -p teamwire-msg -p teamwire-mcp -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux` | `git archive` copy of afkmode/endings (6b794a2) in my scratchpad | 0 |
| `bun test` | the same copy, `ui/` | 0 (163 pass, 3 skip, 0 fail) |
| `bun run typecheck` (`tsc --noEmit`) | the same copy, `ui/` | no error output |
| `cargo clippy -p teamwire-close -p teamwire-watch -p teamwire-msg --locked --all-targets -- -D warnings` | the same copy | 0 |

## Decisions I made

- I ran the brief's crate tests and the ui checks in a `git archive` copy of the branch in my scratchpad, not in a worktree. The ui copy's `node_modules` is a symlink to the endings worktree's `ui/node_modules`, used only for reading. The brief names no place to run checks, and I must not write into a member's directory or add a worktree.
- I counted the merge of units 3 and 4 into one commit as a note, not a finding, because every end-state line of the brief is met.
- I accepted D5's lenient matching as agreed: a numbered body line such as `1. Cost of tokens` counts as the `Cost` heading. This is not a finding.
