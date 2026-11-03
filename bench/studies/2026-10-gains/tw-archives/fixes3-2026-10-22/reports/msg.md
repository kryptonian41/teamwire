# msg report, team fixes3

## Progress

- b4e7570 resend reads the closing team's row; handed_off/acked mirror as handed_off | left: pending/unconfirmed/undelivered arms, docs
- 77c9b40 pending/unconfirmed there mirror as unconfirmed with TeamClosed; undelivered stays undelivered (D-L2 comment) | left: docs
- 38a272a docs/api/cli.md (resend) and docs/api/mcp.md (team_resend) say what a resend does for a closing or closed team; remote.rs module comment updated | left: none

## What changed

`deliver_to_team` in `crates/teamwire-msg/src/remote.rs`, in its `Ok(None)` branch (the recipient team is not open), now calls a new `held_there`. That function reads the receiving team's row with `there.store.read(|tx| tx.delivery(&message.ulid, &local))` and maps it:

| Row there | Outcome passed to `mirror` |
| --- | --- |
| `handed_off`, `acked` | `outcome_of` (acked becomes handed_off) |
| `pending`, `unconfirmed` | `Unconfirmed`, row's route kept, `TeamClosed` "team X is closing or closed; an earlier attempt may have reached it" |
| `undelivered` | `gone(team)`, with the D-L2 comment |
| no row | `gone(team)` |

No write goes into the receiving team. The `NoDatabase` branch is unchanged (still `gone`). No `Pending` outcome reaches `mirror` on this path.

## Tests added (crates/teamwire-msg/tests/cross_team.rs)

Helpers: `copy_in_beta` (beta's copy with db's row in a given state; `ack_delivery` for acked) and `resend_after_beta_closes` (stored_in_alpha_only, copy in beta, begin_close and optionally finish_close Completed, advance 61 s, resend as api).

| Test | On base e78c32be | After |
| --- | --- | --- |
| a_copy_handed_off_in_a_closing_team_is_recorded_handed_off | FAIL `(Undelivered, Some(TeamClosed), Undelivered, 0)` | pass |
| a_copy_read_in_a_closed_team_is_recorded_handed_off | FAIL `(Undelivered, Some(TeamClosed), Undelivered, 0)` | pass |
| a_pending_copy_in_a_closing_team_is_recorded_unconfirmed | FAIL `(Undelivered, Some(TeamClosed), Undelivered, 0)` | pass |
| an_unconfirmed_copy_in_a_closing_team_stays_unconfirmed | FAIL `(Undelivered, Some(TeamClosed), Undelivered, 0)` | pass |
| a_copy_that_surely_never_arrived_stays_undelivered_when_its_team_closes | pass (D4 keeps today's result) | pass |

The existing `a_team_that_closed_after_the_send_was_stored_leaves_it_undelivered` is unchanged and passes (the diff of cross_team.rs is 127 additions, 0 deletions).

## Commands run

- `cargo test -p teamwire-msg --locked --test cross_team`: 17 passed.
- `cargo test -p teamwire-cli --locked --test preset`: 10 passed.
- `cargo test -p teamwire-msg --locked`: every target ok, 0 failed.
- `TEAMWIRE_BASE=e78c32be scripts/gate.sh fast`: exit 0; fmt ok, tidy ok, clippy ok; no WEAKENED or PROTECTED-CHANGED line.

## Decisions I made

- Test names for the four tests the brief did not name: listed in the table above.
- `held_there` matches on every `DeliveryState` variant by name (no guard arms, no wildcard), so a new state forces a decision here.
- The docs sentences name the outcome states and the reason code `team_closed` (the string `DeliveryErrorCode::TeamClosed` serializes to, core message.rs:137).
- The remote.rs module comment gained one sentence: a closing or closed team is only read, and its row decides the sender's row.
