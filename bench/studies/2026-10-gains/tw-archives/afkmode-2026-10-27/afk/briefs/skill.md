# Brief: skill, the AFK skill, its install, and the docs

## 1. Who you are

You are `skill`, an implementer in team `afkmode`. Nobody will answer a question from the user
during this run; the lead answers questions, and every open detail below is already decided.

The other members of wave 1, each on its own branch off `afk/base`:

| Role | Kind | Owns (summary) | Who uses its work |
| --- | --- | --- | --- |
| start | implementer | `teamwire afk start`, doctor's launch checks (`crates/teamwire-cli/src/cmd/afk.rs`, `doctor.rs`, launch's `afk.rs`) | the lead, your skill text |
| budget | implementer | launch's budget refusal and model warnings | launch |
| stophold | implementer | the AFK hold in the lead's Stop hook | the lead |
| closestat | implementer | close's report check, status, teams, `ui/src/shared/` | close, status, the dashboard |
| skill (you) | implementer | the AFK skill, its rendering into the plugin, and every doc of this work | the lead, the user |

You document behaviour the other four build on their own branches. You never see their code; you
write from the design and from decisions.md, which fix every name, code, message and output line.
After you: a reviewer reads your branch, then a closer runs every check on the joined branch and
compares the docs with the joined code, then a reporter writes the run's report.

## 2. The end state

1. **The skill.** `examples/skills/afk/SKILL.md` holds the agreed content of design section 6. You
   may reword it for length and the plain-language style, and you must keep every rule. Adjust it
   to the decisions: refusals print `refused: <code>: <message>` and the lead tells the user and
   stops (D9); `--budget` takes `30m`, `8h`, `1d` and the like, default 8h (D10); the start prints
   the five lines of D12, and the lead's one reply quotes the team, report path and budget end;
   section 7's report headings are `##` headings with exactly the eight names of D17 (What you
   asked; What I understood; The result; Verified; Decisions made for you; Not done, and why; Risks;
   Cost), so close accepts the report; `teamwire status <team>` shows the budget line (D18).
   Show the decision-entry example of section 6 at column 0 inside a fenced block, so a lead copies
   `D3  2026-10-19 14:05  ...` with no indent (D35). The budget-spent hold note is D31's text.
   Frontmatter: `name: afk`, the description of section 6, and no `metadata` key (D21).
2. **The pointer.** `examples/skills/teamwire/SKILL.md` gets, near the top, the design's line and
   nothing more (D36): "For a run the user will not be present for, use `/teamwire:afk`."
3. **The install.** `AFK_SKILL_TEXT` in `crates/teamwire-harness-claude/src/lead_setup/skill.rs`
   (an `include_str!` of the skill), rendered by `PluginFiles::render_with` in `plugin.rs` as
   `teamwire/skills/afk/SKILL.md`, stamped with `skill::stamp` like the main skill. Claude Code then
   lists it as `/teamwire:afk`. The plugin's version hashes every rendered file, so it changes with
   the skill. doctor's plugin check and `teamwire install` compare and write every rendered file
   already (`lead_setup.rs`, `plugin_problems`), so `doctor.rs` and `install.rs` need no change,
   and start owns `doctor.rs`. Two tests in `crates/teamwire-harness-claude/tests/lead_setup.rs`
   prove the install and doctor's check (D32). The Codex plugin is out of scope.
4. **The docs**, each from the design and the decisions, never from guesses about code:
   - `docs/adr/0025-afk-runs.md` (status accepted, 2026-10-27): design sections 3 to 5 as decisions
     and the options they beat, including the rejected separate AFK home, the rejected `afk
     finish` and `afk status` commands, and the rejected several-team run. The schema comment in
     `crates/teamwire-store/src/schema/v5.sql` already cites ADR 0025.
   - `docs/requirements.md`: a new section `### AFK runs (AFK)` with rows AFK-1 to AFK-12, word for
     word from the plan's "Tests and requirement ids" table (Id, Level, Requirement, and Source =
     "AFK design 2026-10-19 section <n>; ADR 0025").
   - `docs/failure-modes.md`: a new section `## <next number>. AFK runs`, one or more rows per
     AFK id, each citing the test names in the plan's table exactly.
   - `docs/api/cli.md`: a `### teamwire afk start --team NAME --title TITLE --task FILE [--budget
     DURATION]` section under the lead commands (D9 to D13: order, codes, output, exit codes 0, 1
     and 2); in launch, the `afk_budget_spent` refusal (D14) and the model-table warning (D15); in
     close, `afk_report_incomplete` (D17); in status, the AFK line and the `afk` object (D18); in
     teams, the `(open, afk)` marker and the `afk_budget_ends_at` key (D19); the
     `hook claude-lead-stop` row, the AFK hold and its two notes (D16, D31); the `install` row, the
     second skill `skills/afk/SKILL.md`.
   - `docs/api/core.md`: `teamwire_report::afk_hold_notes(home: &Path, session: &SessionId,
     platform: &dyn Platform, clock: &dyn Clock) -> Vec<String>`; `StatusView.afk:
     Option<AfkView>` and `AfkView { budget_ends_at: Timestamp, seconds_left: u64, spent: bool,
     decisions: u32 }`; `TeamEntry.afk_budget_ends_at: Option<Timestamp>`; one sentence that launch's
     AFK start steps live in `teamwire_launch::afk`, with no signature (start picks it).
   - `docs/api/mcp.md`: `team_status` carries the `afk` object; the `team_teams` example gains
     `"afk_budget_ends_at": null`.
   - `docs/ARCHITECTURE.md`: the plugin's skill files (line 39) and an `afk/` row in the table of a
     team's directory (written by `teamwire afk start`, the lead and its members; archived by close).
   - `README.md`: one feature bullet, `teamwire afk start` in the commands list, and how a run starts
     (`/teamwire:afk <task>` in a session started with `claude --permission-mode
     bypassPermissions`).
   - `CONTEXT.md`: a glossary entry **AFK run**: one team, led by one Claude Code session, that runs
     one task with nobody present until its report is written.

