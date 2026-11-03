# Review of afkmode/stophold (the AFK hold in the lead's Stop hook)

## Summary
- Verdict: the work is sound. The production code matches design 5.3, the brief and decisions D6, D21 and D22. I found no correctness, security or data-loss defect.
- Both note texts match D6 word for word. The team directory is absolute, because `state_home` passes the path through `std::path::absolute`.
- The hook joins the watch notes first and the AFK notes second with "\n", inside one `within(LEAD_STOP_BUDGET, ...)` call, as D22 says. The hook opens every store read-only and writes nothing.
- One finding (low): the unreadable-team test never reaches the code path it names. A mutation that makes one unreadable team hide every AFK note still passes all 11 tests.
- I ran the implementer's check in a scratch export of the branch. `cargo test -p teamwire-report -p teamwire-cli --locked --no-fail-fast -- --skip herdr_env_wins_over_tmux` exited 0, with 35 test binaries ok and 0 failed.
- I did not run `scripts/gate.sh fast` or `scripts/test-changed.sh`. The export has no `.git`, and the gate's tidy step compares against `main`.

## Findings (ranked by harm)

### 1. Low: the unreadable-team test passes without the behavior it names
- File: crates/teamwire-report/tests/afk_hold.rs:224 (`an_unreadable_team_gives_no_note_and_the_others_still_do`). The untested code is the `Err(e)` branch at crates/teamwire-report/src/afk_hold.rs:44.
- Brief line the finding rests on (section 3): "crates/teamwire-report/tests/afk_hold.rs exists and covers: ... and an unreadable team give no note." Design 5.3 says: "A team row that cannot be read means no AFK hold for that team".
- The test replaces `teams/beta/team.db` with a directory. The store's `team_names` (crates/teamwire-store/src/store.rs:114) lists a team only when `database_path(...).is_file()` is true. So team `beta` never reaches `afk_hold_notes`'s loop at all. The test checks the listing filter, not the "skip a team that cannot be read and keep the others" rule.
- Concrete failure: I changed the `Err(e)` branch at afk_hold.rs:44 in my scratch copy to `return Vec::new()`. With that change, one unreadable team hides every AFK note. `cargo test -p teamwire-report --locked --test afk_hold` still passed 11 of 11. In production, a `team.db` that exists but is not a teamwire database (wrong header, foreign schema, newer schema) does reach that branch. So a later change that drops the AFK hold for every team because of one damaged team would pass this test.
- Today's code handles that branch correctly (it logs and continues). Only the test is weak.

## Requirements checked and found present
| Requirement | Where |
| --- | --- |
| `afk_hold_notes(home, session, platform, clock) -> Vec<String>` exported beside `teams_without_watch` and `watch_missing_note` (task.md, binding) | crates/teamwire-report/src/lib.rs:30 |
| Empty list when any open team this session leads has a working member and a live watch lease (D22) | afk_hold.rs:38, :70-73 |
| One note per open AFK team this session leads, in `team_names` order (D22) | afk_hold.rs:36-51 |
| Note before the budget, and the "budget spent at" note when now >= budget end (D6) | afk_hold.rs:80-89 |
| "Has a working member" rule factored into a `pub(crate)` helper in lead_stop.rs, not copied (brief section 9) | lead_stop.rs:66 |
| Hook reply is the watch notes, then the AFK notes, in one `within` call; no hold when both lists are empty (D22) | crates/teamwire-cli/src/cmd/hook.rs:199-215 |
| `stop_hook_active` and subagent turns hold nothing | hook.rs:193 (unchanged); tests lead_hook.rs `an_afk_continuation_is_not_held_again`, `an_afk_subagent_stop_is_not_held` |
| The hook writes nothing | lead_hook.rs `an_afk_team_with_nothing_to_wake_the_lead_is_held` compares the event count |
| One `afk_team(...)` helper per test crate calls `create_afk_team` with three arguments (D21) | afk_hold.rs tests:55, lead_hook.rs:62 |
| Brief section 3 test list for afk_hold.rs and lead_hook.rs | all present; finding 1 covers the one weak test |

Every other new test fails when its rule is removed. I checked this by reading each test against the code: the closed-team, other-session, non-AFK, live-watch-elsewhere, equal-to-budget and name-order tests each pin one condition.

## Notes for the parent (not defects)
- After `start` merges, `create_afk_team` takes four arguments. The two test helpers named in D21 then fail to compile until each gets a fourth argument (afk_hold.rs tests:60 and lead_hook.rs:73). D21 already plans this one-line change per crate.
- A team in the `closing` state gets no AFK hold, because the code checks for `TeamState::Open`. That matches design 5.3's word "open" and D22, so I record it only for the closer.

## Commands I ran
- `git archive afkmode/stophold | tar -x` into my scratchpad, then the implementer's check there: exit 0.
- Mutation in the scratch copy only (the `Err` branch returns an empty list), then `cargo test -p teamwire-report --locked --test afk_hold`: 11 passed. This run shows finding 1.
- I edited nothing in the repository and committed nothing.

## Decisions I made
- I ran the implementer's check in a `git archive` export inside my scratchpad, not in the implementer's worktree, so that my build wrote nothing into another member's directory.
- I did not run `scripts/gate.sh fast`. The export has no git history for tidy's `--base` comparison, and my brief says not to run the full gate.
