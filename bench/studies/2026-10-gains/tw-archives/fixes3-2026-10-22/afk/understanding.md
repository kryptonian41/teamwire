# Understanding: three fixes in teamwire (team fixes3)

Base: branch `bench`, commit e78c32be. The three fixes share no file.

## Fix 1. The Stop hook's final-message fallback (crate teamwire-report)

End state: the fallback reads the transcript only when the file is a regular file of at most 64 MiB. The usage reader already uses that rule (`crates/teamwire-report/src/usage.rs:45-49`). The fallback (`crates/teamwire-report/src/final_message.rs:25`) calls `std::fs::read` with no check. Both readers use one shared rule.

Acceptance criteria:
- A transcript that is a named pipe: `on_turn_end` returns, the result file says `Final message from: none`, and the completion's `final_source` is `FinalSource::None`. On `bench` the hook blocks until something opens the pipe for writing. A probe test timed out after 10 s.
- A transcript of 64 MiB + 1 byte with a valid assistant line: the same outcome. On `bench` the probe got `Final message from: transcript`.
- A transcript of exactly 64 MiB is still read ("at most").
- The usage reader's behaviour and its 5 unit tests are unchanged.

## Fix 2. Resend to a team that is closing or closed (crate teamwire-msg)

End state: when the receiving team is not open, the code that delivers to another team (`deliver_to_team`, `crates/teamwire-msg/src/remote.rs:64-71`) reads that team's stored delivery row for each recipient instead of marking every one undelivered.

| Row in the receiving team | Resend reports | Sender's row becomes | Typed |
| --- | --- | --- | --- |
| `handed_off` or `acked` (read) | `HandedOff` | `handed_off` | nothing |
| `pending` | `Unconfirmed`, reason `TeamClosed` | `unconfirmed` | nothing |
| no copy, or no database | `Undelivered`, reason `TeamClosed` (as today) | `undelivered` | nothing |

On `bench` the first two rows also report `Undelivered` with `TeamClosed`. Probe tests showed this for a handed-off copy, a read copy and a pending copy.

## Fix 3. Why a team needs a look (ui/)

End state: `problemReasons(team)` in `ui/src/web/model/teams.ts` returns these labels in this order.
1. The lead's liveness label (`lead dead`, `lead gone`, `lead blocked`), only when the team is open.
2. `watch stale`, only when the team is open.
3. `AFK spent`, only when the team is open.
4. `<n> unread`, at any state.
5. `<n> question waiting`, at any state.

`hasProblem` is also true when the list is not empty. The overview card shows each reason. No zod schema changes, because every field already exists in `ui/src/shared/teamwire.ts`.

## Out of scope

- The team detail page, the server, `src/shared/`, `fixtures/` and CSS.
- `send`'s refusal for a closed team (`refused.recipient_team_closed`), which is unchanged.
- The race where a file becomes a pipe between the check and the read. The usage reader has the same race. The task asks for the same rule, not a stronger one.
- The watch's delivery path, and any reading of Codex transcripts.

## Ambiguities and the reading I chose (details in decisions.md)

- The rule's home: one shared reader function in `usage.rs`, called by both readers (D3).
- A receiving-team row that is `unconfirmed` reports like `pending`. A row that is `undelivered` reports like a missing copy (D4).
- The change goes in `deliver_to_team`, so a `send` retried with the same client key gets it too (D5).
- The label stays `<n> question waiting` for any n, word for word from the task (D6).
- "A question to the lead" means one whose `to` list holds exactly `lead`. "Open" means `status.state === "open"`. A `null` state counts as not open (D7).
