# Plan: AFK mode for teamwire (team afkmode)

## Summary

- Precondition holds: afk/base contains 5b86c1e (`create_afk_team(at, team, budget_ends_at, extra)`).
- Before launch the lead commits one contract amendment on afk/base: `git apply afk/amend-afk-args.patch`
  (the `teamwire afk start` arguments, a stub command, and two helpers in launch's member.rs; D7, D29).
- Wave 1: five implementers, all `base: afk/base`, branches `afkmode/<role>`, no shared files (D6):

| Role | Builds | Design | Size with tests |
| --- | --- | --- | --- |
| start | `teamwire afk start`, doctor's launch checks, `--add` on an empty team | 5.2 | ~520 lines |
| budget | launch refuses implementers past the budget; model-table warnings | 5.4 | ~210 lines |
| stophold | the AFK hold in the lead's Stop hook | 5.3 | ~260 lines |
| closestat | close's REPORT.md check; status line and `afk` object; teams marker and key; zod copy | 5.5 | ~380 lines |
| skill | the AFK skill and its install; every doc, ADR 0025, AFK-1..AFK-12 | 5.6, 6 | ~450 lines, mostly docs |

- Order after wave 1: one reviewer per branch, fix rounds, then merge into `afk/afkmode` in this
  order: budget, start, stophold, closestat, skill. No two branches touch the same file, so the order
  only keeps the CLI changes together. Then the closer, then the reporter.
- Every name, message and output line a second member or a doc depends on is fixed in decisions.md
  D9 to D21 and D28 to D36. Every test the docs cite is fixed below ("Tests and requirement ids").
- Risks, highest first:
  1. `tmux_wiring::herdr_env_wins_over_tmux` fails on afk/base in a tmux pane (D23). Member checks skip
     it; the closer runs the full gate detached from tmux, where it passes ("For the closer", D27).
  2. The start unit is the largest; its binary tests need a live child as the lead (D28).
  3. The skill member writes docs for behaviour four other members build; the reviewer of the skill
     branch must compare docs/api/cli.md with D9 to D19, and the closer with the joined code.
  4. A team with zero members (afk start) meets readers that assume members: status, watch, close,
     reopen, verify, metrics, dashboard. Reviewers check each against an AFK team with no members.
  5. Close refuses an AFK team without a full REPORT.md and nothing overrides it (D17); a user who
     wants out writes the eight headings.

## Contract amendments for the lead

1. Apply and commit on afk/base before launch, then launch with `base: afk/base`:

   ```bash
   cd /private/tmp/claude-501/bnruns/bn4tu2e1us/clone
   git checkout afk/base
   git apply /private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/afk/amend-afk-args.patch
   scripts/gate.sh fast
   git add crates/teamwire-cli/src/args.rs crates/teamwire-cli/src/main.rs crates/teamwire-cli/src/cmd/afk.rs crates/teamwire-launch/src/member.rs
   git commit -m "Add the teamwire afk start arguments and launch's team helpers as a contract for the AFK build"
   ```

   It adds `Command::Afk(AfkArgs)`, `AfkCommand::Start(AfkStartArgs { team: String, title: String,
   task: PathBuf, budget: String /* default "8h" */ })`, the dispatch line in main.rs and
   `crates/teamwire-cli/src/cmd/afk.rs`, whose `run` refuses with exit 1 until start replaces it (D7).
   It also extracts two helpers from `create_team` in `crates/teamwire-launch/src/member.rs`, which
   `create_team` now calls (D29):
   `pub(crate) fn new_team(name: &TeamName, title: &str, lead: &LeadPlan) -> NewTeam` and
   `pub(crate) fn lead_cwd(registry: &Registry, lead: &LeadPlan) -> Option<String>`.
   start calls both; budget owns member.rs and leaves them alone.
   Checked on a fresh clone of afk/base: `scripts/gate.sh fast` ok (fmt, tidy, clippy);
   `cargo test -p teamwire-launch -p teamwire-cli --locked --no-fail-fast -- --skip
   herdr_env_wins_over_tmux` 403 passed; `teamwire afk start --help` shows the four flags.
   `git apply --check` on the repository's afk/base: clean. tidy prints
   `WEAKENED crates/teamwire-cli/src/cmd/afk.rs:14` for the stub's `#[expect(` and still exits 0.
2. Nothing else. doctor.rs moved to start (D8); launch's `ensure_hold`, `lead_plan` and title check
   are reached from inside the launch crate (D13); member.rs's helpers come from item 1 (D29).

## Units

Each unit's full brief is `afk/briefs/<role>.md`. What follows is what the lead and the reviewers need
to see the whole.

### start (design 5.2)

`teamwire afk start --team NAME --title TITLE --task FILE [--budget 8h]`. The CLI file parses, finds
the lead session through wiring, runs doctor's `launch_checks` and prints. The steps that touch the store,
the platform and the hold live in `crates/teamwire-launch/src/afk.rs`, a library function tested with
FakePlatform and FakeHarness. Order and codes: D9. Budget: D10. lead_cwd: D11. Output: D12.
The team row's fields and `lead_cwd` come from `member::new_team` and `member::lead_cwd`, which the
contract amendment adds (D29). Two binary tests run the success output and the exit-2 path, with a
spawned child as the lead's process (D28).
`--add` already accepts an open team with no members (validate.rs has no such refusal; read_existing
returns the row, `(Some(e), true)` passes when the state is open); start proves it with a guard test.
`no_lead_process` is checked before `create_afk_team`: `platform.sleep_hold().is_some()` and the lead
harness's `processes_for_session` returns none or fails.

### budget (design 5.4)

`launch_all` in `crates/teamwire-launch/src/member.rs` refuses each implementer past
`afk_budget_ends_at` with `afk_budget_spent` and records nothing for it (D14). `planning_warnings` in
`plan_rules.rs` adds one warning per AFK member below the table (D15). member.rs already holds the
contract's `new_team` and `lead_cwd`, which budget leaves alone (D29).

### stophold (design 5.3)

`teamwire_report::afk_hold_notes(home, session, platform, clock) -> Vec<String>`, beside
`teams_without_watch` and `watch_missing_note`. "Nothing will wake the lead" means no open team led by
`session` has both a started member with no completion for its current generation and a live watch
lease (the same per-member test `needs_watch` uses). Notes: D16, and D31 past the budget. `lead_stop_in` in hook.rs computes the
watch teams and the AFK notes inside the one `within(LEAD_STOP_BUDGET, ...)` call and replies when
either is non-empty. A team whose store cannot be read gives no AFK note.

### closestat (design 5.5)

Close: `crates/teamwire-close/src/afk_report.rs`, called from `team_plan` in plan.rs (D17), so the
dry run, `close_observed` and the `close.refused` event all see it. Status: `AfkView` in
teamwire-watch, filled in status.rs from the team row and `afk/decisions.md` (D18, D35); the CLI line
in status.rs. Teams: `TeamEntry.afk_budget_ends_at` in teamwire-msg's describe(), the `(open, afk)`
text in teams.rs (D19). zod: D20, committed with each `--json` change it copies (D30).

### skill (design 5.6, 6, and the docs)

`examples/skills/afk/SKILL.md` from section 6, every rule kept, adjusted to D9, D10, D12, D17, D21,
D31 and D35. Rendered as a second stamped skill in the Claude plugin, tested in
`tests/lead_setup.rs` (D32). The pointer line in the main skill (D36). ADR 0025
(the design's sections 3 to 5 as decisions and the options they beat, including the rejected separate
AFK home). docs/requirements.md "AFK runs (AFK)" AFK-1..AFK-12 and docs/failure-modes.md rows, both from
the table below. docs/api/cli.md, core.md, mcp.md, ARCHITECTURE.md, README.md and CONTEXT.md.

## Owned files (exhaustive; a file not listed here is owned by nobody, so a member asks the lead)

| Role | Owns |
| --- | --- |
| start | `crates/teamwire-cli/src/cmd/afk.rs`, `crates/teamwire-cli/src/args.rs`, `crates/teamwire-cli/src/main.rs`, `crates/teamwire-cli/src/cmd/doctor.rs`, `crates/teamwire-cli/src/cmd/metrics.rs`, `crates/teamwire-cli/tests/afk.rs`, `crates/teamwire-cli/tests/doctor.rs`, `crates/teamwire-cli/tests/support/healthy.rs`, `crates/teamwire-launch/src/afk.rs`, `crates/teamwire-launch/src/lib.rs`, `crates/teamwire-launch/src/hold.rs`, `crates/teamwire-launch/src/validate.rs`, `crates/teamwire-launch/src/plan.rs`, `crates/teamwire-launch/tests/afk_start.rs`, `crates/teamwire-launch/tests/support/common.rs` |
| budget | `crates/teamwire-launch/src/member.rs`, `crates/teamwire-launch/src/plan_rules.rs`, `crates/teamwire-launch/tests/afk_budget.rs` |
| stophold | `crates/teamwire-report/src/lead_stop.rs`, `crates/teamwire-report/src/afk_hold.rs`, `crates/teamwire-report/src/lib.rs`, `crates/teamwire-report/tests/lead_stop.rs`, `crates/teamwire-cli/src/cmd/hook.rs`, `crates/teamwire-cli/tests/lead_hook.rs` |
| closestat | `crates/teamwire-close/src/afk_report.rs`, `crates/teamwire-close/src/plan.rs`, `crates/teamwire-close/src/lib.rs`, `crates/teamwire-close/tests/afk_report.rs`, `crates/teamwire-close/tests/support/rig.rs`, `crates/teamwire-watch/src/lib.rs`, `crates/teamwire-watch/src/status.rs`, `crates/teamwire-watch/src/afk.rs`, `crates/teamwire-watch/tests/status.rs`, `crates/teamwire-watch/tests/support/fixture.rs`, `crates/teamwire-msg/src/directory.rs`, `crates/teamwire-cli/src/cmd/status.rs`, `crates/teamwire-cli/src/cmd/teams.rs`, `crates/teamwire-cli/tests/status.rs`, `crates/teamwire-cli/tests/teams.rs`, `crates/teamwire-cli/tests/close.rs`, `ui/src/shared/teamwire.ts`, `ui/src/shared/teamwire.test.ts` |
| skill | `examples/skills/afk/SKILL.md`, `examples/skills/teamwire/SKILL.md`, `crates/teamwire-harness-claude/src/lead_setup/skill.rs`, `crates/teamwire-harness-claude/src/lead_setup/plugin.rs`, `crates/teamwire-harness-claude/src/lead_setup.rs`, `crates/teamwire-harness-claude/src/lib.rs`, `crates/teamwire-harness-claude/tests/lead_setup.rs`, `crates/teamwire-cli/tests/preset.rs`, `docs/adr/0025-afk-runs.md`, `docs/requirements.md`, `docs/failure-modes.md`, `docs/api/cli.md`, `docs/api/core.md`, `docs/api/mcp.md`, `docs/ARCHITECTURE.md`, `README.md`, `CONTEXT.md` |

The files that read or pin what each unit changes, and why each is in that member's list:

| File | Pins or reads | Owner |
| --- | --- | --- |
| `crates/teamwire-cli/tests/doctor.rs` | compiles `src/cmd/doctor.rs` by `#[path]` with a stub `Wiring` (fields platform, claude, codex, registry, clock), so a doctor change that names any other crate item breaks it | start |
| `crates/teamwire-cli/src/cmd/metrics.rs` | holds `parse_duration`, reused for `--budget` | start |
| `crates/teamwire-launch/tests/support/common.rs` | the launch tests' rig; budget reads it and must not edit it | start |
| `crates/teamwire-cli/tests/support/healthy.rs` | a new file, if start moves doctor's healthy setup there for its binary tests (D28) | start |
| `crates/teamwire-launch/src/member.rs` (`new_team`, `lead_cwd`) | start's `afk.rs` calls both; the contract amendment adds them (D29) | budget |
| `crates/teamwire-harness-claude/tests/lead_setup.rs:184` | `assert_eq!(files.len(), 6)` on the rendered plugin, 7 after the AFK skill; the AFK-12 install and doctor tests (D32) | skill |
| `crates/teamwire-harness-claude/src/lead_setup/plugin.rs` tests | the exact list of rendered plugin paths | skill |
| `crates/teamwire-cli/tests/preset.rs` | `the_skill_names_only_commands_and_tools_that_exist` reads the main skill; the new AFK skill test sits beside it | skill |
| `crates/teamwire-cli/tests/teams.rs`, `status.rs`, `close.rs` | the text and JSON of teams, status, close | closestat |
| `crates/teamwire-mcp` (`team_status`, `team_teams`) | returns the same JSON; its tests check single keys only (`tests/teams.rs:74-84`), so it needs no change | nobody; closestat's check runs its tests |
| `ui/src/shared/teamwire.ts` and its test | the zod copy of `status --json` and `teams --json` | closestat |
| docs/api/*.md, docs/requirements.md, docs/failure-modes.md, README.md, ARCHITECTURE.md, CONTEXT.md | describe the commands and the plugin's files; no test pins them (`grep -rn 'cli.md' crates tests xtask` finds only comments) | skill |
| `tests/e2e`, `tests/crash` | run the binary on ordinary teams, whose output gains only `"afk": null` and `"afk_budget_ends_at": null` | nobody; the closer's full gate runs them |
| Codex plugin (`crates/teamwire-harness-codex/src/lead_setup/plugin.rs`) | its own render; a Codex lead is out of scope | nobody |

## Tests and requirement ids

Each member writes these test functions with exactly these names, and may add more. The skill
member's failure-modes rows and requirement rows cite them. A test marked (guard) pins today's
behaviour and passes on afk/base too; every other test fails without its change (D33).

| Id | Level | Requirement | Tests (`path::function`) | Member |
| --- | --- | --- | --- | --- |
| AFK-1 | MUST | `teamwire afk start` refuses outside a lead session (`no_lead_session`), from a background session (`background_lead`), and when the session already leads an open AFK team (`afk_already_running`). Nothing is created. | `crates/teamwire-cli/tests/afk.rs::afk_start_outside_a_lead_session_is_refused`; `crates/teamwire-launch/tests/afk_start.rs::afk_start_refuses_a_session_that_already_leads_an_open_afk_team` | start |
| AFK-2 | MUST | It checks the team name and title with launch's rules, refuses a taken name, a bad budget, and a missing or empty task file. Nothing is created. | `crates/teamwire-cli/tests/afk.rs::afk_start_refuses_a_bad_name_title_or_budget_and_creates_nothing`; `crates/teamwire-cli/tests/afk.rs::afk_start_refuses_a_missing_or_empty_task_file`; `crates/teamwire-launch/tests/afk_start.rs::afk_start_refuses_a_taken_team_name` | start |
| AFK-3 | MUST | It runs doctor's launch checks; a FAIL refuses with `doctor_failed` naming the check; a warn is printed and does not refuse; host none is a FAIL. | `crates/teamwire-cli/tests/afk.rs::afk_start_refuses_when_a_doctor_check_fails`; `crates/teamwire-cli/tests/doctor.rs::launch_checks_fail_on_host_none_and_keep_the_plugin_check` | start |
| AFK-4 | MUST | It creates the team with launch's lead fields, `afk_budget_ends_at` and `lead_cwd`, and no members; copies the task to `afk/task.md` byte for byte; starts or reuses the sleep hold and records it. | `crates/teamwire-launch/tests/afk_start.rs::afk_start_creates_the_team_copies_the_task_and_takes_the_hold`; `crates/teamwire-launch/tests/afk_start.rs::afk_start_on_a_platform_without_a_hold_creates_the_team_and_warns`; `crates/teamwire-cli/tests/afk.rs::afk_start_prints_the_run_and_exits_zero`; `crates/teamwire-cli/tests/afk.rs::afk_start_that_cannot_copy_the_task_exits_two_and_names_the_team` | start |
| AFK-5 | MUST | When the platform needs a sleep hold and the lead's process cannot be found, it refuses with `no_lead_process` before anything is created. | `crates/teamwire-launch/tests/afk_start.rs::afk_start_with_no_lead_process_refuses_and_creates_nothing` | start |
| AFK-6 | MUST | `teamwire launch --add` accepts an open AFK team with no members. | `crates/teamwire-launch/tests/afk_start.rs::add_to_an_afk_team_with_no_members_starts_the_member` (guard) | start |
| AFK-7 | MUST | Past `afk_budget_ends_at`, launch refuses each new implementer with `afk_budget_spent` and starts every other kind; a team without the column is unaffected. | `crates/teamwire-launch/tests/afk_budget.rs::past_the_budget_an_implementer_is_refused_and_a_reviewer_starts`; `::before_the_budget_ends_an_implementer_starts`; `::a_team_without_a_budget_is_unaffected` (guard) | budget |
| AFK-8 | SHOULD | Launch warns once per member of an AFK team whose model or effort is below the AFK table. | `crates/teamwire-launch/tests/afk_budget.rs::members_below_the_afk_table_get_one_warning_each`; `::an_ordinary_team_gets_no_model_warning` (guard) | budget |
| AFK-9 | MUST | The lead's Stop hook holds once per stop chain for each open AFK team the session leads when nothing will wake the lead, with the AFK note (budget-spent wording past the budget), in one reply with any watch note; it writes nothing and never fails the turn. | `crates/teamwire-report/tests/lead_stop.rs::an_afk_team_with_nothing_to_wake_the_lead_gets_the_afk_note`; `::a_live_watch_on_a_working_team_means_no_afk_note`; `::a_working_member_without_a_watch_still_gets_the_afk_note`; `::past_the_budget_the_afk_note_says_budget_spent_after_the_team_tag`; `::an_ordinary_or_closed_team_or_another_session_gets_no_afk_note`; `crates/teamwire-cli/tests/lead_hook.rs::an_afk_team_holds_the_stop_once_with_the_afk_note`; `crates/teamwire-cli/tests/lead_hook.rs::the_afk_note_and_the_watch_note_come_in_one_reply` | stophold |
| AFK-10 | MUST | Close refuses an AFK team whose `afk/REPORT.md` is missing or lacks one of the eight headings, with `afk_report_incomplete`; `--role` is unaffected. | `crates/teamwire-close/tests/afk_report.rs::close_refuses_an_afk_team_without_its_report`; `::close_refuses_an_afk_report_that_lacks_a_heading_and_names_it`; `::close_of_an_afk_team_with_a_full_report_archives_the_afk_files`; `::removing_one_member_of_an_afk_team_needs_no_report` (guard); `::an_ordinary_team_closes_without_a_report` (guard); `crates/teamwire-cli/tests/close.rs::close_of_an_afk_team_without_a_report_prints_the_refusal_code` | closestat |
| AFK-11 | MUST | `status <team>` shows the AFK line and `--json` an `afk` object (null for an ordinary team); `teams` marks an AFK team and `teams --json` gives `afk_budget_ends_at`. | `crates/teamwire-watch/tests/status.rs::an_afk_team_shows_its_budget_and_decision_count`; `::a_spent_budget_shows_zero_seconds_left`; `::an_ordinary_team_has_no_afk_view`; `crates/teamwire-cli/tests/status.rs::status_of_an_afk_team_prints_the_afk_line_and_json_object`; `crates/teamwire-cli/tests/teams.rs::teams_marks_an_afk_team_and_its_json_gives_the_budget_end`; `ui/src/shared/teamwire.test.ts` ("afk fields" block) | closestat |
| AFK-12 | MUST | `teamwire install` renders the AFK skill as a second skill of the plugin, doctor's plugin check covers it, and the main skill points at it. | `crates/teamwire-harness-claude/tests/lead_setup.rs::install_writes_the_afk_skill_as_a_second_stamped_skill`; `crates/teamwire-harness-claude/tests/lead_setup.rs::a_plugin_without_the_afk_skill_or_with_an_edited_one_is_a_problem`; `crates/teamwire-cli/tests/preset.rs::the_afk_skill_names_only_commands_and_tools_that_exist`; `crates/teamwire-cli/tests/preset.rs::the_afk_skill_lists_the_eight_report_headings`; `crates/teamwire-cli/tests/preset.rs::the_main_skill_points_at_the_afk_skill` | skill |

## Checks (each run once by the planner, see "Planning check" item 1)

Each check runs the crates its member changes and every crate that depends on them (D34): launch <-
cli; report <- cli, mcp; close <- cli; watch and msg <- cli, mcp; harness-claude <- cli.

| Role | check |
| --- | --- |
| start | `cargo test -p teamwire-cli -p teamwire-launch --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux && scripts/gate.sh fast` |
| budget | `cargo test -p teamwire-launch -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux && scripts/gate.sh fast` |
| stophold | `cargo test -p teamwire-report -p teamwire-mcp -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux && scripts/gate.sh fast` |
| closestat | `cargo test -p teamwire-close -p teamwire-watch -p teamwire-msg -p teamwire-mcp -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux && (cd ui && bun test && bun run typecheck) && scripts/gate.sh fast` |
| skill | `cargo test -p teamwire-harness-claude -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux && scripts/check-docs.sh && scripts/gate.sh fast` |

## Notes for the reviewers and the closer

- Zero-member AFK team: `status` (members empty), `watch` (TEAM-COMPLETE needs one settled member,
  watch/src/plan.rs:233), `close` (the `empty.rs` path needs no team row, so an AFK team takes the
  normal path), `reopen`, `verify`, `metrics`, the dashboard.
- Do not revert-check a test marked (guard) in "Tests and requirement ids": it passes on afk/base by
  design (D33).
- The design's "Live check, with the user present" (cargo install, teamwire install, a real
  `/teamwire:afk` run) is out of scope for this run.

### For the closer: run the full gate detached from tmux (D27)

Run the full gate this way, from the joined branch's checkout. The `rm` removes the log of an
earlier run, so the wait cannot read an old `gate exit` line. The second command returns at once;
wait for the log's last line in the background (a background Bash loop or Monitor).

```bash
cd <your checkout of the joined branch>
rm -f /private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/afk/closer-gate.log
( { scripts/gate.sh; echo "gate exit $?"; } > /private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/afk/closer-gate.log 2>&1 & )
until grep -q '^gate exit' /private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/afk/closer-gate.log; do sleep 10; done
```

Why: inside tmux, gate step 4 stops at `tmux_wiring::herdr_env_wins_over_tmux`, cargo skips every
later test target (e2e, crash, mcp and more), and steps 5 and 6 never run. The double fork makes
launchd (pid 1) the gate's parent, so no tmux is among the test's parents and all six steps run. The
planner ran exactly this from a tmux pane on afk/base plus the final amendment: all six steps ok
(test 236 s) and `gate exit 0`. A step that fails in this run is a real failure. Run no `--skip` here.

## Planning check (the main skill's eight items)

All runs below were made by the planner on 2026-10-27 in a fresh `git clone --branch afk/base` of the
repository, in the planner's scratchpad (D26), with a local `main` branch made from `origin/main` so
that tidy's `--base main` resolves.

| # | Item | Result |
| --- | --- | --- |
| 1 | Each implementer has a runnable check, run once | Pass. Each check of the "Checks" table ran on afk/base plus the amendment patch, exit 0: start 403 tests passed, then gate fast ok; budget 403 passed (launch and cli, D34; rerun on the final patch), gate fast ok; stophold 405 passed (report, mcp and cli, D34; rerun on the final patch), gate fast ok; closestat 562 passed, `bun test` 162 passed (3 skipped, 0 failed), typecheck ok, gate fast ok; skill 434 passed, `docs check: ok`, gate fast ok. |
| 2 | Each check covers what CI covers | Pass. The repository has no CI service (no `.github/`); its CI is `scripts/gate.sh`. Member checks run gate steps 1 to 3 (fmt, tidy with `--base main`, clippy over the workspace, all targets) plus the tests of their crates and of every crate that depends on them (D34); skill adds step 6 (`check-docs.sh`); closestat adds the ui commands of ui/AGENTS.md. Steps 4 (workspace tests, including `tests/e2e` and `tests/crash`) and 5 (`cargo doc` with `-D warnings`) run in the closer's full gate on the joined branch, detached from tmux so that no test is skipped and every step runs (D27, "For the closer"). That way of running it passed all six steps on afk/base plus the amendment. |
| 3 | Each check passes in a fresh checkout of the base after `setup` | Pass. A cold `cargo build --workspace --all-targets --locked` took 21 s (launch's setup limit is 600 s); `bun install --frozen-lockfile` in ui/ took 34 ms. A first run of the start check failed at `tmux_wiring::herdr_env_wins_over_tmux` (exit 101, `left: Some(2) right: Some(0)`), and cargo then skipped every later test target; the cause is a tmux parent process, not the code (D23). The checks now skip that one test by name and use `--no-fail-fast`. |
| 4 | No two members have overlapping `owns` | Pass. 59 owned paths across five members, no path twice (checked by script when writing spec-wave1.json). `teamwire launch --dry-run` of the spec, with the clone's binary against an empty temporary home: exit 0, five members planned, no overlap warning, nothing created. The planner itself is a pane member with no `owns`. |
| 5 | No file the work must change is unowned | Pass. The readers and pinners are in the table under "Owned files": tests/doctor.rs compiles doctor.rs by `#[path]`; start's binary tests may share doctor's healthy setup through tests/support/healthy.rs, which start owns; start calls member.rs's helpers, which the amendment adds, so start never edits budget's file; tests/lead_setup.rs:184 counts six plugin files; plugin.rs's test lists the exact paths; preset.rs runs `teamwire --help`; the CLI tests of status, teams and close; the zod copy; every doc. `teamwire-mcp` returns the changed JSON but its tests read single keys, so it needs no owner; closestat's check runs them. tests/e2e and tests/crash see only ordinary teams. |
| 6 | Each content rule has one worked example | Pass, settled by the planner because no user is present. Each text a user or a second member reads has its exact form in decisions.md: refusal output (D9, with the `bad_name` example in start's brief), success lines (D12), the budget refusal (D14), the model warning (D15, two examples in budget's brief), the hold notes (D16, and D31 past the budget), close's refusal and the heading rule (D17, two examples in closestat's brief), the status line and the decision count (D18, D35, two examples), the teams marker (D19), the pointer line (D36). |
| 7 | Briefs leave out what the harness or session already sets | Pass. No brief sets a commit trailer, a model, an effort or a plugin; the spec carries model, effort and plugins (D4, D5). |
| 8 | Build and test commands live once in the repository's instruction files | Partly. AGENTS.md holds the commands, and the briefs give only each member's targeted check. The tmux test failure (D23) and the detached full gate (D27) are new; the briefs, the checks and "For the closer" carry them, and AGENTS.md does not. Recommendation for the lead: add one line to AGENTS.md's Commands section in a later commit: "Inside tmux, `herdr_env_wins_over_tmux` fails; skip it by name in a targeted check, and run the full gate detached: `( { scripts/gate.sh; echo "gate exit $?"; } > gate.log 2>&1 & )`". The briefs work without it. |
