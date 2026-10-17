# ADR 0021: A message the typed line carries whole is not unread

Status: accepted, 2026-10-17. Supersedes, for such a message, ADR 0003
point 5's sentence "No state means read except `acked`" as it applies to the
unread lists, and the line format of ADR 0003 point 6.

## Context

The typed line carries the first 300 printable characters of a message body
and points at the rest with `Full text: teamwire inbox` (ADR 0003 point 6).
Only `teamwire inbox` or `ack` set `acked`, and the watch, `teamwire status`
and a member's hooks count every `handed_off` delivery as unread.

A body of 300 characters or fewer reaches the member whole in the typed line.
A member then acts on it and has no reason to open its inbox, so the
delivery stays `handed_off`. In a real run (team `openitems`, 2026-10-16), a
285-character answer stayed unread for about 40 minutes after the member had
acted on it. The watch printed `UNREAD`, and the delivery became `acked` only
when the member opened its inbox for a later message.

## Decision

1. **The line says when it holds the whole body.** When the body, folded to
   one line, fits in the 300-character excerpt, the typed line ends the body
   with a full stop and has no `... Full text:` pointer. A longer body keeps
   the pointer, to `teamwire inbox` for a member and to the stored copy for
   the lead. `line_holds_whole` in `teamwire-deliver` decides both cases.
2. **A `handed_off` message whose line held the whole body is not unread.**
   The watch's `UNREAD` line, the unread deliveries in `teamwire status`, and
   a member's PostToolUse reminder and Stop hold all leave such a message
   out. The host took a line that held every character, so the member has
   nothing left to fetch.
3. **`acked` keeps its meaning.** The state still says only that the
   recipient read the message through `inbox` or `ack`, so `delivery.acked`
   and `teamwire.messages.unread` measure what they measured before. A short
   message that is `pending`, `unconfirmed` or `undelivered` is still
   reminded, because no line reached the member.

## Consequences

- A short message produces no false `UNREAD`, no reminder and no held stop.
- A member may still run `teamwire inbox` for a short message. That sets
  `acked` as before.
- A delivery whose typed line Claude Code did not show (for example a
  terminal cleared in the same instant) is not caught for a short message.
  ADR 0003 point 8 covered a member that was working when the line arrived.
  Claude Code queues typed input during a turn and shows it after, so a busy
  member still receives the line.
