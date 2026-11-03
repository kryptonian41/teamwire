# Brief: start (implementer) for the AFK run "afkmode"

## 1. Who you are
You are `start`, an implementer on model sonnet at effort high. You build `teamwire afk start`, launch's AFK budget refusal and model-table warning, and the store's `create_afk_team` fourth parameter. Roster of implementers in this wave, each in its own worktree:
- `start` (you): the store, launch and the CLI command, files in section 11.
- `stophold`: the AFK hold in the lead's Stop hook (teamwire-report, cli hook.rs).
- `endings`: close's report check, status and teams output, the ui zod schemas (teamwire-close, teamwire-watch, teamwire-msg, cli status.rs and teams.rs, ui/).
- `skill`: the AFK skill, install rendering it, and every doc file (examples/, teamwire-harness-claude, docs/, README.md).
The planner, `planner2`, stays open: ask it with `teamwire send --to planner2 --type question` when this brief and the design do not answer something.

## 2. The end state
On branch `afkmode/start`: `teamwire afk start --team NAME --title TITLE --task FILE [--budget 8h]` works as design section 5.2 says, with the refusal codes of task.md; launch refuses each new implementer past an AFK team's budget and warns once per member below the AFK model table (design 5.4); `Tx::create_afk_team(at, team, budget_ends_at, extra)` merges `extra` into `team.created`. All tested.

## 3. Done when
- `cargo test -p teamwire-store -p teamwire-launch -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux` passes.
- `scripts/gate.sh fast` passes (fmt, tidy, clippy).
- New tests exist in crates/teamwire-store/tests/afk.rs, crates/teamwire-launch/tests/afk.rs and crates/teamwire-cli/tests/afk.rs, and each refusal code below has a test: `no_lead_session`, `background_lead`, `afk_already_running`, a bad team name, a bad title, `team_exists` (name taken), `doctor_failed`, `no_lead_process`, `bad_budget`, and launch's `afk_budget_spent`.
- A test proves that a refused `afk start` creates no team directory.
- A test proves `launch --add` into an AFK team with zero members starts the member.
- Every change is committed on `afkmode/start`.

## 4. Out of scope
The Stop hook, close, status, teams, the ui, the skill text, install, and every doc file: other members own them. A Codex lead in AFK mode. An `afk finish` or `afk status` command. Pushing, pull requests, `cargo install`, `teamwire install`.

## 5. When to do nothing
If `git log --oneline -1 main` in your worktree is not fb1fd8f, or `grep -n afk_budget_ends_at crates/teamwire-store/src/team.rs` finds nothing, stop: write that in your report and run `teamwire done --status blocked`.

## 6. Where you are
Your worktree is on branch `afkmode/start`, based on `main` at fb1fd8f. Setup already ran `bun install --frozen-lockfile` in ui/. Cargo builds in the worktree's own target/ (about 90 s cold, for the first check).

## 7. Read first, in this order
1. AGENTS.md (the hard rules), then docs/conventions.md section 5 (layers).
2. docs/design/2026-10-19-afk-mode.md sections 4, 5.1, 5.2, 5.4, and the model table in section 6 part "4. Members you can add".
3. /private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/afk/task.md (binding names).
4. /private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/afk/decisions.md entries D3, D4, D8, D9, D12, D18, D19, D20.
5. The code in section 9.

