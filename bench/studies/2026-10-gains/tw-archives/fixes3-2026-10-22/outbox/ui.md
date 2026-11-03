<!-- teamwire:generated completion=01M5J613Z9SQS713463F4EAGCR -->
# Result from ui (done)

- Team: fixes3
- Status: done
- Generation: 2
- Completion: 2
- Time: 2026-10-22T16:10:12.329Z
- Final message from: hook
- Member report: /private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/reports/ui.md

## Summary

Closer F1 (D-L4 cross-team unread filter), F2 (lead label once per card), F3 (card test for AFK and lead label), F5 (report skip count) fixed; bun test 172 pass 3 skip, typecheck ok, gate fast ok

## Task (first 300 characters)

# Brief: ui, team fixes3 ## Who you are You are `ui`, an implementer on team fixes3. You make fix 3 of three: the dashboard says why a team needs a look, in `ui/`. The team: - planner: wrote this brief and the plan. - plan reviewer: reviewed the plan. - `report`, `msg` and `ui` (you): one implement…

## Final message

I checked all four of the closer's findings against the code. All four were real, and I fixed each one in two new commits on `fixes3/ui`.

| Finding | Fix |
| --- | --- |
| F1: a long message to another team flagged the sender's team as "1 unread" forever | In c66ed52, `problemReasons` now counts an unread delivery only when its recipient has no `@team` part, as the lead decided in D-L4. A one-line comment explains why. A new test fails without the fix: a delivery to `db@beta` alone gives no reason, even when the team is closed. |
| F2: a card showed "lead dead" twice | In b04844d, the card's flags row leaves out the lead's liveness label. The status line at the top of the card already shows that label with its coloured dot. `problemReasons` still returns the label, so `hasProblem` and the model tests do not change. |
| F3: no card test covered `AFK spent` or the lead label | Also in b04844d, a new card test checks `AFK spent` on its own card and not on another card. The test also checks that `lead dead` appears exactly once. Before the F2 fix it failed with 2. |
| F5: my report said 170 pass and left out 3 skipped tests | I corrected the report. The 3 skipped tests need `TW_UI_LIVE=1`, and they skip on the base commit too. |

All checks pass on the last commit, b04844d:
- **`cd ui && bun test`:** 172 pass, 3 skip, 0 fail.
- **`bun run typecheck`:** passes.
- **`TEAMWIRE_BASE=e78c32be scripts/gate.sh fast`:** exits 0, with no WEAKENED or PROTECTED-CHANGED line.

The full detail, including each new test's failure before its fix and the "Decisions I made" list, is in `/private/tmp/claude-501/bnruns/bne68n09a4/twhome/teams/fixes3/reports/ui.md`.
