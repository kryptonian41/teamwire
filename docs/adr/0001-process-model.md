# ADR 0001: Process model — one binary, no daemon

Status: accepted, 2026-09-29.

## Context

teamwire replaces seven Python scripts and one shell hook. Each of them runs,
does one job and exits, except the watchdog, which the lead runs as a long
process under Claude Code's `Monitor` tool. The user ranks stability first.

Three shapes were on the table (rust report, section 1.6):

| Option | On a crash | On sleep and wake | On upgrade | Two versions at once |
| --- | --- | --- | --- | --- |
| A. A long-running daemon owns the state | Every client fails while it is down; in-memory state is lost | Timers and sockets can stall on wake | Stopping it drops work in flight | Needs a leader lock |
| B. No daemon: every command opens the store, works, exits | Only that command is lost; SQLite recovers at next open | Nothing runs while asleep | Replace the binary | Safe if every open checks the schema version |
| C. B plus an optional helper process that only speeds things up | Same as B | The helper re-reads the store on wake | The old helper can keep running | Same as B |

The rust report recommends C, built as B first. No report argues for A.

## Decision

teamwire is **one binary, `teamwire`, with subcommands, and no daemon**.

- Short commands (`launch`, `send`, `done`, `status`, `verify`, `close`,
  `inbox`, `ack`, `log`, `hook ...`) open the team's store, do their work in
  transactions, and exit. None of them needs another teamwire process.
- Two subcommands run for a long time, and both are ordinary processes that
  someone else starts and owns:
  - `teamwire watch <team>`: the lead runs it (under `Monitor` in Claude
    Code). It reads the store and the host every poll and prints at most one
    line. It also runs the OpenTelemetry export when an endpoint is set.
  - `teamwire mcp`: the harness starts one per session over stdio. It is a
    thin front for the same library calls the CLI makes.
- The core library is synchronous. Only `teamwire mcp` starts a tokio runtime,
  because `rmcp` needs one. The rust report leaned this way (its open
  question 1) so that the store and core stay small and easy to test.
- Every write goes through the store library, whose transactions are the only
  place state changes (ADR 0002).

## Consequences

- A crash of any teamwire process loses at most the command in flight. Its
  effect is either committed or not.
- Nothing runs while the machine sleeps, so sleep cannot corrupt anything. The
  watch notices the gap and resets its quiet counters (WAT-14).
- Upgrading means replacing the binary. A running `watch` or `mcp` from the
  old binary keeps working until the schema moves past what it knows; then it
  refuses cleanly and exits (ADR 0002).
- There is nothing to retry delivery later. That is deliberate: a delayed
  typing into a pane is the failure INV-8 forbids. A message left `pending` by
  a crashed sender is shown by `status` and can be resumed by the sender with
  the same key (ADR 0003).
- Nobody watches the lead itself. If the lead's session dies, its watch dies
  with it. This is listed as an open question for the user.
- Hook cost matters again, because a hook starts the binary. teamwire's hooks
  are registered only for members it launched (ADR 0004), so no other session
  ever pays for them.
