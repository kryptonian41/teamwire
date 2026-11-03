# planner report

## Result

Every file of the brief's end state exists under `$TEAMWIRE_HOME/teams/afkmode/afk/`:

| File | What it holds |
| --- | --- |
| `plan.md` | a 30-line summary, the contract amendment, the five units, the owned files and who pins what, the test names and AFK-1..AFK-12, the checks, the eight-item planning check |
| `briefs/start.md`, `budget.md`, `stophold.md`, `closestat.md`, `skill.md` | one brief per implementer, in the main skill's 14 sections |
| `spec-wave1.json` | the `teamwire launch --add` spec: five implementers, sonnet, high, base `afk/base`, branches `afkmode/<role>`, plugins `{"claude-mem@thedotmack": false}` |
| `decisions.md` | D6 to D26 added in the first pass, D27 to D36 in the fix round |
| `amend-afk-args.patch` | the contract amendment for the lead to commit on afk/base before launch (D7) |

The lead must apply `amend-afk-args.patch` on afk/base before launch (plan.md, "Contract amendments
for the lead"). Without it the skill member's command test cannot run, and the start member has no
`new_team` or `lead_cwd` helper to call.

## Progress

- Read the brief, task.md, decisions.md D1-D5, the design. afk/base holds 5b86c1e (`git log --oneline -3 afk/base`).
- Read the code of every unit. Ran each check on a fresh clone of afk/base in my scratchpad. Found that `tmux_wiring::herdr_env_wins_over_tmux` fails on the base inside a tmux pane.
- Wrote and verified the afk arguments skeleton patch (`afk/amend-afk-args.patch`): gate fast ok, preset tests 8 passed, `git apply --check` clean on the repository's afk/base.
- Ran all five member checks on the base plus the patch in the fresh clone: start 403 passed, budget 115, stophold 370, closestat 562 plus 162 bun tests and the typecheck, skill 434 plus the docs check. Gate fast was ok in each run, and every run exited 0.
- Wrote decisions D6-D26, plan.md, the five briefs and spec-wave1.json. `teamwire launch --dry-run` of the spec, using the clone's binary against an empty temporary home: exit 0, five members, no warnings, nothing created.
- Fix round on planrev.md (m001): all ten points hold. Extended the patch, added D27-D36, updated plan.md, all five briefs and the spec, and reran the changed checks, the dry run and the full gate. Details under "Fix round (planrev.md)".

## Verified (command and output)

- The base fails one test inside tmux. Command: `cargo test -p teamwire-cli -p teamwire-launch --locked` on a fresh clone. It exited 101 at `herdr_env_wins_over_tmux`, with `left: Some(2) right: Some(0)` and stderr `could not run tmux: not_found`. The test's own comment (`crates/teamwire-cli/tests/tmux_wiring.rs:340-343`) says it holds only when no tmux or herdr process is among its parents. This session's parent is tmux (`ps`: pid 2332 tmux). With `--no-fail-fast -- --skip herdr_env_wins_over_tmux`, 403 tests passed, exit 0.
- The cold build takes 21 s. Command: `cargo build --workspace --all-targets --locked` on the fresh clone, `real 21.26`.
- `--add` already accepts a team with no members. `crates/teamwire-launch/src/validate.rs:62-90` refuses only `team_exists`, `no_such_team` and `team_closed`, and the only empty-spec refusal is `no_children` at line 101.
- No doctor or install change is needed for a second skill. `plugin_problems` (`crates/teamwire-harness-claude/src/lead_setup.rs:188`) and `installed_for_own_home` compare every file that `PluginFiles::render` returns, and install writes the same list.
- `crates/teamwire-cli/tests/doctor.rs:100` compiles `src/cmd/doctor.rs` by `#[path]` with a stub `Wiring`, so doctor.rs and that test must have one owner.
- `crates/teamwire-harness-claude/tests/lead_setup.rs:184` pins 6 rendered files.
- `crates/teamwire-cli/tests/preset.rs:276` reads the command list from `teamwire --help`.
- The close path for an empty store (`crates/teamwire-close/src/empty.rs`) runs only when there is no team row. An AFK team has a row, so it takes the normal close and archive.
- `TeamEntry` is built only in `describe()` (`crates/teamwire-msg/src/directory.rs:42`). `StatusView` is built elsewhere only with `..StatusView::default()` (`crates/teamwire-cli/src/cmd/status.rs:300`).

## Ruled out

- **promptwire skills.** I searched for rust, clap, zod, sqlite, "claude code plugin" and "claude code hooks". The only matches were skill-creator and a threat-model skill. skill-creator waits for a user (D25).
- **A doctor-function amendment by the lead.** Start owns doctor.rs instead, because the skill unit needs no doctor change (D8).
- **A closed AFK team sliding through close's empty-store path.** That path needs a store with no team row.
- **A test that pins docs/api.** `grep -rn 'cli.md\|core.md\|requirements.md\|failure-modes' crates tests xtask` finds only comments and one core.md test about event types.

## Inferred, not verified

- The unit sizes (start about 520 lines, budget 210, stophold 260, closestat 380, skill 450) are my estimates from the code I read. Nobody has measured them.
- Members run in tmux panes. The dry run said "via tmux" for every member, so they will meet the tmux test failure. The skip in the checks covers it.

## Fix round (planrev.md)

The lead's reprompt m001 asked for one fix round on `reports/planrev.md`. I checked each of the nine
findings, and the small note on the pointer line, against the code. All ten hold. None is rejected.
Each fix is in plan.md, the briefs, spec-wave1.json, the patch and decisions D27 to D36.

| # | Finding | Verdict | Fix | Evidence |
| --- | --- | --- | --- | --- |
| 1 | The closer's full gate inside tmux stops at the tmux test, so e2e, crash, mcp, `cargo doc` and the docs check never run | Holds | D27. plan.md "For the closer" gives the detached command, with an `rm -f` of the old log first. D23 is amended. Planning-check item 2 and summary risk 1 now say the full gate runs every step | From this tmux pane, `cargo test -p teamwire-cli --locked --test tmux_wiring` run under a double fork passed 4 of 4, exit 0. `scripts/gate.sh` run the same way on afk/base plus the final patch printed `gate 1/6 fmt: ok` through `gate 6/6 docs: ok` (test 236 s) and `gate exit 0` |
| 2 | afk start's success output and its exit-2 path have no test | Holds | D28. Two tests added to start's unit 3 and the AFK-4 row: `afk_start_prints_the_run_and_exits_zero`, `afk_start_that_cannot_copy_the_task_exits_two_and_names_the_team`. The brief gives the recipe. start owns the new `crates/teamwire-cli/tests/support/healthy.rs` | `runs_session` at `crates/teamwire-harness-claude/src/session.rs:122-130` accepts a live pid when `startedAt` is missing. `team_names` at `crates/teamwire-store/src/store.rs:115-133` skips a directory without `team.db`. `Bed::healthy` is at `crates/teamwire-cli/tests/doctor.rs:140`, `teamwire()` at 802, the binary test at 833 |
| 3 | start must reuse the `lead_cwd` lookup, which sits inside budget's `member.rs` | Holds | D29. The patch now extracts `pub(crate) fn new_team(name: &TeamName, title: &str, lead: &LeadPlan) -> NewTeam` and `pub(crate) fn lead_cwd(registry: &Registry, lead: &LeadPlan) -> Option<String>`; `create_team` calls both. The start and budget briefs and the contract section say so | In the scratch clone: `cargo fmt --all`; `scripts/gate.sh fast` fmt, tidy, clippy ok; `cargo test -p teamwire-launch -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux` 403 passed, exit 0; `git apply --check` of the new patch on the repository's afk/base: clean |
| 4 | closestat's unit order leaves a `--json` commit without its zod copy | Holds | D30. Unit 2 is status plus its zod change; unit 3 is teams plus its zod change. Unit 4 and "if you prefer" are gone. Done-when names the AGENTS.md rule | AGENTS.md "Its zod schemas copy the CLI's --json output, so a change to that output updates ui/src/shared/teamwire.ts in the same commit" |
| 5 | The budget-spent note drops "decide it yourself", and the test name describes text the note does not have | Holds | D31: the new text keeps the decide-and-log sentence, with the team tag first. Test renamed `past_the_budget_the_afk_note_says_budget_spent_after_the_team_tag` in the stophold brief and the AFK-9 row. D16 is amended | Design line 150 makes the budget-spent note the base note with a prefix |
| 6 | The skill's install test lives in `src/`, and nothing tests doctor's check of the AFK skill | Holds | D32. Two tests in `crates/teamwire-harness-claude/tests/lead_setup.rs`: `install_writes_the_afk_skill_as_a_second_stamped_skill` and `a_plugin_without_the_afk_skill_or_with_an_edited_one_is_a_problem`. The AFK-12 row cites them | `ClaudeConfig::plugin_problems` is public at `crates/teamwire-harness-claude/src/lead_setup.rs:188`; `plugin_problems_name_what_is_missing_stale_or_off` in tests/lead_setup.rs (near line 530) already calls it with `files = true` |
| 7 | Five named tests pin today's behaviour, so "fails without the change" cannot hold for them | Holds | D33. Every brief's Done-when says new-behaviour tests fail without their change and (guard) tests pass on afk/base too. The five tests are marked (guard) in the briefs and the plan. plan.md tells the closer not to revert-check them | The five tests' purposes as written in the first briefs |
| 8 | The budget and stophold checks skip the crates that depend on theirs | Holds | D34. budget runs launch and cli; stophold runs report, mcp and cli. Spec, plan.md checks table, planning-check item 1, and both briefs' section 12 updated | Dependents from `git grep -l '^teamwire-<crate>\b' afk/base -- 'crates/*/Cargo.toml'`: launch <- cli; report <- cli, mcp; close <- cli; watch, msg <- cli, mcp; harness-claude <- cli. Run once in the scratch clone: budget's check 403 passed, stophold's 405 passed, each exit 0, then gate fast ok |
| 9 | A decision log written in the design's indented layout counts as zero decisions | Holds | D35. The count matches `^\s*D[0-9]+\s`; closestat's test includes an indented entry. The skill brief says to show the example at column 0 in a fenced block. D18 is amended | Design section 6 shows `    D3  2026-10-19 14:05  ...`, indented by four spaces |
| 10 | The pointer line has a second sentence the design does not have | Holds | D36: the design's one sentence. Skill brief section 2 item 2 updated. D21 is amended | Design line 187 |

Other checks in this round:
- `afk/spec-wave1.json` was regenerated by a script. The script asserted that no path is owned twice
  (59 paths), that every title is at most 28 characters, that model, effort, base and plugins are
  as decided, and that every member's `owns` and `check` equal plan.md's tables.
- The dry run: `env -u TEAMWIRE_TEAM -u TEAMWIRE_ROLE -u TEAMWIRE_TOKEN -u TEAMWIRE_TOKEN_FILE
  TEAMWIRE_HOME=<empty temp dir> <scratch clone>/target/debug/teamwire launch --dry-run --spec
  afk/spec-wave1.json`, run from the repository root. It printed five members, then `nothing was
  created`, and exited 0. The temporary home stayed empty.
- The repository is unchanged: `git status --short` prints nothing, and the branch is afk/base.
- plan.md's summary is 30 lines (the awk count of the handoff).
- tidy prints `WEAKENED crates/teamwire-cli/src/cmd/afk.rs:14` for the stub's `#[expect(` and
  still exits 0. The plan's contract section says so, so the lead is not surprised.

## Decisions I made

D6 to D36 (D6 to D26 in the first pass, D27 to D36 in the fix round).
The full text of each is in `$TEAMWIRE_HOME/teams/afkmode/afk/decisions.md`:

- D6: five implementers (start, budget, stophold, closestat, skill), no shared files.
- D7: the lead commits the `afk start` argument skeleton (the patch) before launch.
- D8: start owns doctor.rs and adds `launch_checks(&Inputs, &HostPick)`. Install and doctor need no change for the skill.
- D9: the order of afk start's refusals, the codes and the output.
- D10: `--budget` units come from `parse_duration`. Zero is refused.
- D11: `lead_cwd` comes from `session_cwd`, else the current directory.
- D12: the five success lines.
- D13: the afk start steps live in `teamwire_launch::afk` and are tested with launch's fakes.
- D14: the budget refusal is a per-member outcome in `launch_all`. Nothing is recorded for the refused member.
- D15: the model-table rule and the warning text.
- D16: the exact hold-note texts. The watch note comes first.
- D17: the heading rule for close's report check, the refusal texts and their scope.
- D18: the status object and line, and how decisions are counted.
- D19: the `TeamEntry` field and the `(open, afk)` marker.
- D20: the zod schema change. Fixtures stay unchanged.
- D21: how the skill is packaged, and the pointer line.
- D22: the skill member owns every doc. The test names are fixed in plan.md.
- D23: the checks skip `herdr_env_wins_over_tmux` and use `--no-fail-fast`. Members do not run test-changed.sh.
- D24: setup lines, branch names and deadlines (1.5 times normal).
- D25: no promptwire skills.
- D26: the planner's clone lives in the scratchpad, not in afk/.
- D27: the closer runs the full gate detached from tmux by a double fork, into afk/closer-gate.log. Amends D23.
- D28: two binary tests for afk start's success output and its exit-2 path, with a spawned child as the lead. Amends D13.
- D29: the contract amendment also extracts `new_team` and `lead_cwd` from launch's `create_team`. Amends D7 and D11.
- D30: closestat commits each `--json` change with its zod copy.
- D31: the budget-spent hold note keeps "decide it yourself"; the test is renamed. Amends D16.
- D32: the AFK skill's install and doctor tests live in harness-claude's tests/lead_setup.rs.
- D33: five tests are marked (guard); the closer does not revert-check them.
- D34: the budget and stophold checks also run the crates that depend on theirs. Amends D23.
- D35: decision lines may start with white space; the skill shows the example at column 0. Amends D18.
- D36: the main skill's pointer line is the design's one sentence. Amends D21.
