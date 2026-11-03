# revmsg report: review of branch fixes3/msg, team fixes3

## Summary

1. Verdict: the branch is sound. I found no correctness, security or data-loss problem in the diff `e78c32be...fixes3/msg`.
2. Every "Done when" item in briefs/msg.md is true, and every end-state sentence of task fix 2 has a test.
3. I ran the four new tests against the base version of `remote.rs`. All four fail there with `(Undelivered, Some(TeamClosed), Undelivered, 0)`, so none passes without the fix.
4. On the branch, `cargo test -p teamwire-msg --locked` (104 passed), the fast gate, and the `preset` test of teamwire-cli (10 passed) all pass. The gate printed no `WEAKENED` or `PROTECTED-CHANGED` line.
5. One note for the parent, not a finding against `msg`: ADR 0016 still states the old rule, and `msg` may not edit it.

## Findings

None. I checked each part below and found no defect.

## What I checked

### The code change, `crates/teamwire-msg/src/remote.rs:68-76` and `:179-211`

The new function `held_there` runs only in the branch where the receiving team is not open. The function reads one row of the receiving team with `there.store.read(|tx| tx.delivery(&message.ulid, &local))`, exactly as the brief says. The function writes nothing into the receiving team and types nothing. The read transaction ends before `mirror` writes the sender's row, so the code never locks two stores at once.

| Row in the receiving team | What `held_there` passes to `mirror` | Matches the brief |
| --- | --- | --- |
| `handed_off`, `acked` | `outcome_of(&r)`, which turns `acked` into `handed_off` | yes |
| `pending`, `unconfirmed` | `Unconfirmed`, the row's route kept, `TeamClosed` with the detail the brief asks for | yes (D4) |
| `undelivered` | `gone(team)`, with the comment the brief asks for at `remote.rs:206-208` | yes (D-L2) |
| no row | `gone(team)` | yes |
| no database | unchanged, still `gone(team)` at `remote.rs:50-55` | yes |

`held_there` never passes a `Pending` outcome to `mirror`, as the brief requires. The match names every `DeliveryState` variant, so it has no catch-all arm.

### The connected code I read

- `insert_received` (`crates/teamwire-store/src/received.rs:36-44`) returns `None` before any write when the team is not open. So the branch the fix changes is reached only for a team that is closing, closed, or missing its team row.
- `mirror` (`remote.rs:228-270`) keeps the sender's row moving forward only. I traced each case the new code can produce. A sender row of `pending` or `undelivered` moves to `unconfirmed` or `handed_off`. A sender row of `unconfirmed` is never moved to `undelivered`.
- `report_of` (`crates/teamwire-msg/src/attempt.rs:249-266`) drops the reason for a `handed_off` row. So a resend reports `HandedOff` with no reason, even when the receiving team's row carries an old error code from before it was read.
- `resend` (`crates/teamwire-msg/src/resend.rs:106-122`) and `send` (`crates/teamwire-msg/src/send.rs:131-150`) both call `deliver_to_team`. `send` refuses a recipient team that is not open earlier, in `crates/teamwire-msg/src/rules/foreign.rs:41-43`. So `send` reaches the changed branch only when the team starts closing between that check and the delivery. A new message has no copy there, so `send` still records it undelivered, as before.

### The tests, `crates/teamwire-msg/tests/cross_team.rs:531-656`

Each new test stores the message in alpha with `stored_in_alpha_only`, writes beta's copy, begins closing beta, advances the clock 61 s, and resends as `api`. The read-row test also finishes the close with `CloseEnd::Completed`. Each test asserts the reported state, the reason, alpha's row and the lines typed for `db`.

I copied the base `remote.rs` (e78c32be) into my own clone and ran the test file:

```
an_unconfirmed_copy_in_a_closing_team_stays_unconfirmed      FAILED  left: (Undelivered, Some(TeamClosed), Undelivered, 0)
a_pending_copy_in_a_closing_team_is_recorded_unconfirmed     FAILED  left: (Undelivered, Some(TeamClosed), Undelivered, 0)
a_copy_handed_off_in_a_closing_team_is_recorded_handed_off   FAILED  left: (Undelivered, Some(TeamClosed), Undelivered, 0)
a_copy_read_in_a_closed_team_is_recorded_handed_off          FAILED  left: (Undelivered, Some(TeamClosed), Undelivered, 0)
a_copy_that_surely_never_arrived_stays_undelivered_when_its_team_closes   ok
test result: FAILED. 13 passed; 4 failed
```

