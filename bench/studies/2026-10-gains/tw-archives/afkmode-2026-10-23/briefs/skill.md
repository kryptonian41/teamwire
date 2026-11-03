# Brief: skill (implementer) for the AFK run "afkmode"

## 1. Who you are
You are `skill`, an implementer on model sonnet at effort high. You write the AFK skill, make install render it into the Claude Code plugin, and write every doc change of this run. Roster of implementers in this wave, each in its own worktree:
- `start`: `teamwire afk start`, launch's budget and model checks, the store's `create_afk_team` signature.
- `stophold`: the AFK hold in the lead's Stop hook.
- `endings`: close's report check, status and teams output, the ui zod schemas.
- `skill` (you): examples/, teamwire-harness-claude's lead setup, docs/, README.md; files in section 11.
The others send you a `notice` with their final names after their last commit. Every name is already fixed in section 8, so write the docs from section 8 first and correct them from the notices. The planner, `planner2`, stays open: ask it with `teamwire send --to planner2 --type question`.

## 2. The end state
On branch `afkmode/skill`: examples/skills/afk/SKILL.md holds the agreed skill of design section 6, keeping every rule; `teamwire install` renders it into the plugin as `teamwire/skills/afk/SKILL.md`, so Claude Code lists `/teamwire:afk`; doctor's plugin check covers it; the main skill has the pointer line; ADR 0025 and `AFK-` requirement ids exist; the docs describe the AFK command, hold, budget, close and status. Tested.

## 3. Done when
- `cargo test -p teamwire-harness-claude -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux` passes, and `scripts/check-docs.sh` prints `docs check: ok`.
- `scripts/gate.sh fast` passes.
- Tests: the rendered plugin files include `teamwire/skills/afk/SKILL.md` with the compiled-in text (update the file-list test at crates/teamwire-harness-claude/src/lead_setup/plugin.rs:278); changing the AFK text changes the plugin version; a test in crates/teamwire-harness-claude/tests/lead_setup.rs shows an installed plugin whose afk SKILL.md differs is reported as not current; the AFK skill's front matter starts `---\nname: afk\ndescription: `.
- Every rule of design section 6 is in examples/skills/afk/SKILL.md (check it against the section line by line and list any rewording in your report).
- Every change is committed on `afkmode/skill`.

## 4. Out of scope
Rust code outside crates/teamwire-harness-claude. The Codex plugin and a Codex AFK skill. Running `teamwire install` or `cargo install` for real. The live check of design section 7. Changing any existing ADR (add 0025 only).

## 5. When to do nothing
If `git log --oneline -1 main` is not fb1fd8f, stop, say so in your report, and run `teamwire done --status blocked`.

## 6. Where you are
Your worktree is on branch `afkmode/skill`, based on `main` at fb1fd8f. Setup already ran `bun install --frozen-lockfile` in ui/ (you do not change ui/). Cargo builds in your own target/.

## 7. Read first, in this order
1. AGENTS.md (hard rules, the Comments section), docs/conventions.md.
2. docs/design/2026-10-19-afk-mode.md, all of it: sections 3 to 5 become the ADR, section 6 is the skill text.
3. examples/skills/teamwire/SKILL.md (top 40 lines) and reference.md: the main skill's style and where the pointer line goes.
4. /private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/afk/task.md and decisions.md (all entries D3 to D23: they fix every name the docs need).
5. The code in section 9.

## 8. Decisions already made (2026-10-23, full text in afk/decisions.md)
- D23: add `AFK_SKILL_TEXT` (include_str of examples/skills/afk/SKILL.md) in lead_setup/skill.rs, and the entry `teamwire/skills/afk/SKILL.md`, stamped with `skill::stamp`, to the body list in plugin.rs `render_with`. Doctor needs no code change; prove its check covers the file with a test.
- D10: you own every doc file of this run. D15: you may add docs/adr/0025-afk-runs.md, and no other file under docs/adr/. The ADR records the decisions of design sections 3 to 5 and the options they beat, including the rejected separate `afk/` home and the rejected `afk finish` and `afk status` commands.
- Names the docs must use (task.md and decisions):
  - Command: `teamwire afk start --team NAME --title TITLE --task FILE [--budget 8h]`. Refusals: `no_lead_session`, `background_lead`, `afk_already_running`, `team_exists`, launch's name and title refusals, `doctor_failed: <check>`, `no_lead_process`, `bad_budget`. Output lines (D20): `team <name> created for an AFK run`, `afk dir: <team dir>/afk`, `report: <team dir>/afk/REPORT.md`, `budget ends: <ts>`, `keep the lid open and the machine on power: a closed lid still sleeps a Mac`.
  - Store: `Tx::create_afk_team(at, team, budget_ends_at, extra)`; `team.created` carries `lead_cwd` and `afk_budget_ends_at` (D3).
  - Launch: `refused: afk_budget_spent` per implementer past the budget, others start, exit PARTIAL (D18); one warning per member below the AFK table (D8).
  - Stop hook: `teamwire_report::afk_hold_notes(home, session, platform, clock)`; the two note texts in D6; joined after the watch notes (D22).
  - Close: `afk_report_incomplete` with the missing headings (D5).
  - Status and teams: `afk: budget ends <ts>, <H>h<MM>m left, <n> decisions` / `afk: budget spent at <ts>, <n> decisions`; `<name> (<state>, afk) <title>: ...`; JSON `afk` object with `budget_ends_at`, `seconds_left`, `spent`, `decisions`, or null; `afk_budget_ends_at` on each `teams --json` entry and the MCP teams tool (D7, D17).
