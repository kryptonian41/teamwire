# Brief: stophold (implementer) for the AFK run "afkmode"

## 1. Who you are
You are `stophold`, an implementer on model sonnet at effort high. You build the AFK hold in the lead's Stop hook. Roster of implementers in this wave, each in its own worktree:
- `start`: `teamwire afk start`, launch's budget and model checks, the store's `create_afk_team` signature (teamwire-store, teamwire-launch, cli afk.rs, args.rs, main.rs, doctor.rs).
- `stophold` (you): teamwire-report and cli hook.rs, files in section 11.
- `endings`: close's report check, status and teams output, the ui zod schemas.
- `skill`: the AFK skill, install rendering it, and every doc file.
The planner, `planner2`, stays open: ask it with `teamwire send --to planner2 --type question` when this brief and the design do not answer something.

## 2. The end state
On branch `afkmode/stophold`: crate `teamwire-report` exports `afk_hold_notes(home: &Path, session: &SessionId, platform: &dyn Platform, clock: &dyn Clock) -> Vec<String>` beside `teams_without_watch` and `watch_missing_note`, and the lead's Stop hook returns the AFK notes (design 5.3) together with the watch notes in one reply. Tested.

## 3. Done when
- `cargo test -p teamwire-report -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux` passes.
- `scripts/gate.sh fast` passes.
- crates/teamwire-report/tests/afk_hold.rs exists and covers: an open AFK team with no member gives one note; a working member plus a live watch lease on any team the session leads gives no note; a working member with no lease gives a note; past the budget the note starts with `budget spent at`; a closed AFK team, a non-AFK team, a team led by another session, and an unreadable team give no note.
- crates/teamwire-cli/tests/lead_hook.rs covers: the AFK note is returned; `stop_hook_active` true gives nothing; a subagent turn gives nothing; both the watch note and the AFK note in one reply.
- Every change is committed on `afkmode/stophold`.

## 4. Out of scope
Creating AFK teams in production code (that is `start`), close, status, the skill, docs. A Codex lead in AFK mode. Any write from the hook: it reads only.

## 5. When to do nothing
If `git log --oneline -1 main` is not fb1fd8f, or `grep -n afk_budget_ends_at crates/teamwire-store/src/team.rs` finds nothing, stop, say so in your report, and run `teamwire done --status blocked`.

## 6. Where you are
Your worktree is on branch `afkmode/stophold`, based on `main` at fb1fd8f. Setup already ran `bun install --frozen-lockfile` in ui/ (you do not change ui/). Cargo builds in your own target/.

## 7. Read first, in this order
1. AGENTS.md (the hard rules).
2. docs/design/2026-10-19-afk-mode.md sections 4 (the rows on the hold), 5.1 and 5.3, and the risk list at the end of section 7.
3. /private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/afk/task.md (the `afk_hold_notes` signature is binding).
4. /private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/afk/decisions.md entries D6, D12, D21, D22.
5. The code in section 9.

## 8. Decisions already made (2026-10-23, full text in afk/decisions.md)
- D6, the note before the budget ends, word for word (`<team dir>` is the absolute `$TEAMWIRE_HOME/teams/<team>`):
  `[teamwire:<team>] AFK run: nobody will answer you. If you were about to ask the user, decide it yourself, add it to <team dir>/afk/decisions.md, and carry on. If the work is finished, add the reporter, then run `teamwire close <team>` once <team dir>/afk/REPORT.md exists. If a command you started will wake you, stop again.`
- D6, the note past the budget (now >= `afk_budget_ends_at`), word for word:
  `[teamwire:<team>] budget spent at <budget_ends_at>: AFK run: nobody will answer you. Finish what is running, add the reporter, then run `teamwire close <team>` once <team dir>/afk/REPORT.md exists. If a command you started will wake you, stop again.`
  `<budget_ends_at>` is the `Timestamp`'s `to_string()`.
