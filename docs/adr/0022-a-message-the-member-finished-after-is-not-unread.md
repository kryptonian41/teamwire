# ADR 0022: A message the member finished after is not unread for the lead

Status: accepted, 2026-10-18. Adds a second exception, next to ADR 0021's,
to which `handed_off` deliveries the watch's `UNREAD` line and
`teamwire status` count as unread.

## Context

A message longer than the typed line's 300-character excerpt ends with
`... Full text: teamwire inbox`. A member often acts on the excerpt alone,
ignores the one PostToolUse reminder (ADR 0003 point 8), and runs
`teamwire done`. The delivery stays `handed_off` for good, and 10 minutes
later the watch prints `UNREAD`. teamwire sends no further reminder, so the
line only sends the lead to check by hand.

In team `leadres` (2026-10-18), `labs` sent `studies` a 535-character
finding. `studies` checked it, added it to its report, and ran
`teamwire done` 15 seconds later. The watch then printed
`UNREAD m001 -> studies 10m`, and the lead had to read the transcript to
learn that nothing was wrong. Across the 57 archived runs, 11 long messages
to members ended `handed_off`. In 9 of the 11, the member had recorded a
completion after the message arrived.

## Decision

1. **A completion after the line is a read, for the lead's views.** A
   `handed_off` delivery to a member is not unread when the member's newest
   completion was recorded after the delivery's latest attempt began. A
   resend starts a new attempt, so it makes the message unread again.
2. **Only the watch's `UNREAD` line and `teamwire status` use the rule.**
   `unread_since` in `teamwire-deliver` decides it, together with ADR 0021's
   rule, for both.
3. **The member's hooks keep their rule.** The PostToolUse reminder and the
   Stop hold still count such a message. A member records its completion
   before its turn ends, so the hold would otherwise never fire, and the hold
   is the one prompt to read the full text.
4. **`acked` keeps its meaning,** as ADR 0021 point 3 says.

## Consequences

- The lead no longer gets an `UNREAD` line for a message the member already
  acted on and then finished.
- A member that finished without reading a part beyond the excerpt that
  mattered is no longer flagged. The `teamwire.messages.unread` gauge still
  counts every handed-off delivery that is not `acked`.
