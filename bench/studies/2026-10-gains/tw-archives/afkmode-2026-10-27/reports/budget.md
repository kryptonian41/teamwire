- a54e5d6 launch_all refuses an implementer past afk_budget_ends_at, with tests | left: model-table warnings in plan_rules.rs
- 669b8bd planning_warnings warns per member below the AFK model/effort table, with tests | left: run the check, finish
- 34098e1 fix clippy in member.rs (expect on the team read) and the tests (no unwrap/panic outside cfg(test)) | left: nothing

## Check
`cargo test -p teamwire-launch -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux`: every suite "ok", no failures (5 new tests in tests/afk_budget.rs pass).
`scripts/gate.sh fast`: fmt ok, tidy ok, clippy ok. Tidy prints WEAKENED lines for member.rs:60 and afk_budget.rs:4 (my new #[expect] attributes, same as reopen.rs/hold.rs use) and cli afk.rs:14 (not mine); the gate still reports ok.
The first test fails without the refusal (checked by disabling the condition). The warnings test fails without the new code (no warning would exist).

## New pub items
none

## Decisions I made
- The budget is read once in launch_all (team row via ctx.store.read), before the member loop, not per member.
- Effort ranks match whole words (case-insensitive); model ranks match by substring.
- A member is warned once if its model OR its effort is below the table; each judged independently, so an unknown model name with a low effort still warns.
- Tests' helper fns carry #[cfg(test)] so clippy's unwrap rules treat them as test code.
- 629e68f launch_all counts only attempted members as requested, with a test (review finding 1) | left: nothing

## Review round (revbudget.md)
Check after 629e68f: 409 passed, 0 failed (was 408; +1 new test) across launch and cli with herdr_env_wins_over_tmux skipped; `scripts/gate.sh fast`: fmt ok, tidy ok, clippy ok.

- Finding 1 (requested counts the refused member): holds. spans.rs:186-192 writes the launch span only when member.started/failed events for the launch id equal `requested`. Fixed in 629e68f: `requested` now counts members that are not budget-refused, decided with one clock reading. Test `a_refused_implementer_is_not_counted_as_requested` fails with `requested = plan.members.len()` (checked) and passes with the fix.
- Finding 2 (read error means no refusal): not fixed. validate reads the same row with `?` just before and refuses with store_unreadable on a lasting error, so only a short error between the two reads reaches this. The launch tests have no seam that makes a store read fail (no rusqlite dev-dependency, fakes have no store fake), so a test that fails without a fix cannot be written inside my files. The `.ok().flatten()` shape is the one hook.rs and cmd/hook.rs already use.
- Note 1 (before_the_budget_ends... passes on base): accepted, not marked guard in the brief; it fails if the refusal ignores the clock.
- Note 2 (report lacked real output): the counts are above: 409 passed, 0 failed.
- Why each #[expect] that verify flags is needed (neither loosens a test):
  - member.rs:59 `clippy::redundant_closure_for_method_calls`: clippy asks for `ReadTx::team` as a path, but I tried it and rustc rejects it ("FnOnce is not general enough"), because the path does not infer the transaction lifetime for `Store::read`. reopen.rs:112 and hold.rs:132 carry the same attribute for the same reason.
  - tests/afk_budget.rs:4 `dead_code` on `mod common`: the shared helpers file serves several test binaries, and each uses only part of it, so unused helpers would fail clippy. Every launch test file carries it, e.g. tests/refusals.rs:3.

Decisions I made (added this round):
- One `ctx.clock.now()` reading decides which members are refused, and `requested` counts the rest. Refusal at the exact end time is unchanged (at or past).
