# Brief: endings (implementer) for the AFK run "afkmode"

## 1. Who you are
You are `endings`, an implementer on model sonnet at effort high. You build close and status for AFK teams (design 5.5). Roster of implementers in this wave, each in its own worktree:
- `start`: `teamwire afk start`, launch's budget and model checks, the store's `create_afk_team` signature (teamwire-store, teamwire-launch, cli afk.rs, args.rs, main.rs, doctor.rs).
- `stophold`: the AFK hold in the lead's Stop hook (teamwire-report, cli hook.rs).
- `endings` (you): teamwire-close, teamwire-watch, teamwire-msg, cli status.rs and teams.rs, the ui schemas; files in section 11.
- `skill`: the AFK skill, install rendering it, and every doc file.
The planner, `planner2`, stays open: ask it with `teamwire send --to planner2 --type question` when this brief and the design do not answer something.

## 2. The end state
On branch `afkmode/endings`:
- `teamwire close <afk team>` refuses with `afk_report_incomplete` while `<team dir>/afk/REPORT.md` is missing or lacks one of the eight headings. `--role` removal and non-AFK teams are unaffected.
- `StatusView` has `afk: Option<AfkView>`; `AfkView` has `budget_ends_at: Timestamp`, `seconds_left: u64`, `spent: bool`, `decisions: u32` (task.md, binding). `status --json` writes those names; `teamwire status <team>` prints one AFK line.
- `teamwire teams` marks an AFK team; `teams --json` gives each team `afk_budget_ends_at` (timestamp string or `null`). The MCP teams tool returns the same key, since it serialises the same `TeamEntry`.
- ui/src/shared/teamwire.ts accepts the `afk` object and the new key, and still accepts older output without them.

## 3. Done when
- `cargo test -p teamwire-close -p teamwire-watch -p teamwire-msg -p teamwire-mcp -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux` passes, and in ui/: `bun test` and `bun run typecheck` pass.
- `scripts/gate.sh fast` passes.
- New tests: crates/teamwire-close/tests/afk.rs (no report; a report missing one heading, named in the refusal; a full report closes; headings written as `## 3. The result` and as `3. **The result**` both count; `--role` on an AFK team without a report works; a non-AFK team without a report closes); crates/teamwire-watch/tests/afk.rs (`afk` is None for a normal team; seconds_left and spent before and after the budget with a fake clock; decisions counts only lines matching `^D[0-9]+\s`; a missing decisions.md counts 0; an AFK team with zero members renders); CLI tests in crates/teamwire-cli/tests/status.rs and teams.rs for the text line, the mark and both JSON keys; a ui test that parses a status and a teams fixture with and without the new fields.
- Every change is committed on `afkmode/endings`, and the ui schema change is in the same commit as the JSON change it copies (AGENTS.md).

## 4. Out of scope
`afk start`, launch, the Stop hook, the skill, docs. Dashboard views beyond the schema copy. An `afk status` command. A change to the archive (the design says it needs none).

## 5. When to do nothing
If `git log --oneline -1 main` is not fb1fd8f, or `grep -n afk_budget_ends_at crates/teamwire-store/src/team.rs` finds nothing, stop, say so in your report, and run `teamwire done --status blocked`.

## 6. Where you are
Your worktree is on branch `afkmode/endings`, based on `main` at fb1fd8f. Setup already ran `bun install --frozen-lockfile` in ui/. Cargo builds in your own target/.

## 7. Read first, in this order
1. AGENTS.md (hard rules), ui/AGENTS.md (ui commands and rules).
2. docs/design/2026-10-19-afk-mode.md sections 4 (rows on close and status), 5.1, 5.5, and section 6 part "7. Report and close" (the eight headings), and the zero-member risk at the end of section 7.
3. /private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/afk/task.md (binding names).
4. /private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/afk/decisions.md entries D5, D7, D12, D16, D17, D21.
5. The code in section 9.

## 8. Decisions already made (2026-10-23, full text in afk/decisions.md)
- D5: a REPORT.md line is a heading when it starts with 1 to 6 `#` and a space, or is a numbered item `1.` to `8.` and a space; optional `**` around the text. It matches when its text, trimmed of `*` and spaces and compared case-insensitively, starts with: "What you asked", "What I understood", "The result", "Verified", "Decisions made for you", "Not done", "Risks", "Cost". Order is not checked. The refusal is one string in `ClosePlan.refusals` (printed as `REFUSED: <text>` by crates/teamwire-cli/src/cmd/close.rs:113): `afk_report_incomplete: <team dir>/afk/REPORT.md lacks: <heading>, <heading>` or `afk_report_incomplete: <team dir>/afk/REPORT.md is missing`. The check runs in `team_plan` for an open team (crates/teamwire-close/src/plan.rs:60), never in `member_plan`.
- D7: `status <team>` prints, after the team line, `afk: budget ends <budget_ends_at>, <H>h<MM>m left, <n> decisions` or, once spent, `afk: budget spent at <budget_ends_at>, <n> decisions`. `teams` prints `<name> (<state>, afk) <title>: ...` for an AFK team, where today it prints `<name> (<state>) <title>: ...` (crates/teamwire-cli/src/cmd/teams.rs:44). JSON: `"afk": {"budget_ends_at", "seconds_left", "spent", "decisions"}` or `"afk": null`; `"afk_budget_ends_at": "<ts>"` or `null` on each `teams --json` entry.
- D16: you own ui/src/shared/teamwire.ts although ui/AGENTS.md reserves src/shared/ for the lead: the lead delegates it to you. Make every new field optional and nullable (`.nullish()` or `.optional()`), so older output parses.
- D17: `decisions` counts lines of `<team dir>/afk/decisions.md` matching `^D[0-9]+\s`; missing file = 0. `spent` = now >= budget_ends_at. `seconds_left` = whole seconds left, 0 once spent. Time from the `Clock` in `Ctx`.
- D21: in tests, create an AFK team in exactly one helper per test crate, `afk_team(...)`, calling `tx.create_afk_team(at, &team, ends)` with today's three arguments. After `start` merges its four-argument form, the lead will ask you, or change that line itself.
- D12: skip `herdr_env_wins_over_tmux` in your check.
- You write no docs; `skill` owns them. After your last commit, send `teamwire send --to skill --type notice --commit <sha> --text "..."` with the exact status line, the teams mark and both JSON shapes.

