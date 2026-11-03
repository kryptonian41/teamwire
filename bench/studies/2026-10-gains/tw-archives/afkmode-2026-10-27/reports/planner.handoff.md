# Handoff: planner, fix round on the plan review

You are `planner` again: an investigator. You write only under
`$TEAMWIRE_HOME/teams/afkmode/afk/` and your report file, never the repository. Here
`$TEAMWIRE_HOME` is `/private/tmp/claude-501/bnruns/bn4tu2e1us/twhome`, and the repository is
`/private/tmp/claude-501/bnruns/bn4tu2e1us/clone`. The scratch clone is
`SP=/private/tmp/claude-501/-private-tmp-claude-501-bnruns-bn4tu2e1us-clone/b72fc146-7394-4eb6-a26e-1f8b343d5a2d/scratchpad`
(`$SP/base` is a clone of afk/base with the amendment patch staged but not committed, and a local
`main` branch). If that path is gone, make a new clone with
`git clone -q --branch afk/base <repo> <tmpdir>`, then `git branch main origin/main`, then
`git apply` the patch.

## Done

- The first pass is finished, with no commits because this role does not commit. It wrote
  `afk/plan.md`, `afk/briefs/{start,budget,stophold,closestat,skill}.md`, `afk/spec-wave1.json`,
  `afk/amend-afk-args.patch` and decisions D6 to D26 in `afk/decisions.md`. The report is at
  `reports/planner.md`.
- The lead's reprompt m001 asks for one fix round on `reports/planrev.md`, which lists 9 gaps. The
  message: "Fix the plan, briefs, spec and patch for every gap that holds, and answer the rest with
  evidence in your report. For gap 1, find a way for the closer's full gate to run every step ...
  and write it into plan.md for the closer. Add D entries for what you settle. This is the only fix
  round for the plan. Record done when finished." Inbox m001 is already read.
- I checked every gap against the code. **All 9 hold**, and so does the small pointer-line note.
  The verdicts and fixes are under Next. None of the edits are made yet.
- The fix for gap 1 is proven for one test file. I ran
  `( { cargo test -p teamwire-cli --locked --test tmux_wiring; echo "exit $?"; } > $SP/detached.log 2>&1 & )`
  from the Bash tool, and `herdr_env_wins_over_tmux ... ok` passed 4 of 4 with exit 0. The double
  fork leaves the test with a parent chain that holds no tmux. `nearest_host` in
  `crates/teamwire-cli/src/cmd/doctor.rs:137-157` then finds no tmux or herdr, so herdr wins.
- **The detached full gate finished: all six steps ok, `gate exit 0`.** The run was afk/base plus
  the first patch, in `$SP/base`, and printed `gate 1/6 fmt: ok`, `2/6 tidy: ok`, `3/6 clippy: ok`,
  `4/6 test: ok (227s)`, `5/6 doc: ok (5s)`, `6/6 docs: ok`. This proves gap 1's fix; cite it.
  (Earlier note, kept for reference:) The full gate is running the same detached way in `$SP/base`. Its log is `$SP/fullgate.log`, and
  its last line will be `gate exit <n>`. The gate process does not depend on my tab, so it keeps
  running. Read the log: if it holds `gate exit 0`, gap 1's fix is proven for all six steps. If it
  is not finished yet, wait with a background `until grep -q '^gate exit' $SP/fullgate.log; do sleep 10; done`.
  If it failed, read which step failed, and say in the plan which failure is caused by the base.

## Next, in order (each gap: verdict, then the exact fix)

Add new decisions from D27 on, in the format of `afk/decisions.md`. Use a time in UTC, around
`2026-10-27 14:10`. When a new entry changes an old one, add one line `Amended by D<n>.` under the
old entry's `Reverse by:` line. Then update plan.md, the briefs and the spec to match.

1. **Gap 1 (holds): the full gate stops inside tmux.** Write a new D entry: the closer runs
   `scripts/gate.sh` detached from tmux, with the double fork, and waits for the log. Proof: the
   tmux_wiring run above and `$SP/fullgate.log`. Add a "For the closer" block to plan.md that
   replaces the note at plan.md "Notes for the reviewers and the closer", second bullet:

   ```bash
   cd <the closer's clone, on the joined branch>
   ( { scripts/gate.sh; echo "gate exit $?"; } > "$TEAMWIRE_HOME/cache/closer/afkmode/gate.log" 2>&1 & )
   until grep -q '^gate exit' "$TEAMWIRE_HOME/cache/closer/afkmode/gate.log"; do sleep 10; done   # run in the background, or use Monitor
   ```

   Give the reason in the block: the double fork reparents the gate to launchd (pid 1), so no tmux
   is among the test's parents, and all six steps run. Also amend D23: the members' checks keep the
   skip (it is fine for a targeted check), but the full gate needs no skip. Fix planning-check item
   2's text and the summary's risk 1 to match.
