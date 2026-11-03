# Brief: start, `teamwire afk start`

## 1. Who you are

You are `start`, an implementer in team `afkmode`. Nobody will answer a question from the user
during this run; the lead answers questions, and every open detail below is already decided.

The other members of wave 1, each on its own branch off `afk/base`:

| Role | Kind | Owns (summary) | Who uses its work |
| --- | --- | --- | --- |
| start (you) | implementer | `teamwire afk start`, doctor's launch checks, the launch crate's AFK start steps | the lead, the skill |
| budget | implementer | `crates/teamwire-launch/src/member.rs`, `plan_rules.rs`: the budget refusal and model warnings | launch |
| stophold | implementer | `crates/teamwire-report/src/lead_stop.rs`, `crates/teamwire-cli/src/cmd/hook.rs`: the AFK hold | the lead's Stop hook |
| closestat | implementer | close's report check, status, teams, `ui/src/shared/` | close, status, the dashboard |
| skill | implementer | the AFK skill, its install, every doc | the lead |

After you: a reviewer reads your branch, then a closer runs every check on the joined branch, then a
reporter writes the run's report. The exact files each member owns are in the plan (section 7).

## 2. The end state

`teamwire afk start --team NAME --title TITLE --task FILE [--budget DURATION]`, run from a lead
session's shell, creates an AFK run's team with no members, copies the task file into it, takes the
sleep hold, and prints what the user needs; or it refuses with one code and creates nothing. Design
section 5.2 is the behaviour; decisions D7 to D13 fix every detail it leaves open.

