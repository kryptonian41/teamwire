- 1bd20cc afk_hold_notes in teamwire-report (afk_hold.rs), shared has_working_member, 5 tests | left: hook wiring and its 2 tests
- 6d99d48 lead_stop_in computes afk_hold_notes in the same within() call and appends them after the watch notes; 2 hook tests | left: run the check
- b449bf3 test: slice pattern instead of indexing (clippy) | left: run the check

## Result

Check (from the worktree root), last commit passes:
`cargo test -p teamwire-report -p teamwire-mcp -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux` → exit 0, 38 test binaries "ok", 412 passed, 0 failed.
`scripts/gate.sh fast` → fmt ok, tidy ok, clippy ok, exit 0. Tidy also prints `WEAKENED crates/teamwire-cli/src/cmd/afk.rs:14`; that file is from the lead's contract commit fb842b4 and I did not touch it. The gate still exits 0.

New `pub` items: `teamwire_report::afk_hold_notes` (re-exported from `afk_hold.rs`). `has_working_member` in `lead_stop.rs` is `pub(crate)`.

Tests added: 5 in `crates/teamwire-report/tests/lead_stop.rs` and 2 in `crates/teamwire-cli/tests/lead_hook.rs`, named exactly as the brief. Existing tests unchanged (I only extended the `Rig::team` helper into `team_ending`, same behaviour for `team`).

## Decisions I made
- Pulled the per-member loop out of `needs_watch` into `pub(crate) has_working_member(tx)` in `lead_stop.rs`, so both functions use one test; `needs_watch` still checks the lease first.
- `afk_hold_notes` returns empty as soon as any open team of the session has a working member and a live lease (the check covers ordinary teams too).
- In the hook tests the Fixture already creates the team with `create_team`, which I may not edit, so `make_afk` sets `team.afk_budget_ends_at` with an UPDATE through `tx.connection()` rather than `create_afk_team`. The report tests use `create_afk_team`.
- `afk_hold_notes` is `#[must_use]`, like `watch_missing_note`.
- 94e4675 tests: a live watch over finished members and a dead watch over a working member both still get the AFK note (reviewer finding 1) | left: nothing

## Reprompt m002: review of revstophold.md

- Finding 1 (test gap, low): holds. Added `a_live_watch_with_every_member_finished_still_gets_the_afk_note` and `a_dead_watch_on_a_working_team_still_gets_the_afk_note` in `crates/teamwire-report/tests/lead_stop.rs`. I mutated `afk_hold.rs` to prove each fails without its half: `wakes_lead: watched` fails the first, and `is_some()` in place of the liveness check fails the second. I restored the file afterwards (`git diff` empty for it).
- Codex-lead question: no change. Design section 7 puts a Codex lead in AFK mode out of scope, and the reviewer showed no failure, only that the hook is harness-neutral. The hook still holds a Codex lead only when its team has `afk_budget_ends_at`, which only `afk start` sets.
- Check on the last commit: `cargo test -p teamwire-report -p teamwire-mcp -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux` exit 0, 414 passed, 0 failed. `scripts/gate.sh fast` exit 0 (fmt, tidy, clippy ok; the same `WEAKENED ... afk.rs:14` line from the lead's contract commit).
- New `pub` items: unchanged (`afk_hold_notes` only).

Decisions I made (reprompt): none.

## Reprompt m008: closer finding 6

Finding 6 is accurate as a description and I changed no code. Evidence:
- `afk_hold.rs:94` does say "once <dir>/afk/REPORT.md exists" for a team with no members, and D39 (closestat's `afk_report.rs`) lets that team close without a report.
- The note's text is fixed word for word by D16 and D31, and the brief required whole-text tests. Those tests are `an_afk_team_with_nothing_to_wake_the_lead_gets_the_afk_note`, `past_the_budget_the_afk_note_says_budget_spent_after_the_team_tag` and `an_afk_team_holds_the_stop_once_with_the_afk_note`.
- The same sentence is quoted in the design document, `docs/api/cli.md` and `examples/skills/afk/SKILL.md`. Those files are owned by other members, so a text change here would make the code and the docs disagree.
- The effect is mild: the lead reads "If the work is finished, add the reporter, then run `teamwire close`". Following it for an empty team costs an unneeded reporter. `teamwire close` is not refused, and the skill (section 7) gives the right rule.
- A fix needs a new decision (amend D16 and D31) and an edit of the three documents. That is the lead's call. If the lead wants it, the change is one `format!` branch in `note()` for a team with no members, which `afk_hold_notes` would read from the store.

No commit this round. New `pub` items: none.

Decisions I made (reprompt): left the note text as D16 and D31 fix it, for the reasons above.