2. **Gap 2 (holds): the success output of `afk start` and its exit-2 path have no test.** Add two
   named CLI tests to the start brief, unit 3, and to the plan's AFK-4 row, in
   `crates/teamwire-cli/tests/afk.rs`:
   - `afk_start_prints_the_run_and_exits_zero`: the five D12 lines on stdout, exit 0, `afk/task.md`
     bytes equal, and the row has `afk_budget_ends_at`.
   - `afk_start_that_cannot_copy_the_task_exits_two_and_names_the_team`: set up the same way, but
     create `<home>/teams/<team>/afk` as a regular file first. `team_exists` checks only `team.db`,
     so the team is still created, and then the task copy fails.

   The recipe goes in the start brief. The binary passes doctor with the healthy setup of
   `crates/teamwire-cli/tests/doctor.rs` (`Bed::healthy()` at line 140,
   `the_binary_exits_zero_when_all_pass_and_two_when_one_fails` at line 833, `FakeBins` on `PATH`,
   `ignoring_repo()` as the cwd, `HERDR_ENV=1`). The lead process is a live child that the test
   spawns, for example `sleep 60`, plus a session file `<claude dir>/sessions/<pid>.json` holding
   `{"pid": <pid>, "sessionId": "<sid>", "kind": "interactive", "cwd": "<dir>"}` with **no
   `startedAt`**. `runs_session` in `crates/teamwire-harness-claude/src/session.rs:122-130` then
   skips the start-time check. Set `TEAMWIRE_CLAUDE_DIR` and `CLAUDE_CODE_SESSION_ID=<sid>`. On
   macOS the real sleep hold (`caffeinate`) starts and ends when the test kills the child. On Linux
   the platform has no hold. Start may move `Bed::healthy` into a new support file it owns,
   `crates/teamwire-cli/tests/support/healthy.rs`: add that path to start's owns in plan.md, the
   brief and the spec. Write a D entry for this test approach.
3. **Gap 3 (holds): the `lead_cwd` lookup sits in budget's `member.rs`.** Extend the contract
   amendment. In `$SP/base`, extract from `create_team` in `crates/teamwire-launch/src/member.rs`
   (lines 92-120) two helpers that `create_team` then calls, so nothing is left unused:
   `pub(crate) fn new_team(name: &TeamName, title: &str, lead: &LeadPlan) -> NewTeam` (title through
   `crate::plan::clean`) and `pub(crate) fn lead_cwd(registry: &Registry, lead: &LeadPlan) -> Option<String>`.
   Keep the comment "The folder lets `reopen` resume the lead ...". Run `cargo fmt --all`,
   `scripts/gate.sh fast` and `cargo test -p teamwire-launch --locked`. Regenerate the patch with
   `git add -A crates && git diff --cached > $TEAMWIRE_HOME/teams/afkmode/afk/amend-afk-args.patch`,
   then run `git -C <repo> apply --check <patch>`. Update the start brief (sections 8 and 9: call
   `crate::member::new_team` and `crate::member::lead_cwd`, never edit member.rs), the budget brief
   (member.rs already has these two helpers; leave them alone), plan.md's "Contract amendments", and
   D7, or add a new D entry that amends it.
4. **Gap 4 (holds): closestat's commits would split a JSON change from its zod copy.** Reorder
   closestat's units. Unit 2 is the status view, the CLI line, the zod `AfkView` and
   `StatusView.afk`, and their zod tests, in one commit. Unit 3 is the teams key and marker, the zod
   `TeamEntry` key, and its zod test, in one commit. Drop unit 4 and the "if you prefer" wording.
   Say in the brief: AGENTS.md requires the `--json` change and `ui/src/shared/teamwire.ts` in the
   same commit.
5. **Gap 5 (holds): the budget-spent note drops "decide it yourself".** New text, with the team tag
   still first:
   `[teamwire:<team>] budget spent at <ts>: AFK run: nobody will answer you. If you were about to ask the user, decide it yourself, add it to <dir>/afk/decisions.md, and carry on. Launch no new implementer: finish what is running, then add the reporter, and run `teamwire close <team>` once <dir>/afk/REPORT.md exists. If a command you started will wake you, stop again.`
   Rename the test `past_the_budget_the_afk_note_starts_with_budget_spent` to
   `past_the_budget_the_afk_note_says_budget_spent_after_the_team_tag` in the stophold brief and in
   the plan's AFK-9 row. Write a D entry that amends D16. Reason: every teamwire line starts with
   `[teamwire:<team>]`, and the design's base note does too.
