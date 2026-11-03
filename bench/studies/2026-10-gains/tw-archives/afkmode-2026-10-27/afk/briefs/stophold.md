# Brief: stophold, the AFK hold in the lead's Stop hook

## 1. Who you are

You are `stophold`, an implementer in team `afkmode`. Nobody will answer a question from the user
during this run; the lead answers questions, and every open detail below is already decided.

The other members of wave 1, each on its own branch off `afk/base`:

| Role | Kind | Owns (summary) | Who uses its work |
| --- | --- | --- | --- |
| start | implementer | `teamwire afk start`, doctor's launch checks, the launch crate's AFK start | the lead, the skill |
| budget | implementer | launch's budget refusal and model warnings (`member.rs`, `plan_rules.rs`) | launch |
| stophold (you) | implementer | `crates/teamwire-report/src/lead_stop.rs`, `crates/teamwire-cli/src/cmd/hook.rs` | the lead's Stop hook |
| closestat | implementer | close's report check, status, teams, `ui/src/shared/` | close, status, the dashboard |
| skill | implementer | the AFK skill, its install, every doc (it documents your note) | the lead |

After you: a reviewer reads your branch, then a closer runs every check on the joined branch, then a
reporter writes the run's report. The exact files each member owns are in the plan.

## 2. The end state

When the lead of an AFK run ends a turn and nothing will wake it, its Stop hook holds the stop once
with a note that tells it to decide instead of asking. Design section 5.3 is the behaviour; D16
fixes the text, and D31 fixes the text past the budget.

1. Crate `teamwire-report` exports, beside `teams_without_watch` and `watch_missing_note`:

   ```rust
   pub fn afk_hold_notes(home: &Path, session: &SessionId, platform: &dyn Platform, clock: &dyn Clock) -> Vec<String>
   ```

   It returns one note per open team whose `lead_session` is `session` and whose
   `afk_budget_ends_at` is set, in `team_names` order, but only when nothing will wake the lead: no
   open team led by `session` (AFK or not) has both a started member with no completion for its
   current generation and a live watch lease (`watch_lease_is_dead` false). That is the same
   per-member test `needs_watch` makes, combined the other way. Each team's store is opened
   read-only; a team that cannot be read gives no note and is logged, as `teams_without_watch` does.
2. The note, exactly (D16), with `<dir>` = `<home>/teams/<team>` and `<ts>` = `Timestamp`'s Display:

   ```
   [teamwire:<team>] AFK run: nobody will answer you. If you were about to ask the user, decide it yourself, add it to <dir>/afk/decisions.md, and carry on. If the work is finished, add the reporter, then run `teamwire close <team>` once <dir>/afk/REPORT.md exists. If a command you started will wake you, stop again.
   ```

   When `clock.now()` is at or past `afk_budget_ends_at` (D31):

   ```
   [teamwire:<team>] budget spent at <ts>: AFK run: nobody will answer you. If you were about to ask the user, decide it yourself, add it to <dir>/afk/decisions.md, and carry on. Launch no new implementer: finish what is running, then add the reporter, and run `teamwire close <team>` once <dir>/afk/REPORT.md exists. If a command you started will wake you, stop again.
   ```

   Both notes start with the team tag, as every teamwire line does. Past the budget, the words
   `budget spent at <ts>: ` come right after the tag.
3. `lead_stop_in` in `crates/teamwire-cli/src/cmd/hook.rs` computes `teams_without_watch` and
   `afk_hold_notes` inside its one `within(LEAD_STOP_BUDGET, ...)` call, and replies with
   `harness.hold_reply` when either is non-empty: the watch notes first, then the AFK notes, joined by
   `\n`. Everything else stays: no reply for a subagent's stop or when `hold_already_active`
   (`stop_hook_active`) is true, nothing written, exit 0, no reply when the budget runs out or any
   step errs, no reply in a member session.

## 3. Done when

- Every test named in section 10 exists with that exact name and passes. Each test of new
  behaviour fails without its change; a test marked (guard) pins today's behaviour and passes on
  afk/base too (D33). None of your named tests is a guard.
- The existing tests of `crates/teamwire-report/tests/lead_stop.rs` and
  `crates/teamwire-cli/tests/lead_hook.rs` still pass unchanged.
- The check in section 12 passes on your last commit, with its real output in your report.
- Your report lists every new `pub` item.

## 4. Out of scope

- Any change to the member Stop hook, the PostToolUse hook or the session-start hook.
- A counter or stored state for the hold: it fires once per stop chain, through `stop_hook_active`.
- Noticing a background command the lead started itself (the design accepts that risk).
- `teamwire afk start`, launch, close, status, teams, the dashboard, the skill and the docs.

## 5. When to do nothing

If `TeamRow` on your branch has no `afk_budget_ends_at` field, change nothing, say so in your report,
and finish with status `blocked`.

## 6. Where you are

