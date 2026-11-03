# Brief: budget, the AFK budget and model table in launch

## 1. Who you are

You are `budget`, an implementer in team `afkmode`. Nobody will answer a question from the user
during this run; the lead answers questions, and every open detail below is already decided.

The other members of wave 1, each on its own branch off `afk/base`:

| Role | Kind | Owns (summary) | Who uses its work |
| --- | --- | --- | --- |
| start | implementer | `teamwire afk start`; in launch: `afk.rs`, `lib.rs`, `hold.rs`, `validate.rs`, `plan.rs`, `tests/support/common.rs` | the lead, the skill |
| budget (you) | implementer | `crates/teamwire-launch/src/member.rs`, `plan_rules.rs` | every `launch --add` into an AFK team |
| stophold | implementer | the AFK hold in the lead's Stop hook | the lead |
| closestat | implementer | close's report check, status, teams, `ui/src/shared/` | close, status, the dashboard |
| skill | implementer | the AFK skill, its install, every doc (it documents your refusal and warning) | the lead |

After you: a reviewer reads your branch, then a closer runs every check on the joined branch, then a
reporter writes the run's report. The exact files each member owns are in the plan.

## 2. The end state

An AFK run's team is a team whose row has `afk_budget_ends_at` set (`TeamRow.afk_budget_ends_at`).
Design section 5.4 is the behaviour; D14 and D15 fix its details.

1. Past the budget, launch refuses each new implementer and starts every other kind (D14). In
   `launch_all` (`crates/teamwire-launch/src/member.rs`), before a member is recorded: when the team
   row's `afk_budget_ends_at` is set, `ctx.clock.now()` is at or past it, and the member's kind is
   `implementer`, that member's outcome is
   `LaunchOutcome::Refused("afk_budget_spent: the AFK run's budget ended at <ts>; past it only
   reviewers, investigators and closers may join")`, with `<ts>` the `Timestamp` Display. No member
   row, event, worktree or place is made for it. The CLI then prints
   `<role>: refused: afk_budget_spent: ...` with no change to `cmd/launch.rs`. A team without the
   column is unaffected. Validation and `--dry-run` are unchanged.
2. Launch warns once per member of an AFK team whose model or effort is below the AFK table (D15),
   inside `planning_warnings` in `crates/teamwire-launch/src/plan_rules.rs`, which `validate` already
   calls. The team is AFK when `cx.existing` holds a row with `afk_budget_ends_at` set. The table asks
   opus for every kind; effort high for implementer and investigator, xhigh for reviewer and closer.
   Model rank by name, case-insensitive substring: haiku < sonnet < opus; a name holding none of the
   three is not judged. Effort rank: low < medium < high < xhigh < max; another word is not judged.
   A missing model or effort counts as below. The warning text, one per member that falls short:

   ```
   role rev (reviewer): below the AFK table, which asks opus at xhigh or higher; it sets model sonnet, effort high
   role api (implementer): below the AFK table, which asks opus at high or higher; it sets model none, effort none
   ```

   `<kind>` is `MemberKind`'s word; `<model>` and `<effort>` are the spec's strings as written, or
   `none`.

## 3. Done when

- Every test named in section 10 exists with that exact name and passes. Each test of new
  behaviour fails without its change; a test marked (guard) pins today's behaviour and passes on
  afk/base too (D33).
- The check in section 12 passes on your last commit, with its real output in your report.
- Your report lists every new `pub` item (there should be none).

## 4. Out of scope

- `teamwire afk start`, `lib.rs`, `validate.rs`, `hold.rs` and the launch test rig (start).
- The Stop hook, close, status, teams, the dashboard, the skill and every doc: other members.
- Any change to what an ordinary team's launch prints or does.

## 5. When to do nothing

If `crates/teamwire-store/src/team.rs` on your branch has no `afk_budget_ends_at` field on `TeamRow`,
change nothing, say so in your report, and finish with status `blocked`.

## 6. Where you are