6. **Gap 6 (holds): the install test lives in `src/`, and nothing tests the doctor part of
   AFK-12.** Replace the named `plugin.rs` unit test with tests in
   `crates/teamwire-harness-claude/tests/lead_setup.rs`:
   - `install_writes_the_afk_skill_as_a_second_stamped_skill`: the file at
     `teamwire/skills/afk/SKILL.md` is stamped, and the count goes to 7 at line 184.
   - `a_plugin_without_the_afk_skill_or_with_an_edited_one_is_a_problem`: uses the public
     `ClaudeConfig::plugin_problems(want, true)`; see the existing `problems(true)` use near line
     534.

   Update the skill brief, unit 2, and the plan's AFK-12 row. The `src/` unit test may stay as an
   extra.
7. **Gap 7 (holds): regression-guard tests cannot fail without the change.** In every brief's "Done
   when", write: "each test of new behaviour fails without its change; a test marked (guard) pins
   today's behaviour and passes on afk/base too." Mark these as (guard):
   `a_team_without_a_budget_is_unaffected`, `an_ordinary_team_gets_no_model_warning` (budget);
   `removing_one_member_of_an_afk_team_needs_no_report`, `an_ordinary_team_closes_without_a_report`
   (closestat); `add_to_an_afk_team_with_no_members_starts_the_member` (start). Add a line to
   plan.md for the closer: do not revert-check the guard tests.
8. **Gap 8 (holds): two checks skip the crates that depend on the changed crate.** Dependents, from
   the Cargo.toml files: launch <- cli; report <- cli, mcp; close <- cli; watch and msg <- cli, mcp;
   harness-claude <- cli. The new checks:
   - budget: `cargo test -p teamwire-launch -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux && scripts/gate.sh fast`.
     This runs the same packages as start's check, which passed with 403 tests.
   - stophold: `cargo test -p teamwire-report -p teamwire-mcp -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux && scripts/gate.sh fast`.

   **Run the stophold command once in `$SP/base`** and record its counts. Update the spec (both
   `check` strings), plan.md's checks table and planning-check item 1, and both briefs' section 12,
   with the base counts. Write a D entry.
9. **Gap 9 (holds): an indented log line counts as zero decisions.** Two fixes:
   - Count lines matching `^\s*D[0-9]+\s` (leading white space allowed) in D18 and the closestat
     brief. The test must include an indented entry.
   - The skill brief tells the skill to show the decision example at column 0 inside a fenced
     block, so a lead copies `D3  ...` without indentation.

   Write a D entry that amends D18.
10. **The pointer line (minor, holds):** use the design's sentence only: "For a run the user will
    not be present for, use `/teamwire:afk`." Update D21 with an amend line and the skill brief's
    section 2 item 2.
11. Regenerate `afk/spec-wave1.json` with the script logic of the first pass. It is a Python
    heredoc that asserts no path is owned twice and every title is at most 28 characters. Add
    `crates/teamwire-cli/tests/support/healthy.rs` to start's owns and change the two `check`
    strings. Run `$SP/base/target/debug/teamwire launch --dry-run --spec <spec>` with
    `env -u TEAMWIRE_TEAM -u TEAMWIRE_ROLE -u TEAMWIRE_TOKEN -u TEAMWIRE_TOKEN_FILE TEAMWIRE_HOME=$SP/twtmp`
    from the repository root. It should exit 0 and create nothing.
12. Keep plan.md's summary at 30 lines or fewer. Recount with
    `awk '/^## Summary/{f=1;next} /^## Contract amendments/{f=0} f' plan.md | wc -l`.
13. In `reports/planner.md`, add a "Fix round (planrev.md)" section: one row per gap, with the
    verdict, the fix, and the evidence (file:line or command and output). Add the new D numbers to
    **Decisions I made**.
14. Last: `teamwire done --status done --summary "<one line>"`.

## Open questions

None. No question to the lead is outstanding.

## Files and commands

- Review: `$TEAMWIRE_HOME/teams/afkmode/reports/planrev.md`.
- What to edit: `$TEAMWIRE_HOME/teams/afkmode/afk/{plan.md,decisions.md,spec-wave1.json,amend-afk-args.patch}`
  and `afk/briefs/*.md`.
- The first pass's base counts, all exit 0 on afk/base plus the patch: start 403 tests; budget
  115 (`-p teamwire-launch` only); stophold 370 (report and cli); closestat 562 Rust tests, 162 bun
  tests and the typecheck; skill 434 tests and the docs check. gate fast was ok in each run.
- Run cargo commands only in `$SP/base`, never in the repository.

## Decisions so far

D6 to D26 are in `afk/decisions.md`; the reasons are there. In this round, all 9 gaps and the
pointer-line note hold. None is rejected.

## Skills to load

None needed. The plain-language output style applies to everything you write.
