# Closer report: afkmode

## Summary
1. The joined branch `afk/afkmode` (5d2bbd5) fails the full gate as merged. Clippy exits 1 because 15 test targets do not compile.
2. The cause is six test helpers that still call `create_afk_team` with three arguments. The `start` branch made it take four. Decision D21 planned a one-line fix per helper after the merge. Nobody applied the fix. Owners: endings (4 sites), stophold (2 sites). Grade: wrong conclusion.
3. With that fix applied in my clone only, the joined branch passes fmt, tidy, clippy, cargo doc, `scripts/check-docs.sh`, `bun test` and `bun run typecheck`. The workspace tests give 1727 passed and 1 failed.
4. The one failing test is `herdr_env_wins_over_tmux`. It fails on `main` (fb1fd8f) the same way, so it comes from this machine's HERDR_ENV and TMUX, not from this team.
5. Each member branch passes alone. `scripts/gate.sh` exits 1 on every branch, but only at that same environment test. Every other test passes with it skipped, and each member's own check exits 0.
6. `teamwire verify afkmode` gives PASS for all four members, and the lead had already waived every flag. Verify checks each branch alone, so it cannot see the compile break on the joined branch.
7. I broke one fix at a time in the clone for 36 checks across the four branches. In every case, the member's new test failed as it should. No new test passes without its fix.
8. Every end-state item and every name the task fixes is in the code of the joined branch (tables below). "Is tested" holds only after the six-line fix, because the tests for design 5.3 and 5.5 do not compile without it.
9. The docs owned by `skill` are out of date in two places. They say `lead_cwd` is always recorded, which changed after review (wrong detail). They say close prints `refused: afk_report_incomplete`, but close prints `REFUSED: afk_report_incomplete: ...` (wording).
10. No branch adds a `shortcut:` comment, a TODO or a FIXME.
11. Not verified: the design's live check with a real lead. It needs `cargo install` and `teamwire install`, which the AFK boundaries forbid.

## What was merged
`afk/afkmode` = `main` fb1fd8f plus four merges, with no extra commits:

| Merge | Member branch head |
| --- | --- |
| a67076c | afkmode/start c774aed (7 commits) |
| f90d60a | afkmode/endings 0ccf0c3 (4 commits) |
| 2c30913 | afkmode/stophold e5e8e23 (4 commits) |
| 5d2bbd5 | afkmode/skill 73505c1 (8 commits) |

## Findings, most important first

### F1. The joined branch does not compile its tests. Owners: endings and stophold. Grade: wrong conclusion.
- The `start` branch changed the store call to `create_afk_team(at, team, budget_ends_at, extra)` at crates/teamwire-store/src/team.rs:151.
- These test helpers on the joined branch still pass three arguments:
  - crates/teamwire-close/tests/afk.rs:33 (endings)
  - crates/teamwire-cli/tests/teams.rs:92 (endings)
  - crates/teamwire-mcp/tests/teams.rs:76 (endings)
  - crates/teamwire-watch/tests/support/fixture.rs:77 (endings). The cli tests for status, watch, reopen and export also include this file.
  - crates/teamwire-report/tests/afk_hold.rs:60 (stophold)
  - crates/teamwire-cli/tests/lead_hook.rs:73 (stophold)
- `scripts/gate.sh` on 5d2bbd5 stops at step 3 with `error[E0061]: this method takes 4 arguments but 3 arguments were supplied`. `cargo clippy --keep-going` lists 15 test targets that fail to compile.
- The fix is to add `&serde_json::Value::Null` as the fourth argument at each site, then run `cargo fmt`. The patch is at /private/tmp/claude-501/-private-tmp-claude-501-bnruns-bnei8xdqkf-clone/c508a262-3376-4dd3-b4c6-37bb4657d032/scratchpad/d21-fix.patch. With it, every gate step passes except the environment test (table below).
- The fix belongs on the integration branch or on the two member branches, as D21 says. I changed nothing outside my clone.

