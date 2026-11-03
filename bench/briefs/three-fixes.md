# Task: three fixes in teamwire

Make these three independent changes. Each one is finished when its behaviour below is true and tested.

## 1. The Stop hook never opens a named pipe or a huge file for the final message (crate `teamwire-report`)

When a member's turn ends and the Stop hook input carries no final message, the hook falls back to the last assistant text in the session's transcript file. The usage reader in the same crate already refuses a transcript that is not a regular file or is larger than 64 MiB (`MAX_TRANSCRIPT_BYTES`), but the final-message fallback reads the file with no check. A named pipe there blocks the hook forever.

End state: the final-message fallback reads the transcript only when it is a regular file of at most 64 MiB, with the same rule as the usage reader. For a named pipe or a file past the limit, the hook still completes through `on_turn_end`, the result file says `Final message from: none`, and the recorded completion's `final_source` is `FinalSource::None`.

## 2. A resend to a team that is closing keeps what that team already holds (crate `teamwire-msg`)

`teamwire resend` retries a stored message. For a recipient in another team (`db@beta`) whose team is no longer open, the resend now records the delivery as undelivered without looking. But the other team may already hold a copy of the message from an earlier attempt.

End state: when the receiving team is closing or closed, `resend` reads that team's stored delivery row for the recipient. A row there that is handed off or read keeps that outcome: the resend reports `HandedOff`, the sender's own row for `db@beta` becomes handed off, and nothing is typed. A row still pending reports `Unconfirmed` with reason `DeliveryErrorCode::TeamClosed`, and the sender's row becomes unconfirmed. Only a missing copy is recorded undelivered, as today.

## 3. The dashboard says why a team needs a look (`ui/`)

In `ui/src/web/model/teams.ts`, `hasProblem(team)` decides which teams sort first under "needs a look". It misses five signals that need the lead.

End state: a new exported function `problemReasons(team: TeamModel): string[]` returns one label per signal, in this order:

| Signal | Label | Only while the team is open |
| --- | --- | --- |
| The lead's liveness category is failed or attention | the lead's liveness label from `livenessInfo`, such as `lead dead` | yes |
| The watch state is `stale` | `watch stale` | yes |
| The AFK budget is spent (`afk.spent`) | `AFK spent` | yes |
| Unread deliveries exist | `<n> unread` | no |
| Questions to the lead waiting 600 s or more | `<n> question waiting` | no |

A question to another member, or one under 600 s old, does not count. `hasProblem` is also true whenever `problemReasons` returns anything. The overview card shows the reasons.

## How to work and finish

- The repository is teamwire, a Rust tool, with a Bun and TypeScript dashboard in `ui/`. Read `AGENTS.md` first, and `ui/AGENTS.md` for the dashboard.
- Add a test for each change, in the crate's `tests/` folder for Rust and beside the file as `<name>.test.ts` for the dashboard.
- Check your work with the tests of each crate you changed (`cargo test -p <crate> --locked`), `bun test` in `ui/` if you changed it, and `TEAMWIRE_BASE=e78c32be scripts/gate.sh fast` from the repository root. This checkout has no `main` branch, so the gate needs that variable.
- Commit the finished work in this repository. Do not push; there is no remote. End your final message with the name of the branch that holds the finished work.
