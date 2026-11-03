# Closer report: afkmode (AFK mode for teamwire)

## Summary

The joined branch `afk/afkmode` (065b102) is sound. It builds what design
sections 5 and 6 describe, and every part has tests that fail when the code
they pin is removed. I found no defect that changes what the team built or
concluded. Nothing is graded "wrong conclusion".

- Full gate on the joined branch, run detached from tmux as the plan says: all
  six steps ok, `gate exit 0`. No test was skipped.
- `teamwire verify afkmode`: exit 0, all five implementers PASS at the commits
  that were merged.
- I reverted or changed the code behind each new test, 35 single changes, in
  my temporary clone. Every non-guard test failed under its change, with one
  exception: the skill's command test, finding 1 below.
- An AFK team with zero members works with every reader I ran on the joined
  binary: status, teams, verify, metrics, reopen, watch, the Stop hook and
  close. The dashboard's zod schemas parse that real output.
- Findings: 3 wrong details and 3 wording items, all in docs or a test's
  reach, plus one accepted low risk left open from review. No `shortcut:`
  comment was added.

Labels used below: the five implementers are start, budget, stophold,
closestat and skill. D-numbers are entries in `afk/decisions.md`.

## What was merged

`afk/afkmode` = `afk/base` (fb842b4) plus five merges, in the plan's order:

| Member | Branch tip merged | Merge commit |
| --- | --- | --- |
| budget | 629e68f | 18b63d9 |
| start | e026e22 | 37267a1 |
| stophold | 94e4675 | 137f4fc |
| closestat | 8158ee6 | 29ec5e6 |
| skill | 55842b8 | 065b102 |

Each branch tip is an ancestor of the joined head (`git merge-base
--is-ancestor`). Each member's worktree sits at its tip, so verify ran the
merged code.

## Verified (I ran each command and saw the exit code)

### On the joined head, 065b102, in my clone

| Command | Exit | Result |
| --- | --- | --- |
| `scripts/gate.sh`, detached from tmux (parent pid 1), log `afk/closer-gate.log` | 0 | fmt ok 2s, tidy ok 2s, clippy ok 10s, test ok 239s, doc ok 5s, docs ok 1s, `gate exit 0` |
| `scripts/gate.sh fast` | 0 | fmt, tidy, clippy ok |
| `cargo test -p teamwire-cli -p teamwire-launch -p teamwire-report -p teamwire-close -p teamwire-watch -p teamwire-msg -p teamwire-harness-claude -p teamwire-mcp --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux` | 0 | 959 passed, 0 failed. These are the seven changed crates plus teamwire-mcp, which depends on them. The skip is needed because my shell runs inside tmux (D23). The full gate above ran that test without the skip, and it passed. |
| `cd ui && bun test` | 0 | 169 tests: 166 pass, 3 skip (live tests), 0 fail |
| `cd ui && bun run typecheck` | 0 | `tsc --noEmit` clean |
| `scripts/check-docs.sh` | 0 | `docs check: ok` |
| `teamwire afk start --help` | 0 | shows `--team`, `--title`, `--task`, `--budget` (default 8h). This confirms start's claim. |

The union of the five member checks is the cargo, bun and docs rows above plus
`gate.sh fast`, so each member's check also passes on the joined head.

### On each member's branch: `teamwire verify afkmode` (exit 0, "clean")

| Member | Verdict | Head | Tests in the log | Member's claim |
| --- | --- | --- | --- | --- |
| start | PASS, 68s | e026e22 | 419 passed, 0 failed; gate fast ok | 419 passed: matches |
| budget | PASS, 72s | 629e68f | 409 passed, 0 failed; gate fast ok | 409 passed: matches |
| stophold | PASS, 66s | 94e4675 | 414 passed, 0 failed; gate fast ok | 414 passed: matches |
| closestat | PASS, 77s | 8158ee6 | 579 passed; bun 166 pass, 3 skip, 0 fail; `tsc --noEmit` ran; gate fast ok | 579 passed, 166 pass: matches |
| skill | PASS, 62s | 55842b8 | 440 passed; `docs check: ok`; gate fast ok | 440 passed: matches |

