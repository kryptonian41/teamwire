# ADR 0023: The lead marks a result received, and a newer result replaces an older one

Status: accepted, 2026-10-18. Adds a way out of `undelivered` and
`unconfirmed` to ADR 0007 point 2, and narrows which results `teamwire status`
lists as not received.

## Context

ADR 0007 point 2 gives each completion a report state. A result whose typing
failed becomes `undelivered`, and a result whose typing stalled becomes
`unconfirmed`. teamwire never types a result again by itself, so the lead
reads the file the watch line or `reopen` points at. No command recorded that
the lead had read the file. `teamwire status` listed every completion in
either state, of any age, for as long as the team existed, and the dashboard
showed the same list.

In team `uiwave2` (2026-10-17), the lead sat at a permission dialog when
`mepath` and `bemejit` finished, and both results ended `undelivered` with
`session_blocked`. After `reopen`, the user prompted the lead by hand to read
both files. `status` still listed both results a day later. `bemejit` had
since been reprompted and was working again, so its listed result was no
longer its newest one.

## Decision

1. **Only a member's newest completion can be listed as not received.**
   `teamwire status` and `reopen` leave out an `undelivered` or `unconfirmed`
   completion when the member has a newer one. ADR 0007 point 2 already
   delivers only the newest completion. The result file `outbox/<role>.md`
   also holds only the newest result, so the older listing pointed at a file
   that no longer held it.
2. **`teamwire received <team> <ROLE>...` records that the lead read a
   result.** For each role, it moves the member's newest completion from
   `undelivered` or `unconfirmed` to `delivered`, with route `inbox`, and
   writes one `result.delivered` event. Route `inbox` already means "the
   recipient read it on request", so the schema does not change. The stored
   reason stays, as the record of why the typing failed.
3. **The command is for the lead.** A caller that carries a member identity
   is refused, as for `reopen`. Any other caller may run it, because the user
   often runs it from a terminal outside the lead's session.
4. **The command changes nothing for any other state.** A `pending` or
   `delivering` result may still be typed by the hook, and a `delivered`
   result is already received. The command prints the state and exits 0, so
   running it twice is harmless. An unknown role is refused with exit 1.

## Consequences

- A result the lead read stops showing in `status` and on the dashboard.
- The metrics count such a result as delivered by route `inbox`, and its
  `seconds` measure the time until the lead said it read the file.
- teamwire takes the lead's word. The command does not check that the lead
  opened the file.
- An older `undelivered` completion keeps its state in the store. Only the
  listing leaves it out, so `teamwire export` still shows it.