## 3. Done when

- Every test named in section 10 exists with that exact name and passes. Each test of new
  behaviour fails without its change; a test marked (guard) pins today's behaviour and passes on
  afk/base too (D33). None of your named tests is a guard.
- `scripts/check-docs.sh` prints `docs check: ok`.
- Every rule of design section 6 is in the skill; your report has a table: each rule of section 6,
  and the skill line that keeps it.
- The check in section 12 passes on your last commit, with its real output in your report.

## 4. Out of scope

- `doctor.rs`, `install.rs`, the Codex plugin, `examples/skills/codex-lead.md`.
- Any code outside `crates/teamwire-harness-claude/src/lead_setup*` and its tests.
- `docs/design/`, existing ADRs (an ADR changes only through a new one), `docs/build-plan.md`.
- `cargo install`, `teamwire install`, a live run of the skill.

## 5. When to do nothing

If `teamwire --help` built from your branch lists no `afk` command (the lead's afk-arguments commit
is missing), do units 1, 2 and 4, leave out the command test of unit 3, say so in your report, and
finish with status `blocked`.

## 6. Where you are

Your worktree is `/private/tmp/claude-501/bnruns/bn4tu2e1us/clone/.worktrees/` plus your slug, on
branch `afkmode/skill`, cut from `afk/base` (5b86c1e plus the lead's afk-arguments commit). Setup
already ran `cargo build --workspace --all-targets --locked`, so run no install command.

## 7. Read first, in this order

1. `/private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/afk/plan.md`, all of it.
2. `/private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/afk/decisions.md`, D1 to D36.
   An entry with an `Amended by` line is changed by the later entry it names; the later entry wins.
3. `docs/design/2026-10-19-afk-mode.md`, all of it. Section 6 is your skill's content.
4. `AGENTS.md`, then `docs/conventions.md` section 6.
5. `examples/skills/teamwire/SKILL.md` (its top, and the brief and planning-check sections) and
   `reference.md`; `crates/teamwire-harness-claude/src/lead_setup/skill.rs` and `plugin.rs`;
   `crates/teamwire-harness-claude/tests/lead_setup.rs` (line 184 counts the rendered files;
   `plugin_problems_name_what_is_missing_stale_or_off` near line 530 shows how to test
   `plugin_problems`);
   `crates/teamwire-cli/tests/preset.rs` lines 266-400 (`shown_commands`,
   `the_skill_names_only_commands_and_tools_that_exist`).
6. The docs you change, and `docs/adr/0024-*.md` for the ADR format.

## 8. Decisions already made (2026-10-27; full text in decisions.md)

- D7: `teamwire afk start` exists on afk/base as a stub with the agreed flags, so `teamwire --help`
  lists `afk` and your command test can run on your branch.
- D8: doctor and install need no change for a second skill; start owns `doctor.rs`.
- D9 to D20: every name, code, message, output line and JSON key you document. Quote them; do not
  paraphrase a message the user will see.
- D21, D36: the skill's path, stamp, frontmatter, report headings and the pointer line.
- D31: the hold note past the budget. D35: decisions count with leading white space, and your
  skill shows the entry at column 0.
- D32: the install and doctor tests live in `tests/lead_setup.rs`.
- D22: you own every doc this work changes; the test names you cite are the plan's.
- D23: `crates/teamwire-cli/tests/tmux_wiring.rs::herdr_env_wins_over_tmux` fails on afk/base inside
  a tmux pane, before any change. Your check skips it by name. Do not edit or skip it any other way.

## 9. What already exists

- `SKILL_TEXT`, `REFERENCE_TEXT`, `skill::stamp`, `skill::classify`, `PluginFiles::render_with` and
  its exact-paths test, `the_shipped_skill_can_be_stamped_and_has_no_metadata_of_its_own`.
- `shown_commands` and `subcommands` in `crates/teamwire-cli/tests/preset.rs`; reuse them.
- The `team.created` and `afk_budget_ends_at` rows already in `docs/api/core.md` and
  `docs/ARCHITECTURE.md` (5b86c1e, fb1fd8f); extend them, do not repeat them.

## 10. Units in order, commit after each

Use the tdd skill for each unit; test only at these seams: the install and the plugin check in
`crates/teamwire-harness-claude/tests/lead_setup.rs` (the bed's `install`, `plan` and the public
`ClaudeConfig::plugin_problems`), and the skill texts read from `examples/` in
`crates/teamwire-cli/tests/preset.rs`. If a check fails and the cause is not clear from its output,
use the diagnosing-bugs skill before your second fix attempt.

1. The skill text and the pointer line.
2. The install: `AFK_SKILL_TEXT`, the render, and the exact-paths test in `plugin.rs`. Tests in
   `crates/teamwire-harness-claude/tests/lead_setup.rs` (D32):
   - `install_writes_the_afk_skill_as_a_second_stamped_skill` (after the bed installs, the file
     `teamwire/skills/afk/SKILL.md` exists in the plugin directory and is stamped; the count of
     rendered files at line 184 becomes 7)
   - `a_plugin_without_the_afk_skill_or_with_an_edited_one_is_a_problem` (install, then delete the
     installed `skills/afk/SKILL.md`: `config.plugin_problems(&want(), true)` is not empty;
     install again, then change one line of that file: not empty again)
   A unit test in `plugin.rs`'s tests module, such as a changed AFK text changing the plugin
   version, may stay as an extra; the docs cite the two tests above.
3. Tests in `crates/teamwire-cli/tests/preset.rs`:
   - `the_afk_skill_names_only_commands_and_tools_that_exist` (every `teamwire <word>` it shows is a
     subcommand of the binary, every `team_*` tool is in `docs/api/mcp.md`, and the front matter
     starts `---\nname: afk\ndescription: `)
   - `the_afk_skill_lists_the_eight_report_headings` (each of D17's names as a `##` heading, in order)
   - `the_main_skill_points_at_the_afk_skill`
4. The docs of section 2 item 4, then `scripts/check-docs.sh`.

## 11. Files you own, and files owned by others

You own: `examples/skills/afk/SKILL.md`, `examples/skills/teamwire/SKILL.md` (the pointer line only),
`crates/teamwire-harness-claude/src/lead_setup/skill.rs`,
`crates/teamwire-harness-claude/src/lead_setup/plugin.rs`,
`crates/teamwire-harness-claude/src/lead_setup.rs`, `crates/teamwire-harness-claude/src/lib.rs`,
`crates/teamwire-harness-claude/tests/lead_setup.rs`, `crates/teamwire-cli/tests/preset.rs`,
`docs/adr/0025-afk-runs.md` (docs/adr/ is protected; this ticket owns this one new file, and tidy
prints `PROTECTED-CHANGED` for it, which is expected), `docs/requirements.md`,
`docs/failure-modes.md`, `docs/api/cli.md`, `docs/api/core.md`, `docs/api/mcp.md`,
`docs/ARCHITECTURE.md`, `README.md`, `CONTEXT.md`.

Owned by others, never edit: every other file under `crates/teamwire-cli/` (start, stophold,
closestat), including `doctor.rs` and `install.rs`; `crates/teamwire-launch/`,
`crates/teamwire-report/`, `crates/teamwire-close/`, `crates/teamwire-watch/`,
`crates/teamwire-msg/`, `ui/` (other members); `examples/skills/teamwire/reference.md`,
`examples/presets/`, `docs/design/`, existing ADRs, `crates/teamwire-store/`, `tests/fakes/`,
`Cargo.toml`, `Cargo.lock`, `scripts/`, `xtask/`, `AGENTS.md` (protected or the lead's). For a
change in any of them, send the lead a question and carry on.

## 12. Verification

Your check, from the worktree root:

```bash
cargo test -p teamwire-harness-claude -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux && scripts/check-docs.sh && scripts/gate.sh fast
```

On afk/base it passes (434 tests, `docs check: ok`, then fmt, tidy and clippy ok). Do not run the
full suite, `scripts/test-changed.sh` or `scripts/gate.sh` without `fast`.

## 13. How to finish

Do not push and do not open a pull request. Commit each unit. Write your report with one progress
line per commit, the check's real output, the section-6 rule table, every new `pub` item, and a
**Decisions I made** list. Then run `teamwire done` (or `team_done`).

## 14. Open questions you must not answer

None. Decide anything the brief does not cover, and list it under **Decisions I made**.