Your worktree is `/private/tmp/claude-501/bnruns/bn4tu2e1us/clone/.worktrees/` plus your slug, on
branch `afkmode/stophold`, cut from `afk/base` (5b86c1e plus the lead's afk-arguments commit). Setup
already ran `cargo build --workspace --all-targets --locked`, so run no install command.

## 7. Read first, in this order

1. `/private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/afk/plan.md`
2. `/private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/afk/decisions.md`, D6, D16, D23,
   D31, D33, D34.
3. `docs/design/2026-10-19-afk-mode.md`, sections 4 ("How often does the AFK hold fire?"), 5.3, and
   the first risk under "Risks the reviewer should look at".
4. `AGENTS.md`, then `docs/conventions.md` section 6.
5. The code: `crates/teamwire-report/src/lead_stop.rs` (all of it), `crates/teamwire-report/src/lib.rs`,
   `crates/teamwire-cli/src/cmd/hook.rs` (`lead_stop_hook`, `lead_stop_in`, `within`), and the two
   test files you extend.

## 8. Decisions already made (2026-10-27; full text in decisions.md)

- The function name and signature in section 2 are fixed by the task; other members and the docs
  rely on them.
- D16, D31: the note texts and their order, as in section 2.
- D34: your check also runs teamwire-mcp and teamwire-cli, the crates that depend on
  teamwire-report.
- D23: `crates/teamwire-cli/tests/tmux_wiring.rs::herdr_env_wins_over_tmux` fails on afk/base inside
  a tmux pane, before any change. Your check skips it by name. Do not edit or skip it any other way.
- An AFK team in a test: `Tx::create_afk_team(at, &new_team, ends, &serde_json::json!({}))` in place
  of `create_team`. The hook test runs the real binary with the real clock, so give a budget-spent
  team an end in the past (for example `Timestamp::parse("2020-01-01T00:00:00Z")`) and an unspent one
  an end far ahead.

## 9. What already exists

- `teams_without_watch`, `needs_watch`, `watch_missing_note` in `lead_stop.rs` (101 lines); put the
  new code in `crates/teamwire-report/src/afk_hold.rs` if that reads better.
- `Tx::create_afk_team`, `TeamRow.afk_budget_ends_at` (teamwire-store). Do not change the store.
- `teamwire_deliver::watch_lease_is_dead`; the report tests' `Rig` (`team`, `lease`, `complete`); the
  CLI hook tests' `team()`, `hook`, `hook_at`, `stop` helpers.

## 10. Units in order, commit after each

Use the tdd skill for each unit; test only at these seams: `afk_hold_notes` with `TestHome`,
`FakePlatform` and `FakeClock` in `crates/teamwire-report/tests/lead_stop.rs`, and the
`teamwire hook claude-lead-stop` binary with a stdin fixture in
`crates/teamwire-cli/tests/lead_hook.rs`. If a check fails and the cause is not clear from its output,
use the diagnosing-bugs skill before your second fix attempt.

1. `afk_hold_notes` and its export. Tests in `crates/teamwire-report/tests/lead_stop.rs`:
   - `an_afk_team_with_nothing_to_wake_the_lead_gets_the_afk_note` (the whole note text compared)
   - `a_live_watch_on_a_working_team_means_no_afk_note` (the working team may be another, ordinary
     team led by the same session)
   - `a_working_member_without_a_watch_still_gets_the_afk_note`
   - `past_the_budget_the_afk_note_says_budget_spent_after_the_team_tag` (the whole note text
     compared)
   - `an_ordinary_or_closed_team_or_another_session_gets_no_afk_note`
2. The hook. Tests in `crates/teamwire-cli/tests/lead_hook.rs`:
   - `an_afk_team_holds_the_stop_once_with_the_afk_note` (a second stop with `stop_hook_active`
     true gets no reply)
   - `the_afk_note_and_the_watch_note_come_in_one_reply` (watch note first)

## 11. Files you own, and files owned by others

You own: `crates/teamwire-report/src/lead_stop.rs`, `crates/teamwire-report/src/afk_hold.rs`,
`crates/teamwire-report/src/lib.rs`, `crates/teamwire-report/tests/lead_stop.rs`,
`crates/teamwire-cli/src/cmd/hook.rs`, `crates/teamwire-cli/tests/lead_hook.rs`.

Owned by others, never edit: `crates/teamwire-cli/src/cmd/hook/member_tests.rs` and every other file
under `crates/teamwire-cli/` (start, closestat, skill); `crates/teamwire-launch/`,
`crates/teamwire-close/`, `crates/teamwire-watch/`, `crates/teamwire-msg/`,
`crates/teamwire-harness-claude/`, `ui/`, `examples/`, `docs/`, `README.md` (other members);
`crates/teamwire-verify/tests/support/fixture.rs` (the hook tests include it; nobody edits it);
`crates/teamwire-store/`, `tests/fakes/`, `Cargo.toml`, `Cargo.lock`, `scripts/`, `xtask/`
(protected or the lead's). For a change in any of them, send the lead a question and carry on.

## 12. Verification

Your check, from the worktree root:

```bash
cargo test -p teamwire-report -p teamwire-mcp -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux && scripts/gate.sh fast
```

On afk/base plus the lead's contract commit it passes (405 tests, then fmt, tidy and clippy ok). Do not run the full suite,
`scripts/test-changed.sh` or `scripts/gate.sh` without `fast`.

## 13. How to finish

Do not push and do not open a pull request. Commit each unit. Write your report with one progress
line per commit, the check's real output, every new `pub` item, and a **Decisions I made** list.
Then run `teamwire done` (or `team_done`).

## 14. Open questions you must not answer

None. Decide anything the brief does not cover, and list it under **Decisions I made**.