### F2. The docs say `lead_cwd` is always recorded. Owner: skill. Grade: wrong detail.
- docs/requirements.md:303 (AFK-2) says a successful start puts "`lead_cwd` and the budget end in `team.created`".
- docs/adr/0025-afk-runs.md:43 says the event "carries the lead's working directory".
- docs/api/core.md:195-196 says "`teamwire afk start` passes `{"lead_cwd": <the lead's working directory>}`".
- Since c774aed (start's review fix), the code records the folder that the harness reports for the lead session. The code passes `{}` when the harness cannot name that folder (crates/teamwire-cli/src/cmd/afk.rs:209-215 and :243-246). The skill docs were written before that commit (f37f9cc at 23:25, c774aed at 23:28).
- The fix is one clause in each of the three docs, such as "`lead_cwd`, when the harness can name the session's folder". docs/api/core.md:182 already says this for launch.

### F3. The docs show the close refusal in lowercase. Owner: skill. Grade: wording.
- docs/requirements.md:308 (AFK-7) and docs/adr/0025-afk-runs.md:69 say close refuses "with `refused: afk_report_incomplete`".
- Close prints `REFUSED: afk_report_incomplete: <path> is missing` or `... lacks: ...` (crates/teamwire-cli/src/cmd/close.rs:113, text from crates/teamwire-close/src/afk.rs:33 and :58). D5 chose that form.
- docs/api/cli.md gives the real text, so only these two lines differ.

### F4. The skill report blames the wrong branch for a timing test. Owner: skill. Grade: wrong detail.
- reports/skill.md says `locked_stores_let_the_stop_through_at_the_bound` is "a timing test from the stophold branch that is not in my branch's diff".
- The test is on `main` at crates/teamwire-cli/tests/lead_hook.rs:175, and on the skill branch unchanged.
- The test passed in every run I made: the joined branch with the fix, and all four member branches. That is six runs, and none was flaky.

### F5. The verify check for skill does not run the Codex test that skill added. Owner: the lead (the `check` of skill in afk/spec-wave1.json). Grade: wrong detail.
- After D25, skill changed crates/teamwire-harness-codex/src/lead_setup/plugin.rs and added crates/teamwire-harness-codex/tests/lead_skill.rs.
- The spec check is `cargo test -p teamwire-harness-claude -p teamwire-cli ... && scripts/check-docs.sh`, so the verify PASS for skill does not cover the Codex crate.
- I ran `cargo test -p teamwire-harness-claude -p teamwire-harness-codex -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux` on skill. It exits 0 with 505 passed. The full gate on the joined branch also runs it. So the conclusion holds, and only the verify record is narrower than it looks.

## Where two members' work meets
- start and endings or stophold: F1. The store change on `start` breaks six test helpers on the other two branches. Each branch passes alone, but the joined branch fails.
- start and skill: F2. The late `lead_cwd` change on `start` is not in the docs that `skill` wrote earlier.
- start and endings, the half-made team that rev_start found. `start` now copies the task before it writes the team row (crates/teamwire-cli/src/cmd/afk.rs:216-224), so a failed copy leaves no row. Test `a_task_copy_that_fails_leaves_no_team_behind` fails without the fix (checked).
  - One risk is left. If the process dies after the row is written, the lead holds an open AFK team. Close refuses that team with `afk_report_incomplete`, and `afk start` refuses with `afk_already_running`.
  - The only way out is a hand-written REPORT.md with all eight headings. Close has no override flag.
  - This is a rare crash window, so I record it as a risk, not a finding.
- stophold and endings: the hold note, the close check and the status view all use `<home>/teams/<team>/afk/`. The paths are crates/teamwire-report/src/afk_hold.rs:80, crates/teamwire-close/src/afk.rs:23-28 and crates/teamwire-watch/src/afk.rs:22-27. `afk start` writes the same folder (cmd/afk.rs:218). The paths agree.
- Budget spent: launch (launch afk.rs:61), the hold (afk_hold.rs:81) and status (watch afk.rs:33) all use `now >= ends`. The three agree.
- The skill text and close: the skill tells the reporter to write `1. **What you asked**: ...`. Close accepts that form. Test `numbered_and_bold_headings_count` fails when the `**` stripping is reverted (checked).

## Did each member do its task?

| Member | Verdict | Evidence |
| --- | --- | --- |
| start | Did all of it. | Each refusal code has a test and is checked before the first write (cmd/afk.rs:88-149). Launch budget is at member.rs:59-68 and plan_rules.rs:82-90. The model warning is at launch afk.rs:36-53. The store `extra` merge is at team.rs:151-167. The `--add` test into a memberless team exists. Additions: `check_new_team` in launch (launch afk.rs:69-83) and the code `bad_task`. |
| stophold | Did all of it. | `afk_hold_notes` is at afk_hold.rs:24-52, exported at lib.rs:30 beside lib.rs:32. The hook join is at hook.rs:199-215. Every test the brief lists exists, and the review finding about the unreadable team is fixed (checked by revert). |
| endings | Did all of it, except the D21 follow-up (F1). | The close check is at close afk.rs and plan.rs:73. The status view is at watch afk.rs and status.rs:64. The status line is at cli status.rs:94-120. The teams mark is at teams.rs:44-56. JSON keys are at msg directory.rs:21,50. The ui schemas are at teamwire.ts:121-138,170. It made units 3 and 4 one commit and recorded that choice. |
| skill | Did all of it, with two doc details off (F2, F3). | The skill matches design section 6 word for word: my `diff` shows one re-wrapped line, and the same words. The plugin render is at plugin.rs:90. The pointer line is at examples/skills/teamwire/SKILL.md:18. The Codex drop is at codex plugin.rs:101. ADR 0025 and AFK-1 to AFK-12 exist, and every test file they cite exists. |

## End state of the task, on the joined branch

| Item | Delivered | Where |
| --- | --- | --- |
| `teamwire afk start` (5.2) | yes | args.rs:37, main.rs:71, cmd/afk.rs:18-253 |
| The AFK hold in the lead's Stop hook (5.3) | yes; tests compile only after the F1 fix | teamwire-report/src/afk_hold.rs:24-90, cli cmd/hook.rs:199-215 |
| The budget and model table in launch (5.4) | yes | launch afk.rs:36-65, member.rs:59-68, plan_rules.rs:82-90 |
| Close and status for AFK teams (5.5) | yes; tests compile only after the F1 fix | close afk.rs:21-91 and plan.rs:73; watch afk.rs, lib.rs:157, status.rs:64; cli status.rs:94-120, teams.rs:44-56; msg directory.rs:21,50; ui teamwire.ts:121-138,170 |
| The AFK skill, installed like the other skills (5.6, 6) | yes | examples/skills/afk/SKILL.md; harness-claude skill.rs:21, plugin.rs:90; main SKILL.md:18 |

## Names the task fixes, on the joined branch

| Name | Delivered | Where |
| --- | --- | --- |
| refusal `no_lead_session` | yes | cmd/afk.rs:105-111 |
| refusal `no_lead_process`, with nothing created | yes | cmd/afk.rs:142-149; launch hold.rs:69-80 |
| refusal `afk_already_running` | yes | cmd/afk.rs:115-120 |
| refusal `doctor_failed`, naming the check | yes | cmd/afk.rs:130-141 (`<check name>: <detail>`) |
| `afk_hold_notes(home, session, platform, clock) -> Vec<String>` beside `teams_without_watch` | yes | report afk_hold.rs:24-29, lib.rs:30 and :32 |
| `create_afk_team(at, team, budget_ends_at, extra)`, `extra` merged into `team.created` | yes | store team.rs:151-167 |
| `afk start` passes `{"lead_cwd": ...}` | yes, when the harness names the session's folder; `{}` otherwise (start's review decision) | cmd/afk.rs:209-246 |
| `StatusView.afk: Option<AfkView>`, fields `budget_ends_at`, `seconds_left`, `spent`, `decisions` | yes; `status --json` uses the same names | watch lib.rs:157, afk.rs:10-16 |
| `teams --json` key `afk_budget_ends_at`, a timestamp or `null` | yes | msg directory.rs:21,50 |
| zod schemas accept `afk` and `afk_budget_ends_at`, and older output | yes, as `.nullish()` | ui/src/shared/teamwire.ts:138,170; test "older status output without afk still parses" |

