- 2078d87 Store: create_afk_team takes extra, merged into team.created; store test | left: launch budget+model table, --add test, lead lookup export, afk start CLI, notice to skill
- 9cdf8d1 Launch: implementer budget refusal, model-table warning, --add into empty AFK team test | left: lead lookup export, afk start CLI, notice
- da0c05e Launch: pub ensure_hold and lead_process_missing | left: afk start CLI, notice
- 373ebc2 Launch: check_new_team (name, title, team_exists rules shared with validate) | left: afk start CLI, notice
- d646e55 CLI: teamwire afk start (cmd/afk.rs, args, doctor launch_checks, parse_duration pub(crate)) + tests | left: full check, gate fast, notice to skill
- d22c5d4 Clippy fixes for afk code and tests | left: full check, test-changed, notice to skill

## Commands run
- `cargo test -p teamwire-store -p teamwire-launch -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux`: exit 0, 51 result lines, all ok.
- `scripts/gate.sh fast`: exit 0 (fmt, tidy, clippy ok). Tidy prints `WEAKENED` for the `#[expect(...)]` lines in my new files (cmd/afk.rs and the three tests/afk.rs); they are the lint expectations the repo's other tests carry, none loosens a check.
- `scripts/test-changed.sh`: stopped at `tmux_wiring::herdr_env_wins_over_tmux` (the D12 test, fails at the base under tmux). Re-ran the same package list with `--no-fail-fast -- --skip herdr_env_wins_over_tmux`: exit 0.
- Notice with the exact output lines and refusal codes sent to `skill`.

## Decisions I made
- `bad_task` (new refusal code) when the task file cannot be read; checked before any write.
- Order of checks in `afk start`: bad_budget, no_lead_session, background_lead, afk_already_running, name/title/team_exists, bad_task, doctor_failed, no_lead_process. All before `Store::open`, so a refusal creates no team directory.
- `lead_cwd` is the current directory of `afk start` (not the harness's `session_cwd`).
- Launch gained `check_new_team` (shares `title_problem`/`team_exists` with `validate`) and the `BACKGROUND_LEAD` text, both exported, so `afk start` uses launch's rules and messages without copies.
- `lead_process_missing` returns launch's own warning text as the `no_lead_process` message; a lookup error also counts as "not found".
- Doctor helper: `doctor::launch_checks` = `.worktrees` + binary + lead-setup checks, plus a FAIL `host` when no host is detected; `doctor::inputs_for` was extracted from `run` (tests/doctor.rs includes doctor.rs with a stub Wiring, so I used only items that stub has).
- Budget check at launch reads the team row from the store (only for `--add`); the dry-run warning is `role <r>: afk_budget_spent: ...` in plan warnings; a real launch past the budget prints that warning too, then the per-member refusal.
- Model-table warning text names "the default model/effort" when only one of the two is set and the other is below.
- The budget end and the row's `created_at` come from two clock readings, so they differ by milliseconds; the CLI test allows 5 s.
- The macOS success test starts a real `caffeinate -w <sleep pid>`; the test kills the `sleep` lead at the end so caffeinate exits. The `no_lead_process` test is `#[cfg(target_os = "macos")]` because Linux has no sleep hold.
- c774aed afk start: lead_cwd from harness session_cwd (omitted when unknown); task copy before the team row | left: nothing (review findings 1 and 2 fixed)

## Review round (rev_start)
- Finding 1 (medium) held. `afk start` recorded its shell's current directory; launch records `harness.session_cwd(session)` and nothing when unknown, and reopen prefers the recorded value (reopen_lead.rs:62). Now `afk start` does the same. Tests: `lead_cwd_is_the_folder_the_session_started_in_not_the_shells` (session file carries a `cwd` different from the shell's) and the success test now asserts no `lead_cwd` key when the session names no folder. Both fail on the old code (checked by running them against the old afk.rs).
- Finding 2 (low) held. The task copy (`afk/` and `afk/task.md`) now happens before `Store::open` and the team row, so a failed copy leaves no row and a rerun works; the failure message now says "fix the cause and run it again" instead of "run teamwire close". Test: `a_task_copy_that_fails_leaves_no_team_behind` (a file sits where `afk/` goes: exit 6, no team.db, and the rerun succeeds). Fails on the old code. Design 5.2 lists the copy after the row; only the order of the two steps changed, nothing visible on success.
- Reviewer's three "harmless" items: left as they are.
- Re-ran after the fixes: `scripts/gate.sh fast` exit 0; the check command exit 0.

Decisions I made (added): lead_cwd now comes from the harness's session_cwd like launch (replaces my earlier current-directory decision); task copy before the team row.
