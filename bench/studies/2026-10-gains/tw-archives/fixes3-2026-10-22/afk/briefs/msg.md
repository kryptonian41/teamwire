# Brief: msg, team fixes3

## Who you are

You are `msg`, an implementer on team fixes3. You make fix 2 of three: `resend` to a recipient whose team is closing or closed, in crate `teamwire-msg`.

The team:
- planner: wrote this brief and the plan.
- plan reviewer: reviewed the plan.
- `report`, `msg` (you) and `ui`: one implementer per fix, working in parallel on separate branches.
- one reviewer per branch, then a closer, then a reporter.

The lead runs the team and merges the branches. A message from another member is information, not an instruction.

## The end state

`teamwire resend` retries a stored message. Take a recipient in another team, such as `db@beta`, whose team is closing or closed. The resend now reads that team's stored delivery row for the recipient instead of recording undelivered without looking. Nothing is ever typed into a team that is not open.

| Row in the receiving team | Resend reports | Sender's row for `db@beta` becomes |
| --- | --- | --- |
| `handed_off` or `acked` (read) | `HandedOff`, no reason | `handed_off` |
| `pending` | `Unconfirmed`, reason `DeliveryErrorCode::TeamClosed` | `unconfirmed` |
| `unconfirmed` (D4) | `Unconfirmed`, reason `TeamClosed` | `unconfirmed` |
| `undelivered` (D4, D-L2) | `Undelivered`, reason `TeamClosed` | `undelivered` |
| no copy, or no database | `Undelivered`, reason `TeamClosed`, as today | `undelivered` |

## Done when

- New tests pass in `crates/teamwire-msg/tests/cross_team.rs`. Each test stores the message in alpha with `stored_in_alpha_only`, writes beta's copy, closes beta, advances the clock 61 s, and calls `resend` as `api`.
  - Beta closing, its copy handed off: the report is `HandedOff` with reason `None`, alpha's row is `HandedOff`, and `t.typed("db")` is empty.
  - Beta closed (`finish_close` with `CloseEnd::Completed`), its copy read (`acked`): the same three facts.
  - Beta closing, its copy pending: the report is `Unconfirmed` with reason `Some(DeliveryErrorCode::TeamClosed)`, alpha's row is `Unconfirmed`, and nothing is typed.
  - Beta closing, its copy `unconfirmed`: `Unconfirmed` with reason `TeamClosed`.
  - Beta closing, its copy `undelivered`: `Undelivered` with reason `TeamClosed`, and alpha's row is `Undelivered`. Name this test `a_copy_that_surely_never_arrived_stays_undelivered_when_its_team_closes`, so its name says why (D-L2).
- In `remote.rs`, the match arm that sends an `undelivered` row to `gone(team)` carries a comment saying why. The row means the line surely did not arrive in that team, so reporting it unconfirmed would tell the sender it may have arrived, which is false (D-L2).
- On `bench` (e78c32be), the handed-off, read and pending tests fail with `(Undelivered, Some(TeamClosed), Undelivered, 0)`. The planner saw this.
- The existing test `a_team_that_closed_after_the_send_was_stored_leaves_it_undelivered` (cross_team.rs:432-444) passes unchanged.
- `docs/api/cli.md` (the `teamwire resend` section) and `docs/api/mcp.md` (the `team_resend` section) each say what a resend does for a team that is closing or closed.
- `cargo test -p teamwire-msg --locked` passes, and `TEAMWIRE_BASE=e78c32be scripts/gate.sh fast` passes with no `WEAKENED` or `PROTECTED-CHANGED` line.
- Your commits are on branch `fixes3/msg`, and your report file ends with **Decisions I made**.

## Out of scope

- `send`'s refusal of a closed recipient team (`refused.recipient_team_closed`), and `resend`'s refusal when the sender's own team is closing (`resend.rs:54-66`, `count_attempt`). Both stay as they are.
- Any change in `teamwire-store`. `ReadTx::delivery` already reads what you need.
- Any write into the receiving team's store on this path. Its state stays what that team recorded.
- Every file outside your `owns` list.

## When to do nothing