## Checks I ran (all in my clone at $TEAMWIRE_HOME/cache/closer/afkmode/clone)
In the tables below, "workspace tests" means `TEAMWIRE_FAKES_PREBUILT=1 cargo test --workspace --locked --no-fail-fast`. I ran it after `scripts/gate.sh` stopped at its first failing test binary. "doc" means `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked`.

### Joined branch and main

| Where | Command | Exit | Notes |
| --- | --- | --- | --- |
| joined 5d2bbd5, as merged | `scripts/gate.sh` | 1 | fmt ok, tidy ok, clippy FAILED (F1) |
| joined + F1 fix | `scripts/gate.sh` | 1 | fmt, tidy, clippy ok; the test step stops at `herdr_env_wins_over_tmux` |
| joined + F1 fix | workspace tests, nothing skipped | 101 | 1727 passed, 1 failed: `herdr_env_wins_over_tmux` |
| joined + F1 fix | doc | 0 | |
| joined + F1 fix | `scripts/check-docs.sh` | 0 | `docs check: ok` |
| joined + F1 fix | `cargo clippy --workspace --all-targets --locked -- -D warnings` (run again) | 0 | |
| joined (the F1 fix touches no ui file) | `cd ui && bun test` | 0 | 163 pass, 3 skip, 0 fail |
| joined | `cd ui && bun run typecheck` | 0 | |
| joined 5d2bbd5, as merged | `cargo test -p teamwire-cli --locked --test tmux_wiring herdr_env_wins_over_tmux` | 101 | `left: Some(2) right: Some(0)`, with "could not run tmux: not_found" |
| main fb1fd8f | the same | 101 | the same failure, so it exists on the base |