The `undelivered` test passes on the base too. The brief expects that, because D-L2 keeps today's result for that row. The closed-team test reaches the changed branch, not the no-database branch. The no-database branch would report `Undelivered`, and the test passes only with `HandedOff`.

The existing test `a_team_that_closed_after_the_send_was_stored_leaves_it_undelivered` is unchanged. The diff of `cross_team.rs` has 127 additions and 0 deletions.

### The task's end-state sentences for fix 2

| Sentence in task.md | True in code | Tested by |
| --- | --- | --- |
| "resend reads that team's stored delivery row for the recipient" | `remote.rs:192` | all five new tests |
| handed off or read: reports `HandedOff`, sender's row handed off, nothing typed | `remote.rs:197` | the handed-off test (closing) and the read test (closed) |
| pending: reports `Unconfirmed` with `TeamClosed`, sender's row unconfirmed | `remote.rs:198-205` | the pending test |
| "Only a missing copy is recorded undelivered, as today" | `remote.rs:193-195` | the existing closing-team test; the `undelivered` row follows D-L2 |

### Brief compliance

- The diff touches only files `msg` owns: `remote.rs`, `cross_team.rs`, `docs/api/cli.md` and `docs/api/mcp.md`. Nothing in `teamwire-store` changed.
- The docs sentences in `docs/api/cli.md:350-355` and `docs/api/mcp.md:115-118` match what the code does for every row state the code can see. Every `## team_*` heading in `mcp.md` is unchanged, and the `preset` test passes.
- The new comments explain why and carry no ticket or decision labels.
- Three commits, one per unit in the brief, all on `fixes3/msg`. The member report has one progress line per commit and ends with **Decisions I made**.

## Commands I ran (in my clone at afk/tmp/revmsg-clone, never in the repository)

| Command | Result |
| --- | --- |
| `cargo test -p teamwire-msg --locked` on fixes3/msg | 104 passed, 11 suites |
| `cargo test -p teamwire-msg --locked --test cross_team` with base `remote.rs` | 13 passed, 4 failed (the four above) |
| `TEAMWIRE_BASE=e78c32be scripts/gate.sh fast` on fixes3/msg | fmt ok, tidy ok, clippy ok; no `WEAKENED` or `PROTECTED-CHANGED` line |
| `cargo test -p teamwire-cli --locked --test preset` on fixes3/msg | 10 passed |

## Note for the parent (not a finding against msg)

ADR 0016 still states the old rule at `docs/adr/0016-messages-between-teams.md:89-90`: "A team that closes after the sender's copy is stored leaves that row `undelivered` with `team_closed`." After this branch, that sentence is false when the closing team already holds a copy. Take a copy handed off or read there: the sender's row becomes `handed_off`. Take a copy still pending there: the sender's row becomes `unconfirmed`. The `docs/adr/` folder is protected, and `msg` does not own it. AGENTS.md says an ADR changes only through a new ADR that supersedes it. The brief does not ask for one, so I record this here and do not count it against the branch.

## Checked and left out

- A store error while reading the closing team's row now makes that recipient's report `pending` through `survive_store_error` (`send.rs:240-257`). Before, the branch could not fail and always reported `undelivered`. `pending` says "may or may not have arrived", which is honest when the row could not be read. So I do not count this as a defect.
- The route kept on an `unconfirmed` outcome, the error detail text, and "beta's store is unchanged after the resend" have no assertion. The code does each of them correctly, and the brief's "Done when" list does not ask for them.

## Decisions I made

- I ran the base comparison by copying only the base `remote.rs` into my clone, so the base code ran against the branch's new tests. The brief asks whether those tests fail on bench, and this setup answers exactly that.
- I recorded the out-of-date ADR 0016 sentence as a note for the parent, not as a finding. The file is protected and outside `msg`'s ownership.
- I ran the `preset` test of teamwire-cli as a check of the member's claim, because the branch edits `docs/api/mcp.md`, which that test reads.
