# Review of branch afkmode/skill (reviewer rev_skill)

## Summary

- The work is sound. I found no correctness, security or data-loss problem in the Rust change.
- The AFK skill file matches design section 6 word for word. The only difference is one re-wrapped line in section 7 of the skill. No rule is missing or reworded.
- Install renders the skill as `teamwire/skills/afk/SKILL.md`, stamped like the main skill and counted in the plugin version hash. Doctor's plugin check covers the file with no code change.
- Each of the three new tests fails without its fix. None is stubbed or hard-coded to pass.
- The binding names (D5 to D8, D18, D20, task.md) in the docs match the code on the start, stophold and endings branches.
- Four low findings, all about text. (1) The main skill's new pointer line also reaches the Codex lead's skill, which has no AFK skill. (2) and (3) Two doc rows name test files that do not test what the row claims. (4) Two checkable rules of design 5.2 and 5.3 have no `AFK-` requirement row.
- Tests I ran on the branch head cf2fbd6 are listed under "Checks I ran".

## Findings, most harmful first

### 1. Low: the pointer line sends a Codex lead to a skill its plugin does not have

- File: `examples/skills/teamwire/SKILL.md:18`, the line "For a run the user will not be present for, use `/teamwire:afk`."
- The Codex plugin builds its own `teamwire:teamwire` skill from the same main skill text. `teamwire install` passes the Claude harness's `SKILL_TEXT` to the Codex install plan (`crates/teamwire-cli/src/cmd/install.rs:238`). The Codex plugin then renders the shared body into its own skill (`crates/teamwire-harness-codex/src/lead_setup/plugin.rs:117-144`). The Codex plugin renders no `skills/afk/SKILL.md`.
- Failing scenario: a user leads from Codex and says "do this while I am away". The Codex lead's skill now tells it to use `/teamwire:afk`. That skill does not exist in the Codex plugin, and Codex invokes skills with `$`, not `/`. The Codex lead follows a pointer to nothing. Before this branch, the Codex lead's skill had no such line.
- The design says a Codex lead keeps today's behaviour: "Which lead harness gets AFK? | Claude Code only. A Codex lead keeps today's behaviour." (design section 4). The new ADR repeats the rule at `docs/adr/0025-afk-runs.md:85`. The design also asks for the pointer line in the main skill (section 5.6), and the main skill is shared with Codex. The implementer followed section 5.6 exactly. So the two design statements conflict, and the parent has to choose between them. The brief puts the Codex plugin out of scope for this branch.

### 2. Low: AFK-8 names a test file that tests neither status nor teams

- File: `docs/requirements.md:309`, the AFK-8 row. The row gives "Test: `crates/teamwire-watch/tests/afk.rs`, `crates/teamwire-cli/tests/afk.rs`."
- The CLI test file comes from the start branch, and it tests only `teamwire afk start`. Its tests are `a_start_creates_the_team_the_task_copy_and_the_hold` and seven refusal tests. No test in it runs `teamwire status` or `teamwire teams`.
- The tests for the claims in AFK-8 are on the endings branch:
  - `crates/teamwire-cli/tests/status.rs::an_afk_team_prints_one_afk_line_after_the_team_line`
  - `crates/teamwire-cli/tests/teams.rs::an_afk_team_is_marked_and_carries_its_budget_end`
  - `crates/teamwire-mcp/tests/teams.rs::team_teams_gives_an_afk_team_its_budget_end_and_others_null`
- Failing scenario: someone checks AFK-8 by opening the cited CLI file. They find no assertion about the status line, the `teams` mark or `afk_budget_ends_at`. The `teams` half of AFK-8 then looks untested.
- Brief line: "Requirement ids use a new `AFK-` prefix ... one per checkable rule of design 5.2 to 5.6, each naming the test file that checks it". The brief's own list of test files left out status.rs and teams.rs, which is the likely cause.

### 3. Low: a failure-mode row claims test coverage that the cited files do not have

- File: `docs/failure-modes.md:147`, the row "The AFK team has no member yet".
- The row says "Status, watch, close, reopen, verify and metrics read the team as they read any team. The watch stays quiet, because it prints `TEAM-COMPLETE` only after a member settles". The row cites `crates/teamwire-cli/tests/afk.rs`, `crates/teamwire-watch/tests/afk.rs` and `crates/teamwire-close/tests/afk.rs`.
- Here is what each cited file covers for a team with no members:
  - The CLI file checks that `afk start` leaves the member list empty.
  - The watch file builds the status view (`an_afk_team_with_no_members_renders`).
  - The close file plans a close of a memberless AFK team.
