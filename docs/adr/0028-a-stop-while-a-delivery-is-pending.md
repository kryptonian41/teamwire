# ADR 0028: A stop while a delivery is pending makes its sender try again

Status: accepted, 2026-10-27. Adds to ADR 0003 point 8; supersedes nothing.

## Context

A sender stores a message with one delivery row per recipient in state
`pending`, then types a line into each recipient's pane, outside any
transaction, and records the outcome (ADR 0003 point 1). When a member's turn
ends, its Stop hook names the unread messages in one hold. It leaves out a
fresh `pending` row, because the sender may still type that line, and a
message both typed and named in a hold reaches the member twice (ADR 0003
point 8). A `pending` row counts only once it is over 60 seconds old, which
is a sender that crashed.

Each side relied on the other, and nothing recorded that either had. A member
whose turn ended while a sender was still trying to reach it skipped the
message. That sender then found the pane refusing input (a dialog, text left
in the input box, a screen it could not read) and recorded `undelivered`.
The member was idle by then, so no later stop came to name the message. It
stayed unread until the lead noticed it in `status` and ran `resend`.

The end-to-end lifecycle test hit this in about half of its runs on Linux:
two members sent to each other at once, each found the other's pane refusing
input, and the first to finish skipped the other's message.

## Decision

1. **A stop marks what it left to a sender.** At every turn end of a member,
   including the stop after a hold and not a subagent's stop, the hook writes
   one `delivery.recipient_stopped` event per fresh `pending` delivery to the
   member (`data.to`, subject the message). The list and the marks are one
   `Store::write`. A stale `pending` row is not marked: the hold names it as
   before. A row whose age cannot be read is marked.

2. **A sender reads the marks before it records `undelivered`.** In the same
   transaction that would write `undelivered`, the sender counts the
   `delivery.recipient_stopped` events for the recipient since the latest
   attempt began (`message.sent`, or the latest `delivery.resent`), in commit
   order. If there are more than it has seen, it writes nothing and makes
   one more attempt. Other outcomes are recorded as before.

3. **The tries are bounded.** One command adds at most 3 such tries. A
   recipient that stops during each try was held for one more turn each time;
   once the row is `undelivered`, its next stop names the message.

Every `Store::write` runs under `BEGIN IMMEDIATE`, so the hook's write and the
sender's outcome write never overlap. If the sender commits `undelivered`
first, the row is no longer `pending`, and the hook's hold names it. If the
hook commits first, the sender sees the mark and tries again while the member
is idle. No order leaves the message unnamed, and none names a message that a
sender is about to type, so point 8 still holds.

## Consequences

- Schema version 8 rebuilds `events` with `delivery.recipient_stopped` in its
  CHECK list, as versions 4, 6 and 7 did for their types.
- Every turn end of a member makes one more short write, which is empty when
  nothing is pending.
- A send to a member whose turn ends meanwhile can take up to three more
  attempts, each up to about a second of screen reads.
- The lead's own Stop hook does not mark. A message to the lead that its
  sender records `undelivered` is still shown by the watch and `status`.
- `teamwire_otel` makes no metric or span of the new event, as for
  `delivery.reminded`.