### Member branches

| Branch | setup | `gate.sh` | workspace tests, herdr test skipped | doc | check-docs | member check | other | `gate.sh fast` |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| start c774aed | 0 | 1 (herdr test only) | 0 (1693 passed) | 0 | 0 | 0 (555 passed) | | 0 |
| endings 0ccf0c3 | 0 | 1 (herdr test only) | 0 (1688 passed) | 0 | 0 | cargo part 0 (577 passed) | `bun test` 0 (163 pass, 3 skip), `bun run typecheck` 0 | 0 |
| stophold e5e8e23 | 0 | 1 (herdr test only) | 0 (1688 passed) | 0 | 0 | 0 (385 passed) | | 0 |
| skill 73505c1 | 0 | 1 (herdr test only) | 0 (1677 passed) | 0 | 0 | cargo part 0 (437 passed) | the claimed check with `-p teamwire-harness-codex`: 0 (505 passed) | 0 |

Setup is `cd "$CLONE/ui" && bun install --frozen-lockfile`, run once per branch.

`teamwire verify afkmode` exits 0 ("clean"). start, stophold, endings and skill are PASS. Each flag was already waived:
- start: 9 WEAKENED flags for `#[expect(` and `#![expect(`, at cmd/afk.rs:27,185 and in the three new test files.
- stophold: 1 WEAKENED flag.
- endings: 3 WEAKENED flags.
- skill: 2 outside-owns flags for the Codex files that D25 gave to skill.

The two production `#[expect]` lines in cmd/afk.rs follow the repository's pattern: `print_stdout` as in the other cmd files, and `redundant_closure_for_method_calls` as in launch hold.rs:146.

### Revert checks
For each check I broke one fix in the clone, ran the test binary, and then ran `git reset --hard`. The new test failed every time.

