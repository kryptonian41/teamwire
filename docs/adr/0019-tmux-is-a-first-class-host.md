# ADR 0019: tmux is a first-class host

Status: accepted, 2026-10-16. Extends ADR 0003 points 5 and 7, ADR 0006
point 3a, and ADR 0013, and supersedes the sentences of ADR 0004 and ADR 0006
that name a tmux place by its bare `%<n>` id.

## Context

A live run of a real team led from inside tmux (2026-10-16) found that tmux
worked in tests and failed in use:

- Every result and question typed to a lead that ran `teamwire watch` under
  `Monitor` was refused as `not_at_input_prompt` (6 of 6). Claude Code's
  footer then reads `auto mode on · 1 monitor`, which the screen check's fixed
  phrase list did not know. herdr never reads the screen, because it refuses
  to type into a dialog itself.
- `teamwire close` refused every live tmux member, because tmux reports no
  busy state and close refused an unknown state even with `--stop`.
- A command run outside the lead's tmux asked the default tmux server, found
  no panes, and treated every member as gone.
- tmux gives no proof that a submit was taken.

## Decision

1. **The screen check reads the input box's shape.** A pointer line with a
   drawn rule under it and a footer line under the rule is the idle input box,
   whatever the footer says. The old footer phrases still count. Dialog
   markers are checked first and a dialog is still refused at once. A screen
   that shows `esc to interrupt` is not idle by the shape rule.
2. **An unclear screen is read again.** On a host that does not guard dialogs,
   a screen read of `Unknown` is repeated up to 3 times, about 750 ms in all,
   inside the delivery budget, before delivery is refused as
   `not_at_input_prompt`.
3. **One more Enter where the host gives no acknowledgement.** On a host whose
   `submit_ack` is `none` (tmux), teamwire reads the screen once after the
   submit. When the line it typed still sits in the input box, it presses
   Enter once more. It never types the text again, so ADR 0003 point 7 holds:
   the text is never sent twice. `handed_off` on such a host means the host
   took the text and teamwire looked once.
4. **A tmux place names its server.** A place is `%<n>@<socket path>`, and
   every call for that place goes to that server with `tmux -S`. The adapter
   takes its default server from the lead's `TMUX` value. A bare `%<n>` from an
   older team still goes to the adapter's default server.
5. **Close asks the harness when the host cannot tell.** When the host reports
   no state, close uses the harness's session status for busy, idle or blocked.
   `close --role R --stop` also removes a member whose state is still unknown,
   and says so. A whole-team close still refuses an unknown state.
6. **A live check for tmux.** `crates/teamwire-host-tmux/examples/tmux_live_check.rs`
   is run by hand against a private socket, like herdr's.

## Consequences

- A lead located from a shell outside tmux still uses the bare `TMUX_PANE` its
  process holds, and so the default server. Members started by this build are
  found wherever the command runs.
- The "line still in the box" test lives in `teamwire-deliver` and knows
  Claude's pointer glyphs. A `Harness` method should own it before a second
  harness is added.
