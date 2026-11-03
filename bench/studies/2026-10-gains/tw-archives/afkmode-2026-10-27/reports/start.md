- 0d2db29 launch library: afk_check/afk_start in teamwire-launch/src/afk.rs, 8 tests in tests/afk_start.rs | left: doctor launch_checks, CLI command + binary tests
- d2ad30c doctor launch_checks + inputs helper; healthy test setup moved to tests/support/healthy.rs | left: the afk command
- e026e22 teamwire afk start command (cmd/afk.rs), parse_duration pub(crate), 6 binary tests | left: nothing

# Report: start

Status: done. Check run on e026e22:
`cargo test -p teamwire-cli -p teamwire-launch --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux` exit 0, 419 passed, 0 failed (403 on base + 16 new: 8 launch, 2 doctor, 6 afk).
`scripts/gate.sh fast`: fmt ok, tidy ok, clippy ok. Tidy prints `WEAKENED` for the new `#[expect(` lines in cmd/afk.rs, tests/afk.rs and tests/afk_start.rs and still exits 0 (the plan said it does for the stub's expect). They are new lint expectations with reasons, not loosened tests.
`teamwire afk start --help` shows --team, --title, --task, --budget; "not built yet" is gone.
doctor.rs is 798 lines (tidy limit 800); the next doctor change must move code out.

New `pub` items (teamwire-launch, re-exported in lib.rs): `AfkRequest`, `AfkRefusal`, `AfkChecked` (with `team()`), `AfkStarted`, `AfkError { Refused, Failed }`, `afk_check`, `afk_start`. Crate-private widenings: `validate::{BACKGROUND_LEAD, lead_plan, title_problem}`, `hold::{lead_process (now takes &Registry), team_of}`. CLI: `doctor::{inputs, launch_checks}`, `metrics::parse_duration` pub(crate).

Decisions I made:
- Library shape: `afk_check(env, request)` runs every refusal up to `task_empty` (no_lead_session, background_lead, afk_already_running, bad_name, title_*, team_exists, bad_budget, task_*); the CLI runs doctor; `afk_start(env, checked)` does no_lead_process, store open, row, task copy, hold. The `--budget` string is parsed in the CLI (parse_duration is the CLI's) and passed as `Result<Duration, String>` so bad_budget keeps its place in the order.
- `budget_ends_at` is computed in `afk_check` (clock at check time), not again at create.
- `no_lead_process` message is the text `hold::lead_process` already produces ("NO SLEEP HOLD: ...").
- `title_missing` / `title_too_long` messages are launch's, which say "the spec"; reused as is.
- Store open or row-write failure in `afk_start` is `Failed` (exit 2) with no "team exists" suffix; only the task-copy failure adds `; team <team> exists`.
- The `afk/` directory is made with `std::fs::create_dir_all` (as launch does) before `Platform::write_atomic`.
- `refuse_member("afk start")` runs first, then `home_or_refuse`.
- Doctor warn/fail lines come from `launch_checks`, which drops `binary_checks`' own "host" pass line when the host is none, so there is one "host" check.
- Extra tests beyond the named ones: `afk_start_records_the_current_directory_when_the_harness_cannot_place_the_lead`, `afk_start_refuses_no_session_a_background_session_and_bad_arguments_in_order`, `launch_checks_on_a_healthy_herdr_setup_pass_and_include_the_herdr_hook`.
- Intermediate commit d2ad30c has `launch_checks` unused in the binary (dead code) until e026e22 uses it; the branch tip is clean.