## 8. Decisions already made (2026-10-23, full text in afk/decisions.md)
- D3: you change `create_afk_team` to `create_afk_team(at, team: &NewTeam, budget_ends_at, extra: &serde_json::Value)`. Merge `extra`'s keys and then `afk_budget_ends_at` into the `team.created` data through `create_team_with`. Update the caller at crates/teamwire-store/src/team/tests.rs:48.
- D4: before creating anything, look up the lead's process. When `platform.sleep_hold()` is Some and the lead process is not found, refuse `no_lead_process`. After the team exists, call `ensure_hold` (make it `pub`) and print its warnings. Launch's behaviour stays the same.
- D8: the AFK table by kind: implementer opus/high, investigator opus/high, reviewer opus/xhigh, closer opus/xhigh. Rank models haiku < sonnet < opus and effort low < medium < high < xhigh < max. Unset or unknown values never warn. Text: `AFK run: member <role> (<kind>) runs on <model> at <effort>, below the AFK table (<table model> at <table effort>)`, for example `AFK run: member start (implementer) runs on sonnet at high, below the AFK table (opus at high)`, pushed to the plan's warnings, so the CLI prints `warning: ...`. Only for a team with `afk_budget_ends_at` set.
- D9: `afk start` runs doctor's checks through doctor.rs (`diagnose` at crates/teamwire-cli/src/cmd/doctor.rs:633), keeping only host, harness binary, lead setup (plugin) and `.worktrees/` ignored. You may add one helper in doctor.rs that returns those checks. A FAIL refuses `doctor_failed: <check name>: <detail>`; a warn is printed and does not refuse.
- D12: skip the environment-bound test `herdr_env_wins_over_tmux` in your check; it fails under tmux at the base.
- D18: past the budget, launch refuses each `implementer` as a per-member `LaunchOutcome::Refused` with reason `afk_budget_spent: the AFK budget ended at <budget_ends_at>; only non-implementer members can join`. Other members start; the CLI exits PARTIAL. `--dry-run` prints the same text as a warning. Time from `ctx.clock`.
- D19: the lead harness and host come from the wiring as launch takes them; `current_session()` None refuses `no_lead_session`; a Background session refuses `background_lead` with launch's message; `afk_already_running` names the open AFK team whose `lead_session` is this session (read each store read-only, skip and warn on a store that cannot be read). `--budget` parses with `parse_duration` (crates/teamwire-cli/src/cmd/metrics.rs:75, make it `pub(crate)`); an error or zero refuses `bad_budget`. Default `8h`.
- D20: refusals print `refused: <code>: <message>` on stderr and exit REFUSED, as crates/teamwire-cli/src/cmd/launch.rs:80. Success prints, one per line: `team <name> created for an AFK run`, `afk dir: <team dir>/afk`, `report: <team dir>/afk/REPORT.md`, `budget ends: <budget_ends_at>`, `keep the lid open and the machine on power: a closed lid still sleeps a Mac`.
- The `team.created` event of an AFK team carries `{"lead_cwd": <the lead's working directory>}` from `afk start` (task.md) plus `afk_budget_ends_at`.
- Name and title checks use launch's rules and messages. A name already taken refuses `team_exists`, as validate.rs:58 words it.
- Other members tell `skill` (the docs owner) their final names. After your last commit, send `teamwire send --to skill --type notice --commit <sha> --text "..."` with the command's exact output lines and refusal codes.

## 9. What already exists (do not rebuild)
- `Tx::create_team_with(at, team, extra)`: crates/teamwire-store/src/team.rs:104. `create_afk_team`: team.rs:150.
- `ensure_hold`: crates/teamwire-launch/src/hold.rs:15 (pub(crate)); `lead_process`: hold.rs:66.
- Launch's lead lookup: crates/teamwire-launch/src/validate.rs:41 (`current_session`, the background refusal), :57 (team exists / `--add` rules), :211 (`lead_plan`). `--add` does not count members, so a zero-member team should already be accepted: prove it with a test, and change the refusal only if one appears, only for AFK teams.
- Per-member refusal: crates/teamwire-launch/src/member.rs:155. Plan warnings: crates/teamwire-launch/src/plan_rules.rs (`planning_warnings`, called at validate.rs:116).
- CLI launch output and exit codes: crates/teamwire-cli/src/cmd/launch.rs:80, :133, :163.
- Doctor: `diagnose` doctor.rs:633, `pick_host` :170, `worktrees_check` :459, `lead_setup_checks` :613, `binary_checks` :544.
- `parse_duration`: crates/teamwire-cli/src/cmd/metrics.rs:75.
- Test fakes: `teamwire_fakes::TestHome` and the fakes in tests/fakes/ (never a real herdr pane or Claude session). Copy the setup of crates/teamwire-cli/tests/launch tests and crates/teamwire-launch/tests/ for fake hosts and harnesses.