- D22: `afk_hold_notes` returns an empty list when any open team led by `session` has a started member with no completion for its current generation and a live watch lease. Otherwise it returns one note per open team led by `session` with `afk_budget_ends_at` set, in `team_names` order. Each store opens read-only; a team that cannot be read is skipped and logged with `tracing::warn!`, as lead_stop.rs:29 does. In `lead_stop_in`, compute the watch teams and the AFK notes in the same `within(LEAD_STOP_BUDGET, ...)` closure; the reply is the watch notes, then the AFK notes, joined with "\n". The existing `turn.subagent || turn.hold_already_active` check (hook.rs:193) already covers `stop_hook_active` and subagent turns.
- D21: in tests, create an AFK team in exactly one helper per test crate, `afk_team(...)`, which calls `tx.create_afk_team(at, &team, ends)` with today's three arguments. When `start` merges its four-argument form, the lead will ask you, or change that one line itself.
- D12: skip `herdr_env_wins_over_tmux` in your check.
- You write no docs; `skill` owns them. After your last commit, send `teamwire send --to skill --type notice --commit <sha> --text "..."` naming the exported function and the two note texts' file:line.

## 9. What already exists (do not rebuild)
- `teams_without_watch`: crates/teamwire-report/src/lead_stop.rs:16; its per-team rule `needs_watch`: lead_stop.rs:38 (open, lead session, live lease, started member without completion for its generation). Reuse that member rule: factor the "has a working member" part into a `pub(crate)` helper in lead_stop.rs rather than copying it.
- `watch_missing_note`: lead_stop.rs:90. `watch_lease_is_dead`: teamwire-deliver.
- The hook: `lead_stop_in` crates/teamwire-cli/src/cmd/hook.rs:179, `LEAD_STOP_BUDGET` hook.rs:20, `within` near hook.rs:229.
- `TeamRow.afk_budget_ends_at`: crates/teamwire-store/src/team.rs:49. `Tx::create_afk_team`: team.rs:150.
- Existing tests to copy the setup from: crates/teamwire-report/tests/lead_stop.rs and crates/teamwire-cli/tests/lead_hook.rs (stdin fixtures, fake clock, `teamwire_fakes::TestHome`).

## 10. Units in order (commit after each, each with its test)
Use the tdd skill for each unit; test only at these seams: `afk_hold_notes` through the crate's public API with a fake platform and clock; `lead_stop_in` through the hook command with a stdin fixture, as crates/teamwire-cli/tests/lead_hook.rs does. If a check fails and the cause is not clear from its output, use the diagnosing-bugs skill before your second fix attempt.
1. crates/teamwire-report/src/afk_hold.rs: `afk_hold_notes` and the two note texts; export from lib.rs. Tests in crates/teamwire-report/tests/afk_hold.rs (section 3 list).
2. crates/teamwire-cli/src/cmd/hook.rs: add the AFK notes to the reply (D22). Tests in crates/teamwire-cli/tests/lead_hook.rs.
3. Send the notice to `skill`.

## 11. Files you own, and files owned by others
You own: crates/teamwire-report/src/afk_hold.rs (new), crates/teamwire-report/src/lead_stop.rs, crates/teamwire-report/src/lib.rs, crates/teamwire-report/tests/afk_hold.rs (new), crates/teamwire-report/tests/lead_stop.rs, crates/teamwire-cli/src/cmd/hook.rs, crates/teamwire-cli/tests/lead_hook.rs.
Owned by others, do not edit: crates/teamwire-store/, crates/teamwire-launch/, cmd/afk.rs, args.rs, main.rs, doctor.rs (start); crates/teamwire-close/, crates/teamwire-watch/, crates/teamwire-msg/, cmd/status.rs, cmd/teams.rs, ui/ (endings); examples/, crates/teamwire-harness-claude/, docs/, README.md (skill). Protected, nobody edits: root Cargo.toml, Cargo.lock, clippy.toml, rustfmt.toml, rust-toolchain.toml, scripts/, xtask/, tests/fakes/, docs/adr/.
Hard rules that matter here: the hook writes nothing and never fails the turn; time from the `Clock` trait; only teamwire-cli prints or exits (teamwire-report returns strings); no unwrap, expect, panic or todo outside tests; `#[expect(lint, reason = "...")]` only; no new dependency, no Cargo.lock change; split a file past 400 lines, tidy fails past 800.

## 12. Verification
Run `cargo test -p teamwire-report -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux`, then `scripts/gate.sh fast`, then `scripts/test-changed.sh`. Do not run the full gate; the closer does.

## 13. How to finish
No push, no pull request. Write /private/tmp/claude-501/bnruns/bnei8xdqkf/twhome/teams/afkmode/reports/stophold.md: one progress line per commit (sha and what it did), the commands you ran with their exit codes, and a **Decisions I made** list at the end. Then run `teamwire done`.

## 14. Open questions you must not answer
None.