- No cited test runs reopen, verify or metrics on a memberless team. No cited test runs the watch loop and checks that it stays quiet.
- Failing scenario: a reader trusts the "Test that proves it" column for reopen, verify, metrics or the quiet watch. No test in those files proves any of the four.

### 4. Low: two checkable rules of design 5.2 and 5.3 have no AFK requirement row

- File: `docs/requirements.md:302-311`, the `AFK-` table.
- Rule one, from design 5.2: "Check that `--add` accepts a team that has no members yet." The start branch tests it in `crates/teamwire-launch/tests/afk.rs::adding_to_an_afk_team_with_no_members_starts_the_member`. No `AFK-` row states it.
- Rule two, from design 5.3: "A team row that cannot be read means no AFK hold for that team." The stophold branch tests it (commit e5e8e23, "Make the unreadable-team AFK test reach the read error"). AFK-5 (`docs/requirements.md:306`) does not state it.
- Failing scenario: someone uses the requirement table as the list of what the AFK runs must keep. A later change could break either rule, and nothing in the table points at the test that guards it.
- Brief line: "one per checkable rule of design 5.2 to 5.6, each naming the test file that checks it".

## What I checked and found sound

- The skill text. `diff` of design section 6 (the fenced block) against `examples/skills/afk/SKILL.md` shows one re-wrap at the reporter's input paths, and no other change. All five fixed rules are present: the model table, the decision-log form, the eight report headings and the boundaries.
- The pointer line. The text is word for word as the brief gives it, and it sits near the top of the main skill.
- The render. `crates/teamwire-harness-claude/src/lead_setup/plugin.rs:65` stamps the AFK text, and the render fails with an error rather than a panic when the front matter is missing. Line 90 adds the file to the body list, so the file is in the version hash (lines 92-99). `plugin_matches` (line 142) compares every body file, so doctor and install both see a changed or missing AFK file. An older install with six files reads as "another version", and `teamwire install` then updates it. `write_plugin` creates the new `skills/afk/` folder. `teamwire uninstall` removes the whole plugin folder, so no AFK file is left behind.
- The tests, each checked against the code without its fix:
  - `the_seven_files_name_the_binary_and_the_hook` fails without the new body entry.
  - `the_afk_skill_is_the_compiled_in_text_and_changes_the_version` fails if the AFK file is missing from the version hash.
  - `an_installed_plugin_whose_afk_skill_differs_is_not_current` (`crates/teamwire-harness-claude/tests/lead_setup.rs:559`) fails without the render entry, because the file would not exist and the read would panic.
  - `the_afk_skill_is_named_afk_and_can_be_stamped` checks that the front matter starts with `---\nname: afk\ndescription: `.
- The hard rules. The diff adds no unwrap or expect outside tests, no new dependency and no Cargo.lock change. The only protected file it touches is the new `docs/adr/0025-afk-runs.md`, which D15 allows.
- The ADR records the options the brief names: the rejected separate `afk/` home, `afk finish` and `afk status`.
- The docs text against the other branches' code:
  - the refusal codes and output lines of `afk start` (start branch, `cmd/afk.rs`), with refusal exit code 1;
  - launch's `afk_budget_spent` text, the model warning text and exit code 2 (start branch, `teamwire-launch/src/afk.rs`);
  - both hold notes (stophold branch, `teamwire-report/src/afk_hold.rs:83` and `:87`);
  - the close refusal texts and how close matches headings (endings branch, `teamwire-close/src/afk.rs`);
  - the status line, the `teams` mark and the MCP `afk_budget_ends_at` key (endings branch).

## Checks I ran

I ran each check on the skill worktree at cf2fbd6, with a build directory in my scratchpad, so nothing was written into the implementer's worktree.

- `cargo test -p teamwire-harness-claude --locked --no-fail-fast`: exit 0. Every suite printed `test result: ok`.
- `cargo test -p teamwire-harness-codex -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux`: exit 0. All 33 suites printed `test result: ok`, with no failure.

I did not run `scripts/gate.sh`, as the brief says.

## Decisions I made

- I ran the implementer's test commands against their worktree's source with `CARGO_TARGET_DIR` in my own scratchpad. The rules forbid writing into another member's directory, and a normal `cargo test` there writes `target/`.
- I also ran the Codex harness tests. The main skill text is compiled into the Codex plugin, and `scripts/test-changed.sh` selects crates by changed source files, so it would not pick up a change under `examples/`.
- I rated finding 1 low and left the choice to the parent. The design asks for both the pointer line and an unchanged Codex lead, and the brief puts the Codex plugin out of scope.