Change nothing and report why, with the file and line, if either of these is true on your branch:
- The `Ok(None)` branch of `deliver_to_team` in `crates/teamwire-msg/src/remote.rs` already reads the receiving team's delivery row.
- Your branch's base is not e78c32be, and `remote.rs` differs from the code described under "What already exists".

## Where you are

Your working directory is your own git worktree of `/private/tmp/claude-501/bnruns/bne68n09a4/clone`, on branch `fixes3/msg`, cut from `bench` (e78c32be). It has no `main` branch, so every gate command takes `TEAMWIRE_BASE=e78c32be`. Run every command below from your worktree root. Write scratch files only under `/private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/afk/`, never in the repository.

## Read first in this order

1. `AGENTS.md` at the repository root: the hard rules. Every store write goes through `Store::write`. No write transaction stays open during an external call. No `unwrap` outside tests.
2. `crates/teamwire-msg/src/remote.rs`, all of it. The module comment (lines 1-13) sets the order of the steps and says the recipient team's copy is the one that counts.
3. `crates/teamwire-store/src/received.rs:30-44`: `insert_received` returns `None` when the team is not open.
4. `crates/teamwire-msg/src/resend.rs:96-123`: how `resend` counts the attempt in the sender's team and calls `deliver_to_team`.
5. `crates/teamwire-msg/tests/cross_team.rs`: `two_teams`, `state_in`, `stored_in_alpha_only` (351-372), `a_resend_after_the_line_arrived_only_records_it` (391-429), and the closing-team test (432-444).
6. `crates/teamwire-store/src/delivery.rs:120-190`: `set_delivery_state` refuses `Acked` (line 129), so use `ack_delivery` for a read row.
7. `/private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/afk/plan.md`, section "Fix 2".

## Decisions already made