| Branch | Fix reverted | Test that failed |
| --- | --- | --- |
| start | store does not merge `extra` (team.rs:159) | `create_afk_team_merges_extra_into_the_created_event` |
| start | `budget_refusal` never refuses (launch afk.rs:61) | `past_the_budget_an_implementer_is_refused_and_a_reviewer_starts`, `a_dry_run_past_the_budget_warns_about_the_implementer` |
| start | the real launch skips the budget (member.rs:65) | `past_the_budget_an_implementer_is_refused_and_a_reviewer_starts` |
| start | no model warning (launch afk.rs:43) | `each_member_below_the_table_gets_one_warning` |
| start | the lead process is never missing (hold.rs:75) | launch `a_lead_process_that_cannot_be_found_is_reported_only_where_a_hold_exists`, cli `a_lead_process_that_cannot_be_found_is_refused` |
| start | a taken name is not refused (launch afk.rs:79) | launch `a_new_team_needs_a_good_name_a_good_title_and_a_free_name`, cli `a_bad_name_or_title_or_a_taken_name_is_refused` |
| start | a background lead is allowed (cmd/afk.rs:112) | `a_background_lead_is_refused` |
| start | `afk_already_running` is skipped (cmd/afk.rs:115) | `a_session_that_leads_an_open_afk_team_cannot_start_another` |
| start | a doctor FAIL is ignored (cmd/afk.rs:134) | `a_failed_doctor_check_refuses_and_names_the_check` |
| start | a zero budget is allowed (cmd/afk.rs:90) | `a_bad_budget_is_refused` |
| start | an unreadable task is allowed (cmd/afk.rs:123) | `a_missing_task_file_is_refused` |
| start | cmd/afk.rs as it was before c774aed | `lead_cwd_is_the_folder_the_session_started_in_not_the_shells`, `a_task_copy_that_fails_leaves_no_team_behind`, `a_start_creates_the_team_the_task_copy_and_the_hold` |
| endings | close never checks the report (plan.rs:73) | `no_report_refuses_before_any_step`, `a_report_missing_a_heading_names_it`, `numbered_and_bold_headings_count` |
| endings | `**` after a number is not stripped (close afk.rs:84) | `numbered_and_bold_headings_count` |
| endings | every team needs a report (close afk.rs:22) | `a_normal_team_closes_without_a_report` |
| endings | `spent` is always false (watch afk.rs:33) | `the_budget_counts_down_and_is_spent_at_the_end` |
| endings | any line that starts with `D` counts (watch afk.rs:49) | `decisions_count_only_lines_that_start_an_entry` |
| endings | the status view never sets `afk` (watch status.rs:64) | 3 watch afk tests; cli `an_afk_team_prints_one_afk_line_after_the_team_line`, `a_spent_budget_prints_when_it_ended` |
| endings | status prints no afk line (cli status.rs:120) | the same two cli status tests |
| endings | teams prints no `, afk` mark (teams.rs:49) | `an_afk_team_is_marked_and_carries_its_budget_end` |
| endings | `afk_budget_ends_at` is always None (directory.rs:50) | cli `an_afk_team_is_marked_and_carries_its_budget_end`, mcp `team_teams_gives_an_afk_team_its_budget_end_and_others_null` |
| endings | `afk` and `afk_budget_ends_at` removed from the zod schemas | 3 of the 4 new ui tests. The test "older output still parses" passes either way, as a compatibility test should. |
| stophold | a live watch never wakes the lead (afk_hold.rs:38) | `a_working_member_with_a_live_watch_gives_no_note`, `a_live_watch_on_another_team_the_session_leads_gives_no_note` |
| stophold | never past the budget (:81) | `past_the_budget_the_note_says_so` |
| stophold | closed teams count (:66) | `a_closed_afk_team_gives_no_note` |
| stophold | other sessions' teams count (:66) | `a_team_led_by_another_session_gives_no_note` |
| stophold | one unreadable team hides every note (:45) | `an_unreadable_team_gives_no_note_and_the_others_still_do` |
| stophold | a team that is not AFK gets a note (:74) | `a_team_that_is_not_an_afk_run_gives_no_note` |
| stophold | a live lease alone wakes the lead (:73) | `a_live_watch_with_no_working_member_gives_a_note` |
| stophold | a working member alone wakes the lead (:73) | `a_working_member_with_no_watch_gives_a_note` |
| stophold | the hook drops the AFK notes (hook.rs:211) | `an_afk_team_with_nothing_to_wake_the_lead_is_held`, `the_watch_note_and_the_afk_note_come_in_one_reply` |
| stophold | the hook holds a continued stop (hook.rs:193) | `an_afk_continuation_is_not_held_again` |
| stophold | the hook holds a subagent's stop (hook.rs:193) | `an_afk_subagent_stop_is_not_held` |
| skill | the plugin renders no AFK skill (plugin.rs:90) | lib `the_seven_files_name_the_binary_and_the_hook`, `the_afk_skill_is_the_compiled_in_text_and_changes_the_version`; tests/lead_setup.rs `an_installed_plugin_whose_afk_skill_differs_is_not_current` |
| skill | the Codex render keeps the pointer line (codex plugin.rs:101) | `a_codex_lead_is_not_pointed_at_the_claude_afk_skill` |
| skill | the skill is named `afk2` (SKILL.md:2) | `the_afk_skill_is_named_afk_and_can_be_stamped` |

