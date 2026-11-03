# Review of afkmode/stophold (the AFK hold in the lead's Stop hook)

## Summary

The branch is sound. I found no correctness, security or data-loss problem, and no break of an AGENTS.md hard rule.

- Diff reviewed: `git diff afk/base...afkmode/stophold`, commits 1bd20cc, 6d99d48, b449bf3. Six files changed, 326 lines added and 33 removed.
- Both note texts match D16 and D31 character for character. I compared the format string in `crates/teamwire-report/src/afk_hold.rs:84-96` against both decisions.
- The function name, signature, export, ordering, read-only open and log-and-skip rule all match the stophold brief, section 2.
- The hook computes both lists inside the one `within(LEAD_STOP_BUDGET, ...)` call. The hook puts the watch notes first and joins them with `\n` (`crates/teamwire-cli/src/cmd/hook.rs:200-216`).
- All seven named tests exist with the exact names. Each named test fails on afk/base, so none of them is a guard.
- An AFK team with zero members works. `has_working_member` returns false for an empty member list, so the lead gets the AFK note and nothing panics.
- I ran the two named test files on b449bf3. Both passed: `lead_stop` 17 passed, `lead_hook` 10 passed. I did not rerun `scripts/gate.sh fast`.
- No verify log exists for stophold (`verify/` holds only `budget.log`).

One finding, ranked low. The finding is about a gap in the tests, not about the code.

## Findings

### 1. (low, test gap) Two halves of the rule "something will wake the lead" have no test that fails when either half is removed

Where: `crates/teamwire-report/src/afk_hold.rs:74` and `crates/teamwire-report/src/afk_hold.rs:77`.

The code is correct today. No test pins these two parts, though:

| Change to the code | Effect on the 5 report tests and the 2 hook tests | Wrong result a regression would then give |
| --- | --- | --- |
| Line 77: `wakes_lead: watched && has_working_member(tx)?` becomes `wakes_lead: watched` | All 7 tests still pass. | An AFK team whose members have all finished, with the watch process still alive, gets no AFK note. The lead stops and nobody wakes it. |
| Line 74: drop `!watch_lease_is_dead(...)`, so any lease row counts | All 7 tests still pass. | A watch that crashed leaves its lease row. A lead with a working member gets no AFK note, and the dead watch never wakes it. |

Why the tests miss both changes: only `a_live_watch_on_a_working_team_means_no_afk_note` takes a watch lease. That lease is live, and its team has a working member. No AFK test has a lease on a team where every member has finished. No AFK test has a dead lease either. The brief's test list (section 10) does not ask for either case, so the branch still meets "Done when". The existing `teams_without_watch` tests (`a_lease_whose_process_is_gone_does_not_count`) exercise `needs_watch`, not the separate lease check in `afk_hold.rs`.

## Checked and found correct

- Requirement 2.1 (only when nothing wakes the lead, across every open team the session leads, AFK or not). `afk_hold_notes` returns an empty list as soon as any team led by the session has a live lease and a working member (`afk_hold.rs:39-42`). The function returns empty even when an AFK note was collected earlier in the loop. The test with the ordinary team `beta` covers the case across teams.
- The budget boundary. `now >= ends` gives the budget-spent note at the exact end time. The report test advances the clock to exactly `ends`, so `>` in place of `>=` would fail that test.
- The team directory. The note uses `<home>/teams/<team>`, the same path as `database_path` and `paths::team_dir`. The CLI makes `home` absolute through `state_home` (`std::path::absolute`).
- Closed, closing, other-session and unreadable teams give no note. The closed-team test writes `'closed'`, a valid value of the `state` column, so the test reaches the state check and not the error path.
- Hook behaviour that must stay the same: the subagent check, `hold_already_active`, the member-session early return, the exit code, and "writes nothing" are all untouched. `an_afk_team_holds_the_stop_once_with_the_afk_note` checks the event count before and after, and checks that the second stop gets no reply.
- The hook test sets `afk_budget_ends_at` with an `UPDATE`, not with `create_afk_team` (the implementer's recorded decision). The `create_afk_team` call differs only in the `team.created` event data. The hook never reads that data, so the test still runs the real code path. The `UPDATE` writes `Timestamp`'s Display form, the same form `create_afk_team` stores.
- The existing tests are unchanged. The only edit to old test code moves `Rig::team` into `team_ending(name, None)`, with the same behaviour.
- Hard rules: time comes from the `Clock` trait. Library code has no unwrap, expect or panic. The library writes nothing. Every test write goes through `Store::write`. New files stay under 400 lines. No new dependency. No protected file changed.
- New `pub` item: only `teamwire_report::afk_hold_notes`, as the report lists. `has_working_member` is `pub(crate)`.

## Questions for the parent

1. A Codex lead can receive the AFK hold. The Codex lead plugin runs `teamwire hook lead-stop --harness codex` (`crates/teamwire-harness-codex/src/lead_setup/plugin.rs:127`). That command reaches the same `lead_stop_in`, which now adds AFK notes for any harness. On the start branch, `teamwire afk start` takes the lead harness from `wiring.lead_harness` (`crates/teamwire-launch/src/afk.rs:95-99` on afkmode/start). That code does not check for Claude Code. Design section 4 says "Which lead harness gets AFK? Claude Code only. A Codex lead keeps today's behaviour." Neither the stophold brief nor design 5.3 asks the hook to check the harness, so I did not count this against stophold. The question is whether start should refuse a Codex lead, or whether the hook should skip AFK notes for one. Start is the likelier owner.

## Decisions I made

- I ran only the two named test files (`cargo test -p teamwire-report --test lead_stop --locked` and `cargo test -p teamwire-cli --test lead_hook --locked`) to confirm the implementer's claim. I did not rerun the full stophold check or `scripts/gate.sh fast`. The closer runs those on the joined branch.
- I confirmed the two test gaps in finding 1 by reading every AFK test's fixture, not by mutating and rerunning the code. The reviewer role edits no repository file.
- I recorded the Codex-lead point as a question for the parent, not as a finding against stophold, because no line of the stophold brief or of design 5.3 requires the hook to check the harness.
