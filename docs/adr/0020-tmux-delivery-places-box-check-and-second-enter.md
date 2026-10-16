# ADR 0020: tmux delivery finds places from TMUX, the harness owns the box check, and a second Enter is recorded

Status: accepted, 2026-10-16. Supersedes the first two bullets of ADR 0019's
Consequences. Extends ADR 0003 point 5, ADR 0004, and ADR 0015.

## Context

ADR 0019 left two gaps in tmux delivery. The locator read only `TMUX_PANE`
from the target's processes, so `teamwire send` from a shell outside tmux
asked the default tmux server and failed as `host_no_session`. The check that
a pasted line still sits in Claude's input box lived in `teamwire-deliver` and
copied Claude's pointer glyphs, which ADR 0004 puts behind the `Harness`
trait.

When that check fires, delivery presses Enter a second time. Before this ADR
the second Enter went only to an `info` log line, so nobody could tell how
often the paste-then-Enter race happens.

## Decision

1. **A host builds the place from a process's environment.** `Host` gains
   `place_from_env(read)`. Its default returns the value of `place_env_var`,
   so herdr is unchanged. tmux joins `TMUX_PANE` with the socket path from
   the same process's `TMUX` value, giving the same `%<n>@<socket path>` id
   that close, state and watch already use. When `TMUX` cannot be read, the
   candidate is the bare pane id, and the locator's check that the place holds
   the right session is as strict as before.
2. **The harness decides whether a line is still pending.** `Harness` gains
   `line_pending(tail, line)`, default `false`, so a harness that cannot tell
   never gets a second Enter. The Claude harness reads its own input box. A
   box that holds only Claude's collapsed-paste marker (`[Pasted text #N]`)
   counts as pending. `teamwire-deliver` keeps the timing, the dialog check
   and the key press.
3. **A second Enter is a stored event.** `delivery.reentered` records the
   recipient (`to`) and the place. `teamwire-deliver` writes the event after
   every host call has returned. The write is skipped once the delivery's time
   budget is spent. A failed write is logged and never changes the delivery
   outcome. `deliver_line` and `deliver_line_within` take `&mut Ctx` for this
   write.
4. **Schema version 4 rebuilds `events`.** SQLite cannot change a CHECK in
   place, so `schema/v4.sql` renames `events`, creates it with
   `delivery.reentered` in the type list, copies every row with its `seq`,
   restores the AUTOINCREMENT counter, recreates `events_type`, and drops the
   old table. The step and the version bump run in one `BEGIN IMMEDIATE`
   transaction, so a crash leaves version 3 whole. No column changed, so a
   read-only open of a version 3 database needs no view (ADR 0015).

## Consequences

- `teamwire send`, the turn-end hook and the watch reach a tmux agent from a
  shell outside tmux.
- `teamwire log`, `metrics` and `export` can count second Enters per team.
  The OpenTelemetry export carries the event as its `delivery.*` siblings do
  and adds no counter or rate.
- Rollout: once a version 4 binary opens a team's database for writing, an
  older binary refuses that team as schema newer. This includes a watch, a
  lead MCP server or a member hook still running from the old binary. Install
  the new binary when no team is running, or restart those processes after the
  install. ADR 0016 says the same for version 3.
- The `[Pasted text #N]` wording was not captured from a real screen. The
  marker is not tied to the line teamwire pasted, so a person's own paste in
  the 300 ms after teamwire's Enter can be sent early.