Your worktree is `/private/tmp/claude-501/bnruns/bn4tu2e1us/clone/.worktrees/` plus your slug, on
branch `afkmode/budget`, cut from `afk/base` (5b86c1e plus the lead's afk-arguments commit). Setup
already ran `cargo build --workspace --all-targets --locked`, so run no install command.

## 7. Read first, in this order

1. `/private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/afk/plan.md`
2. `/private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/afk/decisions.md`, D6, D14, D15, D23,
   D29, D33, D34.
3. `docs/design/2026-10-19-afk-mode.md`, sections 4 (the table "Which members may launch past the
   budget?") and 5.4, and section 6's "Members you can add" table.
4. `AGENTS.md`, then `docs/conventions.md` section 6.
5. The code: `crates/teamwire-launch/src/member.rs` (`launch_all`, `launch_member`),
   `plan_rules.rs`, `plan.rs` (`Cx`), `validate.rs` (`Existing`), and the rig in
   `crates/teamwire-launch/tests/support/common.rs` (`Rig`, `seed_team`, `team_spec`, `pane_member`,
   `worktree_member`, `Rig::validate`, `Rig::launch`).

## 8. Decisions already made (2026-10-27; full text in decisions.md)

- D14: the refusal is a per-member launch outcome, not a validation error, because section 5.4 starts
  the other kinds in the same launch. Nothing is recorded for the refused member.
- D15: the table rule and the warning text, as in section 2.
- D23, D34: run only your check, which also runs teamwire-cli, the one crate that depends on
  teamwire-launch; the closer runs the full gate on the joined branch.
- D29: `member.rs` already holds `new_team` and `lead_cwd`, which the lead's contract commit
  extracted from `create_team`. start calls both from its own `afk.rs`. Leave them alone.
- D33: a test marked (guard) pins today's behaviour; it need not fail without your change.
- An AFK team in a test: open the store and call `Tx::create_afk_team(at, &new_team, ends,
  &serde_json::json!({}))`, as `seed_team` in common.rs does with `create_team`. Write that helper in
  your own test file; common.rs belongs to start.

## 9. What already exists

- `Tx::create_afk_team` and `TeamRow.afk_budget_ends_at` (teamwire-store). Do not change the store.
- `LaunchOutcome::Refused`, `MemberKind::as_str`, `Timestamp`'s Display, the `Clock` trait and
  `FakeClock` (set its time with its own methods; never read the system clock).
- `planning_warnings`, which already warns about owns overlaps and open implementers.
- `new_team` and `lead_cwd` in `member.rs` (D29); start depends on their signatures.

## 10. Units in order, commit after each

Use the tdd skill for each unit; test only at these seams: launch's `validate` and `launch` with the
launch tests' fakes (`Rig`, `FakePlatform`, `FakeHost`, `FakeHarness`, `FakeClock`), on a team seeded
with `create_afk_team`. If a check fails and the cause is not clear from its output, use the
diagnosing-bugs skill before your second fix attempt.

1. The budget refusal in `member.rs`. Tests in `crates/teamwire-launch/tests/afk_budget.rs`:
   - `past_the_budget_an_implementer_is_refused_and_a_reviewer_starts` (one `--add` spec with both;
     the implementer has no member row afterwards; the reason starts with `afk_budget_spent:`)
   - `before_the_budget_ends_an_implementer_starts`
   - `a_team_without_a_budget_is_unaffected` (guard)
2. The model-table warnings in `plan_rules.rs`. Tests in the same file:
   - `members_below_the_afk_table_get_one_warning_each` (a sonnet implementer, an opus reviewer at
     high, a member with neither key, and an opus closer at xhigh, which gets none)
   - `an_ordinary_team_gets_no_model_warning` (guard)

## 11. Files you own, and files owned by others

You own: `crates/teamwire-launch/src/member.rs`, `crates/teamwire-launch/src/plan_rules.rs`,
`crates/teamwire-launch/tests/afk_budget.rs`.

Owned by others, never edit: `crates/teamwire-launch/src/afk.rs`, `lib.rs`, `hold.rs`, `validate.rs`,
`plan.rs` and `tests/support/common.rs` (start); everything under `crates/teamwire-cli/`,
`crates/teamwire-report/`, `crates/teamwire-close/`, `crates/teamwire-watch/`, `crates/teamwire-msg/`,
`crates/teamwire-harness-claude/`, `ui/`, `examples/`, `docs/`, `README.md` (other members);
`crates/teamwire-store/`, `tests/fakes/`, `Cargo.toml`, `Cargo.lock`, `scripts/`, `xtask/`
(protected or the lead's). For a change in any of them, send the lead a question and carry on.
`member.rs` is about 490 lines and tidy fails a file past 800; keep the new code compact.

## 12. Verification

Your check, from the worktree root:

```bash
cargo test -p teamwire-launch -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux && scripts/gate.sh fast
```

On afk/base plus the lead's contract commit it passes (403 tests, then fmt, tidy and clippy ok).
`crates/teamwire-cli/tests/tmux_wiring.rs::herdr_env_wins_over_tmux` fails on afk/base inside a
tmux pane, before any change (D23). The check skips it by name; do not edit or skip it any other
way. Do not run the full suite,
`scripts/test-changed.sh` or `scripts/gate.sh` without `fast`.

## 13. How to finish

Do not push and do not open a pull request. Commit each unit. Write your report with one progress
line per commit, the check's real output, every new `pub` item, and a **Decisions I made** list.
Then run `teamwire done` (or `team_done`).

## 14. Open questions you must not answer

None. Decide anything the brief does not cover, and list it under **Decisions I made**.
