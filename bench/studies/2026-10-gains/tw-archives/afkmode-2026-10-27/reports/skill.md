- b414d9c AFK skill text, main-skill pointer line, three preset.rs tests | left: install (AFK_SKILL_TEXT, render, 2 lead_setup tests), docs
- 918c5b9 AFK_SKILL_TEXT rendered as teamwire/skills/afk/SKILL.md; two lead_setup tests, file count 7 | left: docs (unit 4), final check
- 02e8bfd ADR 0025, AFK-1..12 in requirements.md, failure-modes.md section 3 | left: api docs, ARCHITECTURE, README, CONTEXT, final check
- ef5d4c7 cli.md, core.md, mcp.md, ARCHITECTURE.md, README.md, CONTEXT.md for AFK runs | left: final check, report

# Report: skill (branch afkmode/skill)

## Check (last commit, from the worktree root)

`cargo test -p teamwire-harness-claude -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux`
exit 0, 440 tests passed, 0 failed (434 on afk/base plus the 6 new ones).
`scripts/check-docs.sh`: `docs check: ok`.
`scripts/gate.sh fast`: fmt ok, tidy ok, clippy ok. Tidy printed
`PROTECTED-CHANGED docs/adr/0025-afk-runs.md` (expected, brief section 11) and
`WEAKENED crates/teamwire-cli/src/cmd/afk.rs:14` (the lead's stub, not my file; the plan notes it).
`teamwire --help` on this branch lists `afk`, so the command test ran.

## Tests added (each failed before its change: red run seen for the two lead_setup tests and the file count; the preset tests were written with the skill)

- `crates/teamwire-harness-claude/tests/lead_setup.rs::install_writes_the_afk_skill_as_a_second_stamped_skill`
- `crates/teamwire-harness-claude/tests/lead_setup.rs::a_plugin_without_the_afk_skill_or_with_an_edited_one_is_a_problem`
- `crates/teamwire-cli/tests/preset.rs::the_afk_skill_names_only_commands_and_tools_that_exist`
- `crates/teamwire-cli/tests/preset.rs::the_afk_skill_lists_the_eight_report_headings`
- `crates/teamwire-cli/tests/preset.rs::the_main_skill_points_at_the_afk_skill`
- extra: `skill.rs::the_afk_skill_can_be_stamped_and_has_no_metadata_of_its_own`
- changed: file count 6 to 7 (`tests/lead_setup.rs`), `plugin.rs::the_seven_files_name_the_binary_and_the_hook` (exact path list)

## New pub items

None. `AFK_SKILL_TEXT` is `pub(super)` in `lead_setup/skill.rs`, like `REFERENCE_TEXT`.
`lead_setup.rs` and `lib.rs` needed no change.

## Section-6 rules and the skill line that keeps them (examples/skills/afk/SKILL.md)

| Rule of design section 6 | Line(s) |
| --- | --- |
| Frontmatter name afk, description, no metadata key | 1-4 |
| Read the teamwire skill first; this file wins on conflict | 8-10 |
| Never wait; answer yourself and log it; a question ends the run; step 1 only stop | 14-19 |
| Preflight: bypass/auto mode check, else restart and stop | 22-24 |
| Task word for word to a file; name and title; `afk start`; `--budget` only when named | 25-33 |
| On refusal or FAIL tell the user and stop (refused:/failed:; D9) | 35-41 |
| Reply once with team, report path, budget end, "Keep the lid open..."; start step 2 same turn, no waiting (D12) | 43-52 |
| One team; every member via `launch --add`; never a second team | 54-55 |
| Five fixed things: never wait, protect context, closer, reporter, boundaries | 58-75 |
| Everything else is the lead's choice | 74-75 |
| Context rules: members do the phases; pass by path; read only result line and summary; no raw diffs/logs; members read each other's files; each member runs once | 77-98 |
| Member profiles table, unsure means add, max only for hard task, deadline_s x1.5, briefs in afk/briefs, files under afk/ | 100-121 |
| Decision-entry form (column 0, D35); answers go into briefs; no open-questions section; unsettled question cuts the unit | 123-140 |
| D1 roster entry before first launch; roster changes get an entry | 142-147 |
| While working: answer questions in-turn, never UNANSWERED | 150-153 |
| Review findings by path, one turn, owner checks, two fix rounds, no second closer, `verify` after each round | 154-158 |
| Integration branch afk/<team>, merge in plan order, abort and reprompt on conflict | 159-162 |
| TEAM-STALLED: status, read pane, replace once, then cut | 163-165 |
| Budget: status line, last implementer by two thirds, refusal past budget, finish and report rest | 166-171 |
| Stop hold and its notes (D16, D31) | 177-193 |
| Report: reporter inputs, `teamwire metrics`, Decisions I made lists, eight `##` headings in order (D17) | 195-217 |
| Close: refuses until headings present, reprompt reporter, archive, branches kept, hold released, PushNotification | 219-227 |
| A run always ends with a report; partial result with an honest report is the goal | 229-232 |

(Line numbers are from the file as committed; check with `grep -n`.)

## Decisions I made

- `AFK_SKILL_TEXT` is `pub(super)`, not `pub`: no test or caller outside the crate needs it, and the brief lists no new pub item.
- No new plugin.rs test for "a changed AFK text changes the plugin version": the AFK file sits in the same hashed `body` as the main skill, and `render_with` has no seam for it without a signature change.
- The skill shows the stop-hold notes (D16 and D31 text) in a short subsection of section 6; the design's section 6 has no such text, but the brief asked for D31's text.
- The skill says a `warning:` line from `afk start` is not a refusal and goes into the report; the design does not say what to do with a warning.
- The skill says a `failed:` (exit 2) start leaves a team that exists, and tells the lead to stop and tell the user.
- The eight report headings are shown in a fenced block with one gloss line each, so the `##` lines are not headings of the skill itself.
- Pointer line placed after the first paragraph of the main skill (line 15), as the design's sentence only (D36).
- `--budget` units documented as `s`, `m`, `h`, `d` (D10 text is ambiguous about a fifth unit).
- failure-modes section is `## 3. AFK runs`, with one extra row for the accepted risk (a lead that stops twice, a usage limit), marked "None: accepted design, no test", as the file does elsewhere.
- Requirement Source column uses the design section of the behaviour: 5.2 for AFK-1..6, 5.4 for 7 and 8, 5.3 for 9, 5.5 for 10 and 11, 5.6 for 12.
- The docs state behaviour from decisions D9-D19 and the plan table only; I did not see the other branches' code. The closer compares them with the joined code. The failure-modes rows for AFK-3, AFK-4 and AFK-8 describe details (which doctor checks, hold warning wording) at the level of D8, D9, D12 and D15.
- 55842b8 review fixes (findings 1-7) and D39: docs, ADR, requirements, skill | left: nothing

## Review round (revskill.md, D39)

Each finding was checked against the other branches' code (`git show afkmode/<role>:<file>`).

| # | Verdict | Evidence and fix |
| --- | --- | --- |
| 1 | Holds | closestat `afk_report.rs` refuses an open AFK team without the report. D39 changes this for a team with no member (`members.is_empty()` returns none). cli.md (afk start, close), failure-modes, ADR, AFK-10 and the skill now say a team that never had a member closes without a report, and a team whose only member was removed still needs one. |
| 2 | Holds | `store.rs:158-220` refuses a newer schema on a read-only open (ADR 0015 point 4). ADR 0025 is unmerged and new, so I corrected it in place (not a supersede). Consequence now says an older binary refuses with the schema-newer error. |
| 3 | Holds | The skill gave a paraphrase. It now quotes D31's note in full with the team tag first. |
| 4 | Holds | `lead_setup_check.rs` gives FAIL for a plugin problem. failure-modes AFK-12 row corrected (doctor FAIL, `doctor_failed` at start). |
| 5 | Holds | start `afk.rs:207,217` return `Failed` with no team row and no team name; `:227` names the team. Skill: "When it names a team, that team now exists". cli.md says the same. |
| 6 | Holds, all five | decisions rule now says leading white space is allowed; third refusal form `cannot be read` added (and a failure-modes row citing `close_says_an_unreadable_afk_report_is_unreadable_not_missing`); `unknown_harness` and `unknown_host` added to the order table; exit 2 row of the exit-code table names `afk start`; uninstall row says "every skill file". I also fixed the launch refusal line to `<role>: refused: afk_budget_spent: ...` (budget `cmd/launch.rs:133`). |
| 7 | Holds | ADR 0001 is the process model; ADR 0002 is storage, so the ADR cites 0002. `afk_check` tests `database_path(...).is_file()`; failure-modes now says the team's `team.db` exists. |
| Note | Fixed | The skill said "start step 2"; it now says "start section 2", so a lead cannot read it as the start command. |

D39: cli.md, failure-modes (two new AFK-10 rows citing `an_afk_team_that_never_had_a_member_closes_without_a_report` and `an_afk_team_whose_only_member_was_removed_still_needs_a_report`, names verified on afkmode/closestat), ADR point 7, AFK-10 requirement text, and the skill's last section (the lead runs `teamwire close <team>` and says why in its last reply).

Check on the last commit: cargo test (harness-claude, cli) exit 0, 440 passed, 0 failed; `docs check: ok`; `gate.sh fast` fmt ok, tidy ok (same two expected lines), clippy ok.

### Decisions I made (this round)

- Edited ADR 0025 in place: it is new on this branch and unmerged, so no supersede is needed.
- AFK-10 requirement text gets one added sentence for D39, so it is no longer word for word from the plan's table; the lead asked for it.
- 5778a70 closer findings 1-4: second-word command test, refusal order, Claude-Code-only wording, README | left: nothing

## Closer round (closer.md findings 1 to 4)

| # | Verdict | Evidence and fix |
| --- | --- | --- |
| 1 | Holds | `shown_commands` keeps one word. `the_afk_skill_names_only_commands_and_tools_that_exist` now also asks `teamwire <word> --help` for subcommands (new helper `subcommands_of`) and checks the word after it when the command has any. Seen red: with `teamwire afk finish --team ...` in the skill the test fails with "`teamwire afk finish` is not a command"; restored, 11 preset tests pass. The main skill's test keeps the old limit (not in this finding's scope). |
| 2 | Holds | start `afk.rs`: `lead_plan` (the source of `unknown_host`) runs after `task_empty`. cli.md's order table now has `unknown_harness` first and `unknown_host` after the task-file row. |
| 3 | Holds | No code refuses a non-Claude lead (D38). cli.md now says "supported only for a Claude Code lead; no check refuses another harness (D38)". ADR point 10 says the same and that a Codex lead can start a run nothing supports. |
| 4 | Holds | README bullet now ends "unless the run never added a member". |

Check on the last commit: cargo test (harness-claude, cli) exit 0, 440 passed, 0 failed; `docs check: ok`; `gate.sh fast` fmt, tidy, clippy ok (same two expected tidy lines).

### Decisions I made (this round)

- The second-word check is generic (any command with subcommands), not only `afk`, so a future skill line is covered; only `afk` has subcommands the skill uses today.
- Findings 5 and 6 are not mine (the lead and stophold); I changed nothing for them.