## 9. What already exists (do not rebuild)
- Close planning: `plan_close_with` crates/teamwire-close/src/plan.rs:19, `team_plan` plan.rs:60, refusals as strings; the empty-store shortcut `nothing_recorded` crates/teamwire-close/src/empty.rs:23 applies only to a store with no team row, so an AFK team never takes it. Close tests to copy: crates/teamwire-close/tests/close.rs, tests/support/.
- Status: `StatusView` crates/teamwire-watch/src/lib.rs:148; `status_with` crates/teamwire-watch/src/status.rs:294 (constructs it at :299 too); the CLI's fallback constructor crates/teamwire-cli/src/cmd/status.rs:300; text render status.rs:94. Watch tests to copy: crates/teamwire-watch/tests/status.rs.
- Teams: `TeamEntry` crates/teamwire-msg/src/directory.rs:15, built at :44 and serialised as is by cmd/teams.rs:37 and by the MCP server (crates/teamwire-mcp/src/reads.rs).
- `TeamRow.afk_budget_ends_at`: crates/teamwire-store/src/team.rs:49.
- ui: `StatusView` schema ui/src/shared/teamwire.ts:122, `TeamEntry` :154; fixtures ui/fixtures/status.json and ui/fixtures/teams.json; test ui/src/shared/teamwire.test.ts.

## 10. Units in order (commit after each, each with its test)
Use the tdd skill for each unit; test only at these seams: close's `plan_close_with` and the `teamwire close` binary against a team directory with and without REPORT.md; `teamwire_watch::status_with` with a fake clock; `teamwire status` and `teamwire teams` through the CLI binary with `teamwire_fakes::TestHome`; the zod schemas through `bun test`. If a check fails and the cause is not clear from its output, use the diagnosing-bugs skill before your second fix attempt.
1. Close: crates/teamwire-close/src/afk.rs (the heading check) and its call in `team_plan`. Tests in crates/teamwire-close/tests/afk.rs.
2. Watch: crates/teamwire-watch/src/afk.rs (`AfkView`, the count), `StatusView.afk`, both constructors. Tests in crates/teamwire-watch/tests/afk.rs.
3. Status CLI and ui in one commit: the AFK line in cmd/status.rs, the `afk` object in ui/src/shared/teamwire.ts, a fixture with it, the tests.
4. Teams in one commit: `TeamEntry.afk_budget_ends_at`, the mark in cmd/teams.rs, the ui `TeamEntry` schema and fixture, the CLI and ui tests; run teamwire-mcp's tests too.
5. Send the notice to `skill`.

## 11. Files you own, and files owned by others
You own: crates/teamwire-close/src/afk.rs (new), lib.rs, plan.rs, crates/teamwire-close/tests/afk.rs (new); crates/teamwire-watch/src/afk.rs (new), lib.rs, status.rs, crates/teamwire-watch/tests/afk.rs (new), tests/status.rs, tests/support/fixture.rs; crates/teamwire-msg/src/directory.rs; crates/teamwire-mcp/tests/teams.rs; crates/teamwire-cli/src/cmd/status.rs, cmd/teams.rs, crates/teamwire-cli/tests/status.rs, tests/teams.rs; ui/src/shared/teamwire.ts, ui/src/shared/teamwire.test.ts, ui/fixtures/status.json, ui/fixtures/teams.json.
Owned by others, do not edit: crates/teamwire-store/, crates/teamwire-launch/, cmd/afk.rs, args.rs, main.rs, doctor.rs (start); crates/teamwire-report/, cmd/hook.rs (stophold); examples/, crates/teamwire-harness-claude/, docs/, README.md (skill). Protected, nobody edits: root Cargo.toml, Cargo.lock, clippy.toml, rustfmt.toml, rust-toolchain.toml, scripts/, xtask/, tests/fakes/, docs/adr/.
Hard rules that matter here: status and teams read only; close's refusal is decided before any step runs; time from the `Clock` trait; only teamwire-cli prints or exits; no unwrap, expect, panic or todo outside tests; `#[expect(lint, reason = "...")]` only; no new dependency, no Cargo.lock or ui lockfile change; split a file past 400 lines, tidy fails past 800 (watch/src/status.rs is about 330 lines: put new code in afk.rs); ui zod schemas change in the same commit as the CLI JSON they copy.

## 12. Verification
Run `cargo test -p teamwire-close -p teamwire-watch -p teamwire-msg -p teamwire-mcp -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux`, then in ui/ `bun test` and `bun run typecheck`, then from the root `scripts/gate.sh fast`, then `scripts/test-changed.sh`. Do not run the full gate; the closer does.

## 13. How to finish
No push, no pull request. Write /private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/reports/endings.md: one progress line per commit (sha and what it did), the commands you ran with their exit codes, and a **Decisions I made** list at the end. Then run `teamwire done`.

## 14. Open questions you must not answer
None.