These tests were not revert-checked, because each one checks that nothing changes, so no fix exists to revert:
- `outside_a_lead_session_nothing_starts`. No small change removes that refusal and still compiles.
- `adding_to_an_afk_team_with_no_members_starts_the_member`. It pins launch behaviour that already existed.
- `removing_a_member_needs_no_report`.
- `a_normal_team_prints_no_afk_line_and_a_null_object`.

## Claimed but not verified
- The design's live check (design section 7, "Live check, with the user present"). Nobody ran `/teamwire:afk` in a real session. It needs `cargo install` and `teamwire install`.
- No test runs the `teamwire close` binary on an AFK team. Library tests cover the refusal, and the CLI prints it through code that already existed (close.rs:113).
- No test checks that the archive holds `afk/REPORT.md`. The archive walks the whole team folder (crates/teamwire-close/src/archive.rs:78), so the design's "no change" claim holds by reading the code.
- Reopen, verify and metrics on a team with no members have no test. docs/failure-modes.md already says so.

## Still broken
- F1. The joined branch fails the full gate until the six-line fix is applied.
- `herdr_env_wins_over_tmux` fails on this machine on every branch and on `main`. D12 already records this.

## Risks (not defects)
- crates/teamwire-cli/src/cmd/doctor.rs grew from 760 to 786 lines on `start`. Tidy fails a file past 800 lines, so the next change to doctor.rs will need a split first. Owner: start.
- The crash window after the team row is written (see "Where two members' work meets").
- The Codex drop matches the pointer line exactly (codex plugin.rs:28-30). If someone rewords that line in the main skill, they must change the Codex constant too. The Codex test fails until they do, so the test catches it.

## Known limits
No branch adds a `shortcut:` comment. I searched the diff of main..afk/afkmode and of each member branch. No branch adds a TODO, FIXME or HACK either.

## Questions for the parent
None.

## Decisions I made
- I applied the F1 fix in my clone only, so the gate could run past clippy on the joined branch. The patch is in my scratchpad as d21-fix.patch. I committed nothing.
- `scripts/gate.sh` stops at the first failing test binary, which is always `herdr_env_wins_over_tmux` here. So I recorded the gate's real exit code and then ran its remaining steps one by one: the workspace tests with `--no-fail-fast`, doc and check-docs. On the joined branch nothing was skipped. On member branches I skipped only that test, as their checks do.
- I checked each test by breaking one line of its fix, not by reverting whole commits. A whole-commit revert mostly fails to compile, and that proves nothing about the test.
- I ran `bun test` and typecheck once on the joined branch with the F1 patch in place. The patch touches only Rust test files, so the ui result is the same for the branch as merged.
- I also ran the Codex harness tests on `skill` (F5), because its spec check leaves them out.
- I graded the growth of doctor.rs and the crash window as risks, not findings, because neither changes what the team shipped.
- I named the lead as the owner of F5, because the check lives in the lead's spec, not in a member's files.
