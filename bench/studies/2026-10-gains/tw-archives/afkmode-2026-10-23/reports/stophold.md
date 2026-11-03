- 99ee44d afk_hold_notes in teamwire-report (afk_hold.rs), has_working_member factored out of lead_stop.rs, tests/afk_hold.rs | left: hook.rs join, tests/lead_hook.rs, notice to skill
- 3efaec1 Stop hook returns afk_hold_notes after the watch notes in one reply (hook.rs), tests in tests/lead_hook.rs | left: check, gate fast, notice to skill
- f0dce4b clippy fixes in tests/afk_hold.rs and tests/lead_hook.rs (no indexing, no fs::write, events helper takes a Store) | left: check, test-changed, notice to skill

## Commands and exit codes (last commit f0dce4b)
- `cargo test -p teamwire-report -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux`: exit 0.
- `scripts/gate.sh fast`: exit 0 (fmt, tidy, clippy ok). Tidy prints `WEAKENED crates/teamwire-report/tests/afk_hold.rs:3`: it is the file-level `#![expect(clippy::unwrap_used, clippy::panic, ...)]` every integration test file carries (same as tests/lead_stop.rs:3). No test was removed or loosened.
- `scripts/test-changed.sh`: exit 101, only `herdr_env_wins_over_tmux` failed (D12, known). Rerun of the same packages (cli, crash-tests, e2e, mcp, report) with `--no-fail-fast -- --skip herdr_env_wins_over_tmux`: exit 0.
- Notice sent to `skill` (m004) naming `afk_hold_notes` and the note texts at afk_hold.rs:87 and :83.

## Decisions I made
- The note's `<team dir>` is built as `home.join("teams").join(team)` in `afk_hold.rs`, the same path `paths::team_dir` and `database_path` use.
- Past-budget test is `now >= ends` (equal counts as spent), as D6 says.
- In lead_hook.rs the AFK team is made by its own `afk_team(working)` helper (own home and store), because `Fixture::new` in teamwire-verify creates its team with `create_team` and that file is not mine. It is the only caller of `create_afk_team` in that crate (D21).
- The unreadable-team test replaces the team's `team.db` file with a directory, to avoid an added `#[expect(disallowed_methods)]` that tidy flags as WEAKENED.
- `afk_hold_notes` reads each team once and returns early on the first team with a working member and a live lease, so teams after it in name order are not read.
- e5e8e23 unreadable-team test now writes a non-database file via Platform::write_atomic so the team is listed and the read error branch runs (fails when that branch returns early) | left: none

## Reprompt m007 (review in rev_stophold.md)
- Finding 1 holds. `team_names` lists a team only when `team.db` is a file (store.rs:125), so my directory trick never reached the `Err` branch. Fix: the test now overwrites `beta`'s `team.db` with a non-database file through `Platform::write_atomic` (no `#[expect]` added). Proof: with `return Vec::new()` added to the `Err` branch of afk_hold.rs, the test fails (10 passed, 1 failed); with the branch restored, 11 pass. The mutation was undone before the commit.
- Commands: `scripts/gate.sh fast` exit 0 (same single WEAKENED line for the file-level `#![expect]` at afk_hold.rs:3, as before); `cargo test -p teamwire-report -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux` exit 0.
- The two "notes for the parent" need no change from me.
- Decisions I made: replaces the earlier decision about the directory trick. The unreadable team is now a non-database file written with `Platform::write_atomic`.
