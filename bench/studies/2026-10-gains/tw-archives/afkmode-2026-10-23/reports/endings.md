- aafbbfc close refuses an AFK team without a complete afk/REPORT.md (afk_report_incomplete) | left: watch AfkView, status CLI + ui, teams, notice to skill
- d6ac475 watch: AfkView and StatusView.afk (budget_ends_at, seconds_left, spent, decisions) | left: status CLI line + ui afk schema, teams mark + ui, notice to skill
- 6b794a2 status afk line, teams (open, afk) mark + afk_budget_ends_at (TeamEntry), ui AfkView/TeamEntry schemas + fixtures + tests, cli/mcp tests (units 3 and 4 in one commit: the ui files hold both) | left: scripts/gate.sh fast, test-changed, notice to skill

## Commands run
- `cargo test -p teamwire-close -p teamwire-watch -p teamwire-msg -p teamwire-mcp -p teamwire-cli -p teamwire-crash-tests -p teamwire-e2e --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux`: exit 0
- `cd ui && bun test` (163 pass, 3 skip, 0 fail) and `bun run typecheck`: exit 0
- `scripts/gate.sh fast`: exit 0 (fmt, tidy, clippy ok). Its WEAKENED lines for `#![expect(` at the top of the two new test files crates/teamwire-close/tests/afk.rs and crates/teamwire-watch/tests/afk.rs come from tidy's rule against added `#[expect(`; every test file in the repo carries the same attribute.
- `scripts/test-changed.sh`: exit 101 only on `herdr_env_wins_over_tmux` (skipped by D12; fails with tmux not installed); the rest of its crates passed in the run above.

## Decisions I made
- Units 3 and 4 are one commit: ui/src/shared/teamwire.ts, teamwire.test.ts and both fixtures hold the status and the teams changes together, so a split would need hunk surgery. The ui schema is in the same commit as the JSON it copies.
- An unreadable REPORT.md (not "missing") is refused as `afk_report_incomplete: <path> cannot be read: <error>`.
- Heading match: after the `#`/number/`**` marker is stripped, the text starts with the heading, case-insensitive; a `#` heading may also carry a number (`## 3. The result`). A non-`#` line counts only as a numbered item 1 to 8.
- D21 helpers: `Bed::afk_team(budget)` in crates/teamwire-watch/tests/support/fixture.rs (used by watch and cli status tests), `afk_team(...)` in close tests/afk.rs, cli tests/teams.rs and mcp tests/teams.rs. Each holds one `create_afk_team(at, &team, ends)` call with today's three arguments: four places for the lead to change after `start` merges.
- Status CLI tests use the real clock (the binary has no fake clock): a 10-year budget for the running case, a 1-hour budget from the fixture's 2026-09-28 start for the spent case.
- `decisions` saturates at u32::MAX instead of failing on overflow.
- ui `afk` and `afk_budget_ends_at` are `.nullish()`; the fixtures carry the object in the first team, `null` in the second and omit the key in the third.
- 0ccf0c3 cli status test: budget 5000 years from the fixture's fixed start, expected text built from that timestamp, no 2036/2027 date | left: nothing
- Review round (rev_endings.md): one finding, a CLI status test that depended on today's date (`years(10)` budget, `> 9 years` left, `starts_with("2036-")`). It held. The binary reads the real clock and has no override, and `SystemTime::now` is banned, so the fix is a budget of 5000 years from the fixture's fixed start; the expected text and JSON value come from that timestamp, and the left-seconds bound is 1e9. The test is valid on any real date before about the year 6900. The spent-case test needs no change: its end (fixed 2026-09-28T13:00Z) is in the past on any later real date.
