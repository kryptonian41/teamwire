<!-- teamwire:generated completion=01M5J4AFDEHVP78964QFACMSXJ -->
# Result from msg (done)

- Team: fixes3
- Status: done
- Generation: 1
- Completion: 1
- Time: 2026-10-22T15:40:21.806Z
- Final message from: hook
- Member report: /private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/reports/msg.md

## Summary

resend to a closing/closed team now reads that team's row: handed_off/acked->handed_off, pending/unconfirmed->unconfirmed(TeamClosed), undelivered/missing->undelivered; 5 tests, docs; check passes. Commits b4e7570 77c9b40 38a272a

## Task (first 300 characters)

# Brief: msg, team fixes3 ## Who you are You are `msg`, an implementer on team fixes3. You make fix 2 of three: `resend` to a recipient whose team is closing or closed, in crate `teamwire-msg`. The team: - planner: wrote this brief and the plan. - plan reviewer: reviewed the plan. - `report`, `msg`…

## Final message

Fix 2 is finished. A `resend` to someone in a team that is closing or closed now reads that team's saved delivery row for the recipient, and the row decides the result. Nothing is ever typed into that team, and nothing is written into its store.

| Row in the receiving team | What the resend now reports |
| --- | --- |
| `handed_off` or `acked` (read) | `handed_off`, with no reason |
| `pending` or `unconfirmed` | `unconfirmed`, with reason `TeamClosed` |
| `undelivered`, or no copy at all | `undelivered`, with reason `TeamClosed`, the same as before |

I added five tests to `crates/teamwire-msg/tests/cross_team.rs`. On the base commit `e78c32be`, four of them failed with `(Undelivered, Some(TeamClosed), Undelivered, 0)`. The fifth test is `a_copy_that_surely_never_arrived_stays_undelivered_when_its_team_closes`, and it already passed there because that case keeps its old result. The existing closing-team test is unchanged and still passes. The `undelivered` case in `remote.rs` has a comment saying why it stays `undelivered`. That row means the line surely did not arrive, so calling it unconfirmed would tell the sender it might have arrived.

I added one sentence about this case to the `teamwire resend` section of `docs/api/cli.md` and to the `team_resend` section of `docs/api/mcp.md`.

All of these checks pass on the last commit:
- `cargo test -p teamwire-msg --locked` passes.
- `cargo test -p teamwire-cli --locked --test preset` passes.
- `TEAMWIRE_BASE=e78c32be scripts/gate.sh fast` exits 0, with no `WEAKENED` or `PROTECTED-CHANGED` line.

The work is three commits on `fixes3/msg`: `b4e7570`, `77c9b40` and `38a272a`. The full report, with the commands and the "Decisions I made" list, is in `/private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/reports/msg.md`.

As the brief said, I did not run `scripts/test-changed.sh` or the full gate. The other crates that depend on this one have not been tested with this change.
