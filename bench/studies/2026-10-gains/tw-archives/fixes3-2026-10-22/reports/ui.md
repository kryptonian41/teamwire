# ui report, team fixes3

## Progress

- 17fceae add problemReasons and its tests in teams.test.ts | left: hasProblem counts reasons, card shows reasons
- df8fd9b hasProblem counts problemReasons; sort test's quiet team clears unread and questions (D9) | left: card shows reasons
- 5a002b0 TeamRow renders problemReasons as flags; Overview.test.ts checks each card | left: final checks and gate
- c66ed52 F1 (D-L4): problemReasons skips unread rows whose `to` has `@`; test for a cross-team row alone | left: F2, F3, F5
- b04844d F2+F3: card flags skip the liveness label the meta row already shows; card test for AFK spent and a single lead dead | left: F5 report wording, final checks

## Result

Branch `fixes3/ui`, three commits on e78c32be. All files changed are in my owns list.

- `problemReasons(team)` in `ui/src/web/model/teams.ts` returns the five labels in the brief's order. Lead, watch and AFK labels need `status.state === "open"`. The 600 s threshold is the constant `QUESTION_WAIT_S`.
- `hasProblem` keeps every old check and adds `|| problemReasons(t).length > 0`.
- `TeamRow` in `Overview.tsx` renders each reason as `<span className="flag">` inside one `<p className="chips">`, before the stuck-delivery flags.

## Tests added, and what each did before its fix

| Test | File | Before the fix |
| --- | --- | --- |
| gives each problem reason its own label | teams.test.ts | failed: `Export named 'problemReasons' not found` |
| lists all problem reasons in a fixed order (also closing/closed/null) | teams.test.ts | same load error |
| counts a question only to the lead and only after 600 s (599/600, api, lead@beta, [lead, api]) | teams.test.ts | same load error |
| a lead that is not failing gives no liveness reason (working, idle, alive unknown, "unknown", null) | teams.test.ts | same load error |
| problem reasons on the fixtures (payments, docs, lost) | teams.test.ts | same load error |
| a team whose only signal is a reason is a problem and sorts first | teams.test.ts | failed: `Expected: true, Received: false` |
| each card shows its own team's problem reasons and no other team's | Overview.test.ts | failed: payments card `shown` expected true, received false |

After extending `hasProblem`, the old test `sorts problem teams first, then by title` failed as the brief predicted. D9 fixed it: line 42 now clears `unread_deliveries` and `unanswered_questions`. Its expected value is unchanged.

## Commands run on the last commit

- `cd ui && bun test` : 170 pass, 3 skip, 0 fail. The 3 skips need `TW_UI_LIVE=1` (`ui/src/shared/teamwire.test.ts:61`) and skip on e78c32be too.
- `cd ui && bun run typecheck` : `tsc --noEmit`, no errors.
- `TEAMWIRE_BASE=e78c32be scripts/gate.sh fast` from the worktree root: fmt ok, tidy ok, clippy ok, exit 0. No WEAKENED and no PROTECTED-CHANGED line.

## Reprompt m003: the closer's findings F1, F2, F3 and F5

| Finding | Holds? | What I did |
| --- | --- | --- |
| F1: a cross-team unread row flags the sender's team forever | yes | Fixed in c66ed52 as the lead decided in D-L4. `problemReasons` counts only unread rows whose `to` has no `@`, with a one-line comment on why. New test "counts an unread delivery only to a member of this team": a `db@beta` row alone gives no reason, also when the team is closed, and `db@beta` plus `web` gives `1 unread`. Before the fix it failed (`Expected - 1, Received + 3`). |
| F2: "lead dead" shows twice on a card | yes | Fixed in b04844d. The card's flags row leaves out the reason equal to `livenessInfo(...).label`, because the meta row already shows that label with its coloured dot. `problemReasons` itself still returns the label, so `hasProblem` and the model tests are unchanged. |
| F3: the card test covers 3 of 5 labels | yes | Fixed in b04844d. New card test "a card shows the AFK reason, and the lead's liveness only once" renders payments with a dead lead and a spent AFK budget. It checks `>AFK spent<` on the payments card and not on docs, and `>lead dead<` exactly once on the payments card. Before the F2 fix it failed (`Expected: 1, Received: 2`). The lead label now has a card test that fails without the F2 fix. |
| F5: the report left out the 3 skipped tests | yes | Corrected the "Commands run" line above to 170 pass, 3 skip, 0 fail. |

Commands on the last commit, b04844d:
- `cd ui && bun test`: 172 pass, 3 skip, 0 fail. The skips are the same 3 `TW_UI_LIVE=1` tests.
- `cd ui && bun run typecheck`: `tsc --noEmit`, no errors.
- `TEAMWIRE_BASE=e78c32be scripts/gate.sh fast`: fmt ok, tidy ok, clippy ok, exit 0. No WEAKENED and no PROTECTED-CHANGED line.

## Decisions I made

- The per-card overview test checks every reason against every card in one loop. A wrong card or a missing reason both fail it, and the failure message names the team and the reason.
- The new tests in teams.test.ts build inputs from one `calm` status: the payments fixture with `unread_deliveries` and `unanswered_questions` cleared. Payments' lead is `working`, its watch `running`, its afk absent, so `calm` gives no reasons.
- F2: I removed the duplicate from the card's flags row, not from the meta row. The meta row shows the lead's liveness for every team, healthy or not, so it stays. The flags row drops only the one string equal to the liveness label. No other reason can equal it, because every other label starts with a number, `watch` or `AFK`.