- The main skill's pointer line, near the top, word for word: "For a run the user will not be present for, use `/teamwire:afk`."
- The model table in the skill stays as design section 6 gives it (opus). This run's own implementers are sonnet because the user's task said so; that does not change the skill.
- Requirement ids use a new `AFK-` prefix (AFK-1, AFK-2, ...) in docs/requirements.md, one per checkable rule of design 5.2 to 5.6, each naming the test file that checks it: crates/teamwire-cli/tests/afk.rs, crates/teamwire-launch/tests/afk.rs, crates/teamwire-report/tests/afk_hold.rs, crates/teamwire-cli/tests/lead_hook.rs, crates/teamwire-close/tests/afk.rs, crates/teamwire-watch/tests/afk.rs, crates/teamwire-harness-claude/tests/lead_setup.rs.
- D12: skip `herdr_env_wins_over_tmux` in your check.

## 9. What already exists (do not rebuild)
- `SKILL_TEXT` and `REFERENCE_TEXT`: crates/teamwire-harness-claude/src/lead_setup/skill.rs:12 and :17; `stamp` in the same file.
- The plugin render: crates/teamwire-harness-claude/src/lead_setup/plugin.rs:57 (`render_with`), the file list at :79, the version hash after it, `plugin_matches` :135, the file-list test :278 and the reference test :316 (copy it for the AFK file).
- Lead setup tests: crates/teamwire-harness-claude/tests/lead_setup.rs (:200 reads the rendered skill).
- crates/teamwire-cli/tests/preset.rs:331 checks that the main skill names only existing commands. Leave it on the main skill only: `teamwire afk` exists only after `start` merges.
- Docs listings: docs/api/cli.md (commands), docs/api/core.md (store API, the `afk_budget_ends_at` column line is already there), docs/api/mcp.md (teams tool output), docs/ARCHITECTURE.md (components), docs/failure-modes.md (the risks of design section 7: hold misses a lead's own background command, zero-member team, older binary read-only, usage limit), README.md, examples/README.md (list the new skill).

## 10. Units in order (commit after each, each with its test)
Use the tdd skill for each unit; test only at these seams: the rendered plugin files from `render` and the lead setup state through crates/teamwire-harness-claude's public API; doctor's plugin check through crates/teamwire-harness-claude/tests/lead_setup.rs. If a check fails and the cause is not clear from its output, use the diagnosing-bugs skill before your second fix attempt.
1. examples/skills/afk/SKILL.md from design section 6, and the pointer line in examples/skills/teamwire/SKILL.md. Test: the front matter test (section 3).
2. Render it into the plugin (D23). Tests: file list, version change, out-of-date detection.
3. docs/adr/0025-afk-runs.md and the `AFK-` rows in docs/requirements.md.
4. docs/api/cli.md, docs/api/core.md, docs/api/mcp.md, docs/ARCHITECTURE.md, docs/failure-modes.md, docs/conventions.md (only if it lists skills or commands), README.md, examples/README.md. Then correct them from the other members' notices.

## 11. Files you own, and files owned by others
You own: examples/skills/afk/ (new), examples/skills/teamwire/SKILL.md, examples/README.md, crates/teamwire-harness-claude/src/lead_setup/skill.rs, lead_setup/plugin.rs, crates/teamwire-harness-claude/src/lead_setup.rs, crates/teamwire-harness-claude/tests/lead_setup.rs, docs/adr/0025-afk-runs.md (new), docs/requirements.md, docs/failure-modes.md, docs/api/cli.md, docs/api/core.md, docs/api/mcp.md, docs/ARCHITECTURE.md, docs/conventions.md, README.md.
Owned by others, do not edit: crates/teamwire-store/, crates/teamwire-launch/, cmd/afk.rs, args.rs, main.rs, doctor.rs (start); crates/teamwire-report/, cmd/hook.rs (stophold); crates/teamwire-close/, crates/teamwire-watch/, crates/teamwire-msg/, cmd/status.rs, cmd/teams.rs, ui/ (endings). Protected, nobody edits: root Cargo.toml, Cargo.lock, clippy.toml, rustfmt.toml, rust-toolchain.toml, scripts/, xtask/, tests/fakes/, and every existing file in docs/adr/.
Hard rules that matter here: no unwrap, expect, panic or todo outside tests; `#[expect(lint, reason = "...")]` only; no new dependency, no Cargo.lock change; split a source file past 400 lines, tidy fails past 800 (plugin.rs is 403 lines: if the change pushes it much further, move the tests or a helper into a new file); no conversation or ticket history in code or docs; change an ADR only by adding one.

## 12. Verification
Run `cargo test -p teamwire-harness-claude -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux`, then `scripts/check-docs.sh`, then `scripts/gate.sh fast`, then `scripts/test-changed.sh`. Do not run the full gate; the closer does.

## 13. How to finish
No push, no pull request. Write /private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/reports/skill.md: one progress line per commit (sha and what it did), the commands you ran with their exit codes, each rewording of section 6 you made, and a **Decisions I made** list at the end. Then run `teamwire done`.

## 14. Open questions you must not answer
None.
