# ADR 0032: The handoff order lives in launch

Status: accepted, 2026-10-30. Supersedes the last sub-point of ADR 0027
point 5, which put the order of a handoff's steps in `cmd/handoff.rs`. The
rest of ADR 0027 stands.

## Context

A handoff runs its steps in a fixed order: launch's checks, close's checks
(a running turn, a dialog, an unknown state, uncommitted paths), close's
place close with its wait for the old processes, then launch's open, trust
step, record and first line. After a failure past the spawn, it closes the
new tab and writes the old session id back. `teamwire-launch` and
`teamwire-close` are both layer 4 and may not depend on each other
(`docs/conventions.md` section 5), so ADR 0027 put the order in the CLI.

There the order had no test that used fakes. The CLI tests start the binary,
and a library test could reach each half but not the order between them.
HOF-1's stop before the spawn had no test of its own. The handoff refusals
also copied the wording of close's busy and uncommitted-work refusals, with
different advice at the end.

## Decision

1. **`teamwire_launch::hand_off` runs the steps in order.** It owns the
   refusals, the partial-failure recovery and the result. Its error is
   `HandoffError`: `Refused` and `Store` touched nothing, and `Partial` comes
   after the old place's close began. It reports each step to a callback as
   a `Said::Line` or a `Said::Warning`, so the CLI still prints each line
   when the step is done.
2. **The close steps are injected.** `PlaceCloser` holds two function
   pointers, the checks and the place close. `cmd/handoff.rs` fills them with
   `teamwire_close::handoff_refusals` and `close_handoff_place`, prints what
   `hand_off` says, and maps its error to exit codes 1, 2 and 6. No layer-4
   crate depends on another.
3. **The refusal wording is written once.** Close's `busy_text` and
   `uncommitted_text` take the advice that ends the refusal. A removal passes
   `--stop` or "commit or copy that work out", and a handoff passes "run
   handoff again" or "the member commits its work". The texts the user sees
   do not change.

Beaten: a trait object for the closer. Close's two functions need no state,
so plain function pointers are enough.

Beaten: returning the lines at the end. A failed handoff would then print
nothing of the steps it finished, and the user would wait through the
place-close wait with no output.

## Consequences

`crates/teamwire-launch/tests/handoff_order.rs` covers the order with fakes:
a refusal stops before any close, a failed place close stops before the
spawn, a failure after the spawn closes the new tab and restores the old
session id, and a handoff that succeeds closes first, then opens and records
it.