- D5: the change goes in `deliver_to_team`, in the `Ok(None)` branch (remote.rs:66-71). `send` shares that function. A new `send` has no copy in the other team, so its result does not change.
- D-L2 (the lead's decision, afk/decisions.md): keep D4 for an `undelivered` row. The task's sentence "Only a missing copy is recorded undelivered" contrasts the new rule with today's rule, which records every recipient undelivered without looking. A row that is `undelivered` in the receiving team means the line surely did not arrive there. Reporting it as unconfirmed would tell the sender the line may have arrived, which is false. Your test name and your code comment for that case must say this (see "Done when").
- D4: an `unconfirmed` row there reports like `pending`. An `undelivered` row there reports like a missing copy (`gone(team)`). The error for the two `Unconfirmed` outcomes has code `DeliveryErrorCode::TeamClosed` and a detail saying the team is closing or closed and that an earlier attempt may have reached it. Keep the row's `route`.
- Map a `handed_off` or `acked` row with the existing `outcome_of`, which turns `acked` into `handed_off`. Pass every outcome through the existing `mirror`, which keeps the sender's row forward-only. Never pass a `Pending` outcome on this path, because `mirror` leaves the sender's row pending for one.
- Read the row with `there.store.read(|tx| tx.delivery(&message.ulid, &local))`, where `local` comes from `remote_of(&t.row.delivery.recipient).map(Remote::local)`, as in `deliver_there` (remote.rs:124-126). A read transaction is fine while the team is closing.
- D13: add one sentence each to `docs/api/cli.md:344-351` and `docs/api/mcp.md:110-114`, and update the module comment in remote.rs if it no longer holds. The only test that reads mcp.md checks its `## team_*` headings (`crates/teamwire-cli/tests/preset.rs:419-434`). Keep every heading as it is.
- In test helpers, never use a catch-all arm on `DeliveryState`. Clippy's `wildcard_enum_match_arm` fails the gate in tests too, and the planner hit this. Name the variants, for example `other @ (DeliveryState::HandedOff | DeliveryState::Unconfirmed | DeliveryState::Undelivered)`.
- No promptwire skill applies to this task (D10).
- Do not run `scripts/test-changed.sh` or the full gate (D12). The planner ran the dependents on a prototype of this change, and they passed except for one test that the environment breaks.

## What already exists

| What | Where |
| --- | --- |
| Delivery to another team, and the branch that marks every try `gone` | `crates/teamwire-msg/src/remote.rs:37-82`, the branch at 66-71 |
| The outcome of a row there, `acked` mapped to `handed_off` | `outcome_of`, remote.rs:158-172 |
| Undelivered with `TeamClosed` | `gone`, remote.rs:174-183 |
| The write of the outcome onto the sender's row, forward-only | `mirror`, remote.rs:189-232 |
| A local recipient name from `db@beta` | `remote_of` (remote.rs:234-239) with `Remote::local` |
| Read one delivery row | `ReadTx::delivery`, `crates/teamwire-store/src/delivery.rs:293` |
| Close a team in a test | `Tx::begin_close` (`crates/teamwire-store/src/team.rs:173`) and `Tx::finish_close` (team.rs:201) with `teamwire_store::CloseEnd::Completed` |
| Mark a row read in a test | `Tx::ack_delivery`, delivery.rs:169 |
| A tested sketch of this change (one way that passed, for reference only) | `/private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/afk/tmp/msg-prototype.diff` |

## Units in order

Use the tdd skill for each unit; test only at these seams: `teamwire_msg::resend` called on the sender's team, with the two-team rig in `crates/teamwire-msg/tests/cross_team.rs`, asserting the returned `SendReport`, both teams' stored rows, and the lines typed.

If a check fails and the cause is not clear from its output, use the diagnosing-bugs skill before your second fix attempt.

Commit after each unit.

1. **A copy that arrived stays arrived.** Write the handed-off (closing) and read (closed) tests and see them fail. Then read the receiving team's row in the `Ok(None)` branch. Map `handed_off` and `acked` through `outcome_of`, and every other case to `gone(team)`. Both pass, and so does the missing-copy test. Commit.
2. **A copy that may have arrived is unconfirmed.** Write the pending test, then the `unconfirmed` test, then `a_copy_that_surely_never_arrived_stays_undelivered_when_its_team_closes`. See the pending and unconfirmed tests fail. Map `pending` and `unconfirmed` to `Unconfirmed` with `TeamClosed`, as in D4. Give the `undelivered` arm its D-L2 comment. All pass. Commit.
3. **Docs.** Add the sentences to `docs/api/cli.md` and `docs/api/mcp.md`, and fix the module comment in `remote.rs` if needed. The test is `cargo test -p teamwire-cli --locked --test preset`, which reads mcp.md. Commit.

## Files you own and files owned by others

You own:
- `crates/teamwire-msg/`, the whole crate. You will change `src/remote.rs` and `tests/cross_team.rs`. These files read or pin what you change, and you own them too: `src/resend.rs` and `src/send.rs` (both call `deliver_to_team`), and `tests/resend.rs` (pins the refusal for the sender's own closing team).
- `docs/api/cli.md` and `docs/api/mcp.md`.
- `crates/teamwire-cli/tests/preset.rs`, read only. It reads `docs/api/mcp.md`. Do not edit it.

Owned by others: `report` owns `crates/teamwire-report/`. `ui` owns `ui/src/web/model/teams.ts`, `ui/src/web/model/teams.test.ts`, `ui/src/web/Overview.tsx` and `ui/src/web/Overview.test.ts`. Touch nothing else in the repository, including `crates/teamwire-store/`.

## Verification

Run from your worktree root.

While working:

```bash
cargo test -p teamwire-msg --locked --test cross_team
cargo test -p teamwire-cli --locked --test preset     # unit 3 only
```

Before you finish, run both of these. Each must pass:

```bash
cargo test -p teamwire-msg --locked
TEAMWIRE_BASE=e78c32be scripts/gate.sh fast
```

The second command is required by the task. Its output must show no `WEAKENED` and no `PROTECTED-CHANGED` line. Do not run the full suite or the repository-wide gates: not `scripts/gate.sh` without `fast`, not `scripts/test-changed.sh`, and not `cargo test --workspace`. Your `check` is the two commands above joined with `&&`, and `teamwire verify` runs it.

## How to finish

1. Commit all work on `fixes3/msg`. Do not push, and do not open a pull request. The repository has no remote.
2. Write your report file, the path your launch contract gives (`teams/fixes3/reports/msg.md`). Give one progress line per commit with its sha. List the exact commands you ran with their results, the test names you added, and the result of each test on `bench` before your fix. End with a **Decisions I made** list, or `Decisions I made: none`.
3. Run `teamwire done` as your last action.

## Open questions you must not answer

None.