Every flag verify raised is a `weakened` line on a `#[expect(...)]` lint
attribute with a reason. All were already waived. I read each one: start's
`cmd/afk.rs:21,31` (print to stdout and stderr), `tests/afk.rs:5,9` and
`tests/afk_start.rs:3,12`; budget's `member.rs:59` and `tests/afk_budget.rs:4`;
closestat's `tests/afk_report.rs:3`. None loosens a test. Tidy also prints
`PROTECTED-CHANGED docs/adr/0025-afk-runs.md`, which the skill brief allows.

### Revert checks: each new test against the code it pins

Each check below ran in my clone on the member's branch. I changed one piece
of code at a time and kept the tests. The logs are
`afk/closer-mut-{start,stophold,closestat,closestat2,skill}.log`. The budget
results are in the progress notes at the end.

| Member | Code removed or changed | Tests that failed | Tests that still passed |
| --- | --- | --- | --- |
| budget | `member.rs` and `plan_rules.rs` set back to afk/base | `past_the_budget_an_implementer_is_refused_and_a_reviewer_starts`, `members_below_the_afk_table_get_one_warning_each`, `a_refused_implementer_is_not_counted_as_requested` | guards `a_team_without_a_budget_is_unaffected`, `an_ordinary_team_gets_no_model_warning`; also `before_the_budget_ends_an_implementer_starts` (wording item 5) |
| budget | review fix 629e68f only (`requested` counts every member again) | `a_refused_implementer_is_not_counted_as_requested` | the other 5 |
| start | `cmd/afk.rs` set back to the afk/base stub | all 6 binary tests in `tests/afk.rs` | none |
| start | doctor's FAIL ignored in `cmd/afk.rs` | `afk_start_refuses_when_a_doctor_check_fails` | the other 5 |
| start | the `afk_already_running` check | `afk_start_refuses_a_session_that_already_leads_an_open_afk_team` | the other 7 |
| start | the `team_exists` check | `afk_start_refuses_a_taken_team_name` | the other 7 |
| start | the `no_lead_process` check (`if false`) | `afk_start_with_no_lead_process_refuses_and_creates_nothing` | the other 7 |
| start | the hold condition (`if true`) | `afk_start_on_a_platform_without_a_hold_creates_the_team_and_warns` | the other 7 |
| start | the task copy written to another name | `afk_start_creates_the_team_copies_the_task_and_takes_the_hold`, `..._without_a_hold_creates_the_team_and_warns` | the other 6 |
| start | `ensure_hold` not called | the same two | the other 6 |
| start | `lead_cwd` key renamed | `afk_start_creates_the_team...`, `afk_start_records_the_current_directory_when_the_harness_cannot_place_the_lead` | the other 6 |
| start | doctor's host-none FAIL in `launch_checks` | `launch_checks_fail_on_host_none_and_keep_the_plugin_check` | the other 43 doctor tests |
| start | (guard, not reverted, D33) | | `add_to_an_afk_team_with_no_members_starts_the_member` passes |
| stophold | no team counts as AFK (`afk_ends: None`) | 6 AFK tests in `tests/lead_stop.rs` | 13 older tests |
| stophold | a live watch never wakes the lead | `a_live_watch_on_a_working_team_means_no_afk_note` | 18 |
| stophold | the state and session filter removed | `an_ordinary_or_closed_team_or_another_session_gets_no_afk_note` | 18 |
| stophold | review fix 94e4675, half 1 (`wakes_lead: watched`) | `a_live_watch_with_every_member_finished_still_gets_the_afk_note` | 18 |
| stophold | review fix 94e4675, half 2 (any lease counts) | `a_dead_watch_on_a_working_team_still_gets_the_afk_note` | 18 |
| stophold | `now > ends` instead of `>=` | `past_the_budget_the_afk_note_says_budget_spent_after_the_team_tag` | 18 |
| stophold | `cmd/hook.rs` set back to afk/base | `an_afk_team_holds_the_stop_once_with_the_afk_note`, `the_afk_note_and_the_watch_note_come_in_one_reply` | 8 |
| closestat | `plan.rs` set back to afk/base (no report check) | `close_refuses_an_afk_team_without_its_report`, `close_refuses_an_afk_report_that_lacks_a_heading_and_names_it`, `close_of_an_afk_team_with_a_full_report_archives_the_afk_files` (review fix a1236c0), `close_says_an_unreadable_afk_report_is_unreadable_not_missing`, `an_afk_team_whose_only_member_was_removed_still_needs_a_report`, CLI `close_of_an_afk_team_without_a_report_prints_the_refusal_code` | guards `removing_one_member_of_an_afk_team_needs_no_report`, `an_ordinary_team_closes_without_a_report`; `an_afk_team_that_never_had_a_member_closes_without_a_report` (pinned by the next row) |
| closestat | D39 fix 8158ee6 (`members.is_empty()` check) | `an_afk_team_that_never_had_a_member_closes_without_a_report` | 7 |
| closestat | review fix a1236c0 (every read error says "missing") | `close_says_an_unreadable_afk_report_is_unreadable_not_missing` | 7 |
| closestat | `StatusView.afk` always `None` | `an_afk_team_shows_its_budget_and_decision_count`, `a_spent_budget_shows_zero_seconds_left`, CLI `status_of_an_afk_team_prints_the_afk_line_and_json_object` | `an_ordinary_team_has_no_afk_view` (pins today's `null`) |
| closestat | decision count without leading white space (D35) | `an_afk_team_shows_its_budget_and_decision_count` | 17 |
| closestat | `TeamEntry.afk_budget_ends_at` always `None` | `teams_marks_an_afk_team_and_its_json_gives_the_budget_end` | 3 |
| closestat | `cmd/teams.rs` set back to afk/base (no marker) | the same test | 3 |
| closestat | `ui/src/shared/teamwire.ts` set back to afk/base | 5 of the 7 "afk fields" tests | the two "older output without the key" tests, by design (D20) |
| skill | the AFK file not rendered into the plugin | `install_writes_the_afk_skill_as_a_second_stamped_skill`, `a_plugin_without_the_afk_skill_or_with_an_edited_one_is_a_problem`, `a_fresh_install_writes_the_files...` (the file count), unit `the_seven_files_name_the_binary_and_the_hook` | the rest |
| skill | pointer line removed from the main skill | `the_main_skill_points_at_the_afk_skill` | 10 |
| skill | heading `## Risks` renamed | `the_afk_skill_lists_the_eight_report_headings` | 10 |
| skill | `teamwire afk start ...` replaced by `teamwire afkstart` | `the_afk_skill_names_only_commands_and_tools_that_exist` | 10 |
| skill | `teamwire afk start ...` replaced by `teamwire afk finish` (no such subcommand) | none: all 11 passed | finding 1 |

### An AFK team with zero members (design section 7 risk 2)

I appended a scratch test to the clone's `crates/teamwire-cli/tests/afk.rs`
(`afk/closer-zero-test.rs`). The test runs the joined binary on start's
healthy test setup. Its outputs are in `afk/closer-zero/`. Every command
below exited 0.

| Reader | Result on a team with no members |
| --- | --- |
| `afk start --budget 2h` | the five lines of D12 |
| `status zero` | `afk: budget ends <ts> (1h59m left), 0 decisions`, `members:` empty |
| `status zero --json`, `status --json` | `"afk": {budget_ends_at, seconds_left 7199, spent false, decisions 0}`, `members: []` |
| `status` | `zero open 0 members, 0 done` |
| `teams`, `teams --json` | `zero (open, afk) Zero: ...`; `"afk_budget_ends_at": "<ts>"` |
| `verify zero` | `clean` |
| `metrics zero`, `metrics --json` | zero counts |
| `reopen zero --dry-run` | `lead skipped: already open (this session)` |
| `watch zero --once` | `[teamwire:zero] WATCHING 0 members`, no `TEAM-COMPLETE` |
| `hook claude-lead-stop` with `stop_hook_active` false | one AFK note (wording item 6) |
| the same with `stop_hook_active` true | no reply |
| `close zero --dry-run`, `close zero` | no report needed (D39); `released sleep hold`; archived; then `teams` says `no teams` |
| exit-2 path: `afk start` with `afk` blocked by a file | exit 2, `failed: ...; team broken exists` |
| a second `afk start` from that session | exit 1, `refused: afk_already_running` |
| `close broken` | exit 0, closed without a report, as cli.md and failure-modes now say |
| dashboard: zod `StatusView`, `StatusAll`, `TeamsList`, `MetricsView` on the real JSON | all parse; `afk` and `afk_budget_ends_at` kept (`afk/closer-zod.ts`, exit 0) |

### Binding names of task.md, checked on the joined head

| Name | Where | Result |
| --- | --- | --- |
| `no_lead_session`, `afk_already_running`, `no_lead_process` | `crates/teamwire-launch/src/afk.rs:101,110,201` | present, tested |
| `doctor_failed` naming the check | `crates/teamwire-cli/src/cmd/afk.rs:81` | `doctor_failed: <name>: <detail>`, tested |
| `afk_hold_notes(home: &Path, session: &SessionId, platform: &dyn Platform, clock: &dyn Clock) -> Vec<String>`, exported beside `teams_without_watch` | `crates/teamwire-report/src/afk_hold.rs:26-31`, `lib.rs` | exact |
| `create_afk_team(at, team, budget_ends_at, extra: &Value)`; afk start passes `{"lead_cwd": ...}` | `crates/teamwire-store/src/team.rs:151-157`; `afk.rs:214` | exact, tested |
| `StatusView.afk: Option<AfkView>`; `AfkView {budget_ends_at: Timestamp, seconds_left: u64, spent: bool, decisions: u32}` | `crates/teamwire-watch/src/lib.rs`, `afk.rs:11-20` | exact; JSON uses the same names |
| `teams --json` key `afk_budget_ends_at`, string or `null` | `crates/teamwire-msg/src/directory.rs:20-21,50` | exact; MCP `team_teams` shares the struct |
| zod accepts the new keys and older output | `ui/src/shared/teamwire.ts:122-137,169` | `.nullable().optional()`, tested |

### Design sections 5 and 6, item by item

| Design item | Built at | Tested by |
| --- | --- | --- |
| 5.1 an AFK team is a team row with `afk_budget_ends_at` | store (afk/base), `afk.rs:214` | AFK-4 tests |
| 5.2 step 1 lead session, refusals | `afk.rs:91-115` | AFK-1 tests |
| 5.2 step 2 name, title, taken name | `afk.rs:116-127` | AFK-2 tests |
| 5.2 step 3 doctor's launch checks, FAIL refuses, warn prints | `doctor.rs:703-725`, `cmd/afk.rs:73-84` | AFK-3 tests |
| 5.2 steps 4-6 team row, task copy, hold through `ensure_hold` | `afk.rs:186-253` | AFK-4, AFK-5 tests |
| 5.2 step 7 output | `cmd/afk.rs:90-96` | `afk_start_prints_the_run_and_exits_zero` |
| 5.2 `--add` on an empty team | unchanged validate rules | guard `add_to_an_afk_team_with_no_members_starts_the_member` |
| 5.3 the AFK hold, its notes, one reply, budget, writes nothing | `afk_hold.rs`, `cmd/hook.rs:200-216` | AFK-9 tests |
| 5.4 `afk_budget_spent` per implementer; model-table warning | `member.rs:59-84`, `plan_rules.rs:12-37,108-125` | AFK-7, AFK-8 tests |
| 5.5 close needs the report; `--role` unaffected | `close/src/afk_report.rs`, `plan.rs:75-79` | AFK-10 tests |
| 5.5 status line and `afk` object; teams marker | `watch/src/afk.rs`, `cli/src/cmd/status.rs:124-138`, `teams.rs:44-51` | AFK-11 tests |
| 5.6 skill file, second stamped skill, doctor checks it, pointer line | `examples/skills/afk/SKILL.md`, `plugin.rs:86`, main skill line 13 | AFK-12 tests |
| 6 the skill's agreed content | `examples/skills/afk/SKILL.md` | I compared it with design lines 200-372; every rule is kept, mostly word for word |

Out of scope, as the plan says: the live check with the user present
(`cargo install`, `teamwire install`, a real `/teamwire:afk` run).

## Coverage: did each member do its task?

- **start: did all of it.** It built `afk_check` and `afk_start`
  (`crates/teamwire-launch/src/afk.rs:91-253`), the command
  (`crates/teamwire-cli/src/cmd/afk.rs:35-105`), doctor's `launch_checks`
  (`crates/teamwire-cli/src/cmd/doctor.rs:703-725`), and every test named in the
  plan. The refusal order matches D9 (`afk.rs:95-156`). Reviewer note 1, a
  Codex lead, was settled by the lead as D38 (no change).
- **budget: did all of it.** The refusal is at `crates/teamwire-launch/src/member.rs:59-84`.
  The warnings are at `crates/teamwire-launch/src/plan_rules.rs:12-37,108-125`.
  Review finding 1 was fixed and pinned. Review finding 2 is still open (risk 1 below).
- **stophold: did all of it.** `afk_hold_notes` is at
  `crates/teamwire-report/src/afk_hold.rs:26-97`. The hook calls it at
  `crates/teamwire-cli/src/cmd/hook.rs:200-216`. Both note texts match D16
  and D31 character for character (`afk_hold.rs:94`). The review gap was
  closed with two tests that I confirmed fail without their halves.
- **closestat: did all of it.** It covers close (`crates/teamwire-close/src/afk_report.rs`,
  `plan.rs:75-79`), status (`crates/teamwire-watch/src/afk.rs`,
  `crates/teamwire-cli/src/cmd/status.rs:124-138`), teams (`teams.rs:44-51`,
  `crates/teamwire-msg/src/directory.rs:20-21,50`), the zod copy
  (`ui/src/shared/teamwire.ts:122-137,169`), and D39. Each `--json` change
  shares its commit with its zod change (c92ec32, f87ba47).
- **skill: did all of it, with three wrong details in the docs.** The skill
  (`examples/skills/afk/SKILL.md`), its install (`plugin.rs:60-61,86`), the
  pointer line, ADR 0025, AFK-1 to AFK-12, the failure-modes rows and every API
  doc are in place. All seven revskill findings and the note are fixed on the
  joined head. The new findings follow.

## Findings, most important first

No finding is graded wrong conclusion. Each line names the owner the lead
should route it to.

1. **Wrong detail. Owner: skill.** The AFK command test checks only the first
   word after `teamwire`.
   - Where: `crates/teamwire-cli/tests/preset.rs:405-422` uses the shared
     helper `shown_commands`, `preset.rs:308-314`. The helper keeps the first
     word after `teamwire ` and drops the rest.
   - Failing case: I changed the skill's command at `SKILL.md:31` to
     `teamwire afk finish --team <name>`. No such subcommand exists. All 11
     preset tests still passed. A made-up top-level word (`teamwire afkstart`)
     does fail the test.
   - The doc that overclaims: `docs/failure-modes.md:181` says "the skill names a
     command or tool that does not exist ... The test fails". The test name says the same.
   - Why the conclusion holds: I checked every command the skill shows against
     the joined binary's `--help`: `afk start` and its four flags,
     `launch --add`, `verify <team> --role`, `status <team>`,
     `metrics <team>`, `close <team>` and `install`. All of them exist. Only
     the test is weaker than the docs say. The main skill's existing test has
     the same limit, because it uses the same helper.
2. **Wrong detail. Owner: skill.** `docs/api/cli.md:109` lists the wrong refusal order.
   - The first row of the order table pairs `unknown_harness` with
     `unknown_host`.
   - In the code, `unknown_host` comes from `lead_plan`, which runs last in
     `afk_check` (`crates/teamwire-launch/src/afk.rs:156`). That is after
     `task_empty` (`afk.rs:149`) and before doctor.
   - `unknown_harness` is first, as the table says (`afk.rs:98`).
3. **Wrong detail. Owner: skill. The lead's D38 bears on it.** The docs state
   a rule that nothing enforces.
   - `docs/adr/0025-afk-runs.md:76-77` says "Only a Claude Code lead can run an
     AFK run. A Codex lead keeps today's behaviour."
   - `docs/api/cli.md:100` says the team is "led by the calling Claude Code
     session".
   - `afk_check` takes whatever lead harness the wiring picks
     (`crates/teamwire-launch/src/afk.rs:95-99`). The wiring picks `codex`
     when `CODEX_THREAD_ID` is set (`crates/teamwire-cli/src/wiring.rs:138-143`).
     So a Codex lead can start an AFK team, and the Stop hook then holds it
     (revstophold's question). I found this by reading. I did not run it.
   - D38 chose not to refuse a Codex lead. The fix is to the words: say that
     only a Claude Code lead is supported, not that only one can run it.
4. **Wording. Owner: skill.** `README.md:78-79` says "`teamwire close` accepts
   the run only after its report has all eight headings". D39 lets a team that
   never had a member close without one. cli.md, the ADR and the skill already
   say so.
5. **Wording. Owner: the lead (plan and D33).**
   `crates/teamwire-launch/tests/afk_budget.rs::before_the_budget_ends_an_implementer_starts`
   passes on afk/base, and D33 does not list it as a guard. My first budget
   revert check confirmed it passes. The test still has value: it fails if the
   refusal ignores the clock. Two other new tests also pin today's behaviour,
   and their briefs do not mark them as guards:
   - `crates/teamwire-watch/tests/status.rs::an_ordinary_team_has_no_afk_view`
   - the two zod tests "without the key"
6. **Wording. Owner: stophold.** For an AFK team with no members, the hold note
   still says to run close "once <dir>/afk/REPORT.md exists"
   (`afk_hold.rs:94`; seen in `afk/closer-zero/30_hook_first_stop.txt`). D39
   lets that team close without a report. A lead that follows the note adds a
   reporter it does not need. The skill's section 7 (`SKILL.md:231-234`) gives
   the right rule, so a lead that reads the skill does the right thing.

## Contradictions between members

- **The skill docs and the code from the other branches.** The skill member wrote
  the docs before it could see the other branches' code. Its review round
  fixed every mismatch the reviewer found. Each of those fixes is still in
  place on the joined head:
  - the D31 quote;
  - the D39 close rule;
  - the three refusal texts;
  - the doctor FAIL;
  - "When it names a team";
  - the ADR schema sentence.

  Findings 2, 3 and 4 are the mismatches left.
- **The note from stophold and D39 from closestat.** The note text came before
  D39, and the two now differ for an empty team (finding 6).
- **Budget and the skill's member table.** The two agree. The floors in D15
  (high for an implementer or investigator, xhigh for a reviewer or closer,
  opus for every kind) match the profiles in `SKILL.md`'s table at lines 105-114.
- **Start, stophold and closestat on the team directory.** All three use
  `<home>/teams/<team>/afk/`. Start creates it, the hold note names it, and
  status reads `decisions.md` from it. The zero-member run shows the three
  agree.
- **Start and budget.** Start never edits `member.rs`. It calls `new_team` and
  `lead_cwd` from the contract commit (`afk.rs:209-211`). D29 holds.

No other contradiction found.

## Claimed but not verified by me

- Budget says rustc rejects `ReadTx::team` written as a path at `member.rs:59`.
  That claim is why the `#[expect(clippy::redundant_closure_for_method_calls)]`
  is there. I did not try the change. Ten other call sites carry the same
  attribute.
- Revstart's real-binary run on macOS showed `caffeinate -ims -w <pid>`
  running after `afk start`. My zero-member run shows close releasing a hold
  (`released sleep hold 66491`). I did not look at the process itself.
- Design section 7 risk 3: a schema-4 binary refuses a schema-5 database. I
  did not run an old binary. ADR 0025 now states the store's rule
  (`store.rs:158-220`).

## Still open, accepted, or known limits

1. **Budget, low, still open from review (revbudget finding 2).** The budget
   check reads the team row with `.ok().flatten()`
   (`crates/teamwire-launch/src/member.rs:63-68`). A store read error at that
   moment therefore lets an implementer start past the budget. Budget answered
   with evidence: `validate` reads the same row with `?` just before, so only
   a short error between the two reads reaches this line.
2. **Start, known limit.** `crates/teamwire-cli/src/cmd/doctor.rs` is 798
   lines, and tidy fails past 800 (AGENTS.md). The next change to doctor must
   move code out first.
3. **Design risks the design accepts.**
   - A lead that stops twice with nothing running is stuck.
   - A Claude usage limit pauses every session.
   - Both are recorded in ADR 0025 and in the last row of failure-modes
     section 3, with no test.
4. **`shortcut:` comments.** None. `git diff afk/base afk/afkmode | grep '^+.*shortcut:'`
   finds nothing.
5. **Tmux.** Inside tmux, `tmux_wiring::herdr_env_wins_over_tmux` fails
   (D23). The plan's line for AGENTS.md about this (planning check item 8) is
   still not added.

## Decisions I made

- I ran the member checks on the members' branches through
  `teamwire verify afkmode`. Verify ran each member's exact check in that
  member's worktree, at the same tip that was merged. I did not run the checks
  a second time in my clone.
- On the joined head, I ran the eight crates of the five checks as one cargo
  command, with D23's skip, because my shell runs inside tmux. The detached
  full gate covers the skipped test.
- No reviewer finding was graded "wrong conclusion". I still reverted every
  review fix that came with a test: 629e68f, 94e4675 (both halves), a1236c0
  (both findings) and 8158ee6 (D39).
- For start and stophold, a whole-file revert would only have broken the
  build. So I changed one condition at a time, so that each test ran and
  failed on its own assertion.
- I tested the zero-member readers with a scratch test in my clone, not by
  hand against a home directory. That way the run used the healthy setup and
  the fakes of start's own tests, and touched nothing outside temporary
  directories.
- I graded finding 3 "wrong detail" and did not reopen D38. The code follows
  D38. Only the words overstate it.

## Questions for the parent

None.

## Progress log

- 20:04 Read brief, task, plan, D1-D39, member and reviewer reports, comms m001-m006.
- 20:05 Clone made at `$TEAMWIRE_HOME/cache/closer/afkmode/clone`. Setup (`cargo build --workspace --all-targets --locked`, `bun install --frozen-lockfile`) exit 0 in 22 s. Full gate started detached.
- 20:10 Full gate `gate exit 0`. Zero-member readers run, all exit 0. zod parse exit 0.
- 20:11 Joined-head checks: gate fast, eight crates, bun test, typecheck, check-docs, all exit 0.
- Budget revert, `member.rs` and `plan_rules.rs` set back to afk/base: 3 failed (`members_below...`, `past_the_budget...`, `a_refused_implementer...`), 3 passed (two guards and `before_the_budget_ends...`). Revert of 629e68f only: 1 failed (`a_refused_implementer_is_not_counted_as_requested`).
- Stophold, closestat, start and skill revert checks: see the table and the logs in `afk/`.
- `teamwire verify afkmode` exit 0, clean.
- Clone reset to afk/afkmode with `git reset --hard` and `git clean -fd`. The clone is left in place for `teamwire close`.