1. Refusals, in this order, stopping at the first (D9): `no_lead_session` (wiring's lead harness
   gives no current session), `background_lead` (launch's code and message), `afk_already_running`
   (an open team in the home has this session as lead and `afk_budget_ends_at` set), `bad_name`,
   `title_missing`, `title_too_long`, `team_exists` (launch's rules: `TeamName::parse`, the cleaned
   title at most 28 characters, a `team.db` already at `database_path`), `bad_budget` (D10),
   `task_unreadable`, `task_empty` (the file holds only white space), `doctor_failed`
   (`doctor_failed: <check name>: <detail>`), `no_lead_process` (the platform has a sleep hold and
   the lead harness's `processes_for_session` returns no process or fails).
   Each prints `refused: <code>: <message>`, then `nothing was created`, on stderr, exit 1.
2. Doctor's checks: a new `pub(crate) fn launch_checks(inputs: &Inputs<'_>, pick: &HostPick) ->
   Vec<Check>` in doctor.rs (D8): the host line (`FAIL` when the host is none), the herdr session
   hook (herdr only), `.worktrees ignored` (in the current directory), the harness and host binaries
   with their version and flag probes, and the lead setup checks. `doctor` itself prints what it
   printed before. Each `warn` prints as `warning: <name>: <detail>` and does not refuse.
3. On success: the team row through `Tx::create_afk_team(at, &team, budget_ends_at,
   &json!({"lead_cwd": cwd}))` with launch's lead fields (session and kind, harness, host, route as
   `lead_plan` works it out), built by `crate::member::new_team(&name, &title, &lead)`;
   `budget_ends_at = Clock::now() + budget`; `lead_cwd` from `crate::member::lead_cwd(registry,
   &lead)`, else the command's current directory (D11, D29). Then `<home>/teams/<team>/afk/task.md`,
   byte for byte, written with `Platform::write_atomic`. Then the sleep hold through `ensure_hold`,
   recorded on the row. Then on stdout exactly (D12):

   ```
   team <team>: AFK run started
   afk dir: <home>/teams/<team>/afk
   report: <home>/teams/<team>/afk/REPORT.md
   budget ends: <RFC 3339 UTC, Timestamp's Display>
   Keep the lid open and the machine on power: a closed lid still sleeps a Mac.
   ```

   Hold warnings go to stderr as `warning: ...`. Exit 0. A failure after the row exists (the task
   copy) prints `failed: <what>; team <team> exists` and exits 2.
4. `teamwire launch --add --spec <spec>` adds members to that team with no members. Today's code
   already accepts it (`validate.rs`, `(Some(e), true)` with an open state); a test proves it.
5. The steps after doctor (the `no_lead_process` check, create, copy, hold) live in a new
   `crates/teamwire-launch/src/afk.rs` as a library function, so launch's fakes can test each step
   (D13). Two binary tests also run the success path and the exit-2 path (D28). The CLI file `crates/teamwire-cli/src/cmd/afk.rs` parses, finds the session through
   wiring, runs `launch_checks`, calls the library and prints. Place the `afk_already_running`,
   name, title and `team_exists` checks where they test best, in the order above. One shape that
   works: `afk::check(env, request) -> Result<Checked, Refusal>` before doctor and
   `afk::start(ctx_parts, checked) -> Result<Started, Refusal>` after it.

## 3. Done when

- Every test named in section 10 exists with that exact name and passes. Each test of new
  behaviour fails without its change; a test marked (guard) pins today's behaviour and passes on
  afk/base too (D33).
- `teamwire afk start --help` shows the four flags of the contract; the stub's "not built yet" text
  is gone.
- The check in section 12 passes on your last commit, with its real output in your report.
- Your report lists every new `pub` item.

## 4. Out of scope

- Launch's budget refusal and model-table warnings (budget owns `member.rs` and `plan_rules.rs`).
- The Stop hook, close, status, teams, the dashboard, the skill and every doc: other members.
- A Codex lead. Any refusal for one beyond what launch already does.
- `cargo install`, `teamwire install`, pushing, pull requests.

## 5. When to do nothing

If `afk/base` lacks the lead's commit that adds `Command::Afk` to `crates/teamwire-cli/src/args.rs`
(check with `grep -n 'Afk(AfkArgs)' crates/teamwire-cli/src/args.rs`), change nothing, say so in your
report, and finish with status `blocked`.

## 6. Where you are

Your worktree is `/private/tmp/claude-501/bnruns/bn4tu2e1us/clone/.worktrees/` plus your slug, on
branch `afkmode/start`, cut from `afk/base` (5b86c1e plus the lead's afk-arguments commit). Setup
already ran `cargo build --workspace --all-targets --locked`, so run no install command.

## 7. Read first, in this order

1. `/private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/afk/plan.md`
2. `/private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/afk/decisions.md`, D6 to D13 and D23.
3. `docs/design/2026-10-19-afk-mode.md`, sections 4, 5.1, 5.2, and "Risks the reviewer should look at".
4. `AGENTS.md`, then `docs/conventions.md` section 6.
5. The code: `crates/teamwire-launch/src/validate.rs` (`validate`, `lead_plan`, `team_title`),
   `hold.rs` (`ensure_hold`, `lead_process`), `member.rs` (`create_team`, `new_team`, `lead_cwd`;
   read only), `crates/teamwire-cli/src/cmd/launch.rs` (wiring use and output), `cmd/doctor.rs`
   (`run`, `diagnose`), `crates/teamwire-cli/tests/doctor.rs` lines 1-230 (it compiles doctor.rs by
   `#[path]`; `Bed::healthy`) and 797-860 (`teamwire()`, `ignoring_repo()`,
   `the_binary_exits_zero_when_all_pass_and_two_when_one_fails`),
   `crates/teamwire-harness-claude/src/session.rs` lines 95-130 (the session file and
   `runs_session`), `crates/teamwire-launch/tests/sleephold.rs` and `tests/support/common.rs` (the
   fakes' rig).

## 8. Decisions already made (2026-10-27; full text in decisions.md)

- D7: the CLI arguments are fixed by the lead's commit: `AfkArgs { command: AfkCommand }`,
  `AfkCommand::Start(AfkStartArgs { team: String, title: String, task: PathBuf, budget: String })`,
  `--budget` default `8h`. Replace the stub body of `cmd/afk.rs`; keep the flags.
- D8: you own doctor.rs. `launch_checks` takes `&Inputs`, because tests/doctor.rs compiles doctor.rs
  with a stub `Wiring` that has only `platform`, `claude`, `codex`, `registry`, `clock`. Build
  `Inputs` in one helper that `run` and `cmd/afk.rs` both call. doctor.rs is about 705 non-test
  lines; tidy fails a file past 800.
- D9: refusal order, codes and output, as in section 2.
- D10: `--budget` uses `parse_duration` from `cmd/metrics.rs` (make it `pub(crate)`); zero, or an
  end past `Timestamp::checked_add`, is `bad_budget`.
- D11, D12, D13: `lead_cwd`, the success lines, the library split, as in section 2.
- D28: two binary tests run the success path and the exit-2 path; the recipe is in section 10,
  unit 3.
- D29: the lead's contract commit extracted `new_team` and `lead_cwd` from `create_team` in
  `member.rs`. Call them; budget owns `member.rs`, so never edit it.
- D33: a test marked (guard) pins today's behaviour; it need not fail without your change.
- D23: `crates/teamwire-cli/tests/tmux_wiring.rs::herdr_env_wins_over_tmux` fails on afk/base inside
  a tmux pane, before any change. Your check skips it by name. Do not edit or skip it any other way.
- Worked example of a refusal, as the user sees it:

  ```
  $ teamwire afk start --team my-team --title "Add docs" --task task.md
  refused: bad_name: <TeamName::parse's message>
  nothing was created
  $ echo $?
  1
  ```

## 9. What already exists

- `Tx::create_afk_team(at, team: &NewTeam, budget_ends_at, extra: &Value)` (teamwire-store,
  5b86c1e); `TeamRow.afk_budget_ends_at`. Do not change the store.
- `hold::ensure_hold`, `validate::lead_plan`, `validate::team_title`, `plan::clean`, `TITLE_MAX`:
  reuse them inside the launch crate.
- `pub(crate) fn new_team(name: &TeamName, title: &str, lead: &LeadPlan) -> NewTeam` and
  `pub(crate) fn lead_cwd(registry: &Registry, lead: &LeadPlan) -> Option<String>` in
  `crates/teamwire-launch/src/member.rs` (the lead's contract commit, D29). `create_team` calls
  both; call them from `afk.rs` as `crate::member::new_team` and `crate::member::lead_cwd`.
- `teamwire_store::team_names`, `Store::open_read_only`, `database_path`.
- `FakePlatform::without_sleep_hold`, `add_external_hold`, `hold_events`; the rig's `lead_at`.
- `cmd::send::home_or_refuse`, `refuse_member` (a member session may not start a run).

## 10. Units in order, commit after each

Use the tdd skill for each unit; test only at these seams: `teamwire afk start` through the CLI
binary with `teamwire_fakes::TestHome` (the refusals, the success output and the exit-2 path), the launch crate's AFK
start function with launch's fakes (`FakePlatform`, `FakeHarness`, `FakeClock`), and doctor's
`launch_checks` with the fake platform in `tests/doctor.rs`. If a check fails and the cause is not
clear from its output, use the diagnosing-bugs skill before your second fix attempt.

1. The launch library: `crates/teamwire-launch/src/afk.rs`, its `pub` items in `lib.rs`, what
   `hold.rs` and `validate.rs` must expose inside the crate. Tests in
   `crates/teamwire-launch/tests/afk_start.rs`:
   - `afk_start_creates_the_team_copies_the_task_and_takes_the_hold` (row fields, `lead_cwd` in
     `team.created`, task bytes equal, hold recorded owned, no member rows)
   - `afk_start_with_no_lead_process_refuses_and_creates_nothing` (no `team.db`, no team directory)
   - `afk_start_on_a_platform_without_a_hold_creates_the_team_and_warns`
   - `afk_start_refuses_a_session_that_already_leads_an_open_afk_team`
   - `afk_start_refuses_a_taken_team_name`
   - `add_to_an_afk_team_with_no_members_starts_the_member` (guard: `validate(.., add = true, ..)`
     then `launch`, on a team made by the AFK start function; it passes with no change to
     `validate.rs`)
2. Doctor's `launch_checks`. Test in `crates/teamwire-cli/tests/doctor.rs`:
   `launch_checks_fail_on_host_none_and_keep_the_plugin_check`.
3. The CLI command in `crates/teamwire-cli/src/cmd/afk.rs`, and `parse_duration` made `pub(crate)`.
   Tests in `crates/teamwire-cli/tests/afk.rs`, through the binary:
   - `afk_start_outside_a_lead_session_is_refused`
   - `afk_start_refuses_a_bad_name_title_or_budget_and_creates_nothing`
   - `afk_start_refuses_a_missing_or_empty_task_file`
   - `afk_start_refuses_when_a_doctor_check_fails` (a session id set, valid arguments, and a doctor
     FAIL such as the plugin not installed; stderr names the check)
   - `afk_start_prints_the_run_and_exits_zero` (stdout is exactly D12's five lines, exit 0,
     `afk/task.md` byte-equal to the task file, and the team row has `afk_budget_ends_at`)
   - `afk_start_that_cannot_copy_the_task_exits_two_and_names_the_team` (the same setup, but first
     create `<home>/teams/<team>/` and a regular file named `afk` in it; exit 2, stderr holds
     `failed:` and `team <team> exists`, and the row exists)

   The recipe for the last two (D28). The binary must pass `launch_checks` and find a live lead:
   - Doctor's healthy setup: `Bed::healthy()` in `crates/teamwire-cli/tests/doctor.rs` (line 140)
     with `FakeBins::ensure()` installed into a directory put first on `PATH` (then
     `/usr/bin:/bin`), `ignoring_repo()` as the current directory, `HERDR_ENV=1`, `TMUX` removed,
     `--home` and `TEAMWIRE_CLAUDE_DIR` as `teamwire()` at line 802 sets them. You may move that
     setup into a new `crates/teamwire-cli/tests/support/healthy.rs`, which you own, and include it
     from both test files with `#[path]`.
   - A live lead: spawn a child (`std::process::Command::new("sleep").arg("60")`) and kill it at
     the end of the test, also on failure (a guard that kills on drop). Write
     `<claude dir>/sessions/<pid>.json` holding
     `{"pid": <pid>, "sessionId": "<sid>", "kind": "interactive", "cwd": "<dir>"}`, with no
     `startedAt`, and set `CLAUDE_CODE_SESSION_ID=<sid>`. `runs_session`
     (`crates/teamwire-harness-claude/src/session.rs:122-130`) then counts the live pid as the
     session's without a start-time check.
   - On macOS the real sleep hold starts and ends when the child dies; Linux has no hold. Assert
     the stdout lines and the row, not the hold.
   - `team_names` skips a directory without `team.db`, so the file named `afk` blocks only the task
     copy, after the row exists.

## 11. Files you own, and files owned by others

You own: `crates/teamwire-cli/src/cmd/afk.rs`, `crates/teamwire-cli/src/args.rs`,
`crates/teamwire-cli/src/main.rs`, `crates/teamwire-cli/src/cmd/doctor.rs`,
`crates/teamwire-cli/src/cmd/metrics.rs` (only `parse_duration`'s visibility),
`crates/teamwire-cli/tests/afk.rs`, `crates/teamwire-cli/tests/doctor.rs`,
`crates/teamwire-cli/tests/support/healthy.rs`, `crates/teamwire-launch/src/afk.rs`, `crates/teamwire-launch/src/lib.rs`,
`crates/teamwire-launch/src/hold.rs`, `crates/teamwire-launch/src/validate.rs`,
`crates/teamwire-launch/src/plan.rs`, `crates/teamwire-launch/tests/afk_start.rs`,
`crates/teamwire-launch/tests/support/common.rs`.

Owned by others, never edit: `crates/teamwire-launch/src/member.rs` and `plan_rules.rs` (budget;
call `new_team` and `lead_cwd`, never change them);
`crates/teamwire-cli/src/cmd/hook.rs`, `status.rs`, `teams.rs`, `close.rs` and their tests
(stophold, closestat); `crates/teamwire-cli/tests/preset.rs`, `crates/teamwire-harness-claude/`,
`examples/`, `docs/`, `README.md` (skill); `crates/teamwire-store/`, `tests/fakes/`, `Cargo.toml`,
`Cargo.lock`, `scripts/`, `xtask/` (protected or the lead's). For a change in any of them, send the
lead a question and carry on with the rest.

## 12. Verification

Your check, from the worktree root:

```bash
cargo test -p teamwire-cli -p teamwire-launch --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux && scripts/gate.sh fast
```

On afk/base it passes (403 tests, then fmt, tidy and clippy ok). Do not run the full suite,
`scripts/test-changed.sh` or `scripts/gate.sh` without `fast`; the closer runs those on the joined
branch (D23).

## 13. How to finish

Do not push and do not open a pull request. Commit each unit. Write your report with one progress
line per commit, the check's real output, every new `pub` item, and a **Decisions I made** list.
Then run `teamwire done` (or `team_done`).

## 14. Open questions you must not answer

None. Decide anything the brief does not cover, and list it under **Decisions I made**.