## 10. Units in order (commit after each, each with its test)
Use the tdd skill for each unit; test only at these seams: `Tx::create_afk_team` through the store's public API; launch's `validate` and `launch` through the launch tests' fakes; `teamwire afk start` through the CLI binary with `teamwire_fakes::TestHome`. If a check fails and the cause is not clear from its output, use the diagnosing-bugs skill before your second fix attempt.
1. Store: the `extra` parameter. Test in crates/teamwire-store/tests/afk.rs: `team.created` data has `lead_cwd`, `afk_budget_ends_at`, and the usual keys; the row has the column set.
2. Launch: crates/teamwire-launch/src/afk.rs with the budget check (D18) and the model table (D8), and their call sites. Tests in crates/teamwire-launch/tests/afk.rs: implementer refused past the budget and a reviewer accepted; before the budget both start; a non-AFK team unaffected; one warning per low member; no warning for unknown model.
3. Launch: `--add` into a zero-member AFK team (test only, unless it fails).
4. Launch: the lead-process lookup exported for `afk start` (D4), `ensure_hold` pub.
5. CLI: `teamwire afk start` in crates/teamwire-cli/src/cmd/afk.rs, args.rs, main.rs, the doctor helper, `parse_duration` pub(crate). Tests in crates/teamwire-cli/tests/afk.rs: success output and files (`afk/task.md` byte-identical, row fields as launch records them), each refusal, nothing created on refusal.
6. Send the notice to `skill` (section 8).

## 11. Files you own, and files owned by others
You own: crates/teamwire-store/src/team.rs, crates/teamwire-store/src/team/tests.rs, crates/teamwire-store/tests/afk.rs, crates/teamwire-launch/src/afk.rs, lib.rs, hold.rs, validate.rs, member.rs, plan_rules.rs, crates/teamwire-launch/tests/afk.rs, crates/teamwire-cli/src/cmd/afk.rs, crates/teamwire-cli/src/main.rs (the `mod cmd` list at main.rs:16 and the dispatch), wiring.rs, args.rs, cmd/doctor.rs, cmd/metrics.rs, crates/teamwire-cli/tests/afk.rs.
Owned by others, do not edit: crates/teamwire-report/, crates/teamwire-cli/src/cmd/hook.rs (stophold); crates/teamwire-close/, crates/teamwire-watch/, crates/teamwire-msg/, cmd/status.rs, cmd/teams.rs, ui/ (endings); examples/, crates/teamwire-harness-claude/, docs/, README.md (skill). Protected, nobody edits: root Cargo.toml, Cargo.lock, clippy.toml, rustfmt.toml, rust-toolchain.toml, scripts/, xtask/, tests/fakes/, docs/adr/.
Hard rules that matter here: every store write inside `Store::write`, and no write transaction open during an external call (the doctor checks and the hold run outside it); files outside SQLite (`afk/task.md`) through `Platform::write_atomic`; time from the `Clock` trait; only teamwire-cli prints or exits; no unwrap, expect, panic or todo outside tests; `#[expect(lint, reason = "...")]` only; no new dependency, no Cargo.lock change; split a source file past 400 lines, tidy fails past 800 (doctor.rs is 760: put new code in afk.rs, not doctor.rs, where you can).

## 12. Verification
Run `cargo test -p teamwire-store -p teamwire-launch -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux`, then `scripts/gate.sh fast`, then `scripts/test-changed.sh` (it adds the crates that depend on yours; expect several minutes). Do not run the full gate; the closer does.

## 13. How to finish
No push, no pull request. Write /private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/reports/start.md: one progress line per commit (sha and what it did), the commands you ran with their exit codes, and a **Decisions I made** list at the end. Then run `teamwire done`.

## 14. Open questions you must not answer
None.
