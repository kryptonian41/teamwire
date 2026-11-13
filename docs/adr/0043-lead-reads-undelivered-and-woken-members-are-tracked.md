# ADR 0043: The lead reads what was never typed, and a woken member is tracked

Status: accepted, 2026-11-13. Supersedes the points named under
"Superseded" below, and only those. The older ADRs keep their words as
history.

## Context

Team deckh's live run on 2026-11-10 lost about 90 minutes to three faults
that share one cause. The watch and `status` work out a member's state from
its newest completion in its current generation, and only a `reprompt` or a
handoff starts a generation. teamwire recorded neither of two events:

- a turn that a message other than a reprompt starts in a finished session;
- a message that the lead read from its file without having it typed.

So a question or a result that met a blocked lead pane stayed
`undelivered` after the lead had read it, and the only way to clear it typed
the old message again (F1). A member whose result was not delivered, and
which was working again, counted as settled, so the watch printed
`TEAM-COMPLETE` and exited while the member worked (F2). A message that woke
a finished member started work that no generation tracked, and the lead
could then no longer hand the member off (F3). The root causes, with file
and line on `develop` (`eda2e0b`), are in
`specs/001-deckh-fixes/research.md`.

## Decisions

1. **The lead's hooks list what was never typed (F1-a).** At each lead tool
   call and at the lead's turn end, the lead hook adds one line per item to
   the lead's turn: each delivery to `lead` in state `undelivered`, and each
   completion whose report state is `Undelivered`. A line names the item,
   the file to read, and the `teamwire received` command that clears it,
   and holds the phrase `was never typed into your pane`. A result line
   names the same file as the watch's `Result:` path: the result file, or
   the member's report when the result file is missing or no longer holds
   the recorded text. That rule lives in `teamwire-deliver` so that the
   watch and the lead hook share it. The hooks add text and type nothing.
   The note is computed at each call and never stored. It ships for Claude
   Code, Codex and pi, through the shared hook entry points. An
   `unconfirmed` row may have been typed, so the note leaves it out;
   `teamwire status` lists it.
2. **`teamwire received` takes message ids (F1-b).** An argument is a
   message id only when it is `m` followed by digits and no role of the
   team has that name. When a role has that name too, the command refuses
   it and names both. For a message id, the command moves the lead's
   delivery row from `undelivered`, `unconfirmed` or `pending` to `acked`
   in one store write, and types nothing. A row already `handed_off` or
   `acked` stays as it is, and the command says `already delivered` or
   `already read`. The MCP server has no `received` tool, so the lead's MCP
   tools do not change.
3. **An answer settles its question (F1-c).** An `answer` with `re = q`
   acks the delivery row of `q` for the answer's sender, in the same write
   as the answer, when that row exists.
4. **The Claude Code lead tool hook runs on every tool call (F1-d).** The
   plugin's `PostToolUse` entry has no matcher. The answer to an
   `AskUserQuestion` dialog is the call that frees a blocked pane, and an
   edit-and-shell matcher never ran the hook for it. Codex and pi already
   ran the hook on every call. The note about member work still runs only
   for calls that are member work.
5. **The watch's question lines name the file (F1-e).**
   `QUESTION-UNDELIVERED` and `QUESTION-UNCONFIRMED` end with
   ` File: <team dir>/comms/<id>.md`.
6. **A member does not resend to the lead (F1-f).** The lead's hooks list
   every message to the lead that was never typed, so a resend would type a
   second copy. The member's seat text keeps "do not send it again" and no
   longer tells a member to retry a message to the lead with `resend`. The
   line `send` prints when the lead's copy of a user's message was not
   delivered names no `resend` either. For an `undelivered` copy it says the
   lead's hook lists it at the lead's next tool call. For an `unconfirmed` or
   `pending` copy, which may have been typed, it says `teamwire status`
   lists it. The line still starts `lead copy not delivered:`, which the
   dashboard matches.
7. **The watch keeps a member whose session works (F2).** For a newest
   completion whose report state is `Undelivered` or `Unconfirmed`, the
   watch still announces it once, then looks at the session:

   | The session | The member |
   | --- | --- |
   | working, or at a dialog mid-turn | not settled; `TEAM-COMPLETE` waits |
   | idle | settled, as before |
   | died | settled, as before |
   | not visible to the host | settled, and the line says `session not visible` |

   `teamwire status` shows such a member as `working`, or
   `working (at a dialog)` when its session waits at a dialog. The
   dashboard sorts `working (at a dialog)` as active.
8. **A waking message starts a generation (F3-a).** A message that is not a
   reprompt, to a member whose newest completion is in its current
   generation (`done`, `blocked` or `failed`) and whose summary does not
   start with `handoff:`, bumps the member's generation in the same write as
   the message. The completion's turn must have ended: a completion still
   `pending` was written mid-turn by `done`, and its turn-end hook delivers
   it from the current generation. So a message before that turn ends is a
   plain message in the current generation. A message from another team
   follows the same rule, and the hold of decision 11, in the recipient's
   team, when its copy there is stored. The new generation ends when the member records `done`, like
   any generation. A known delivery failure undoes the bump, and an
   `unconfirmed` delivery keeps it, as for a reprompt (REP-3).
9. **A woken generation is its own event type (F3-f).** The bump writes
   `member.woken`, with `generation` and `message` in its data, and an undo
   writes `member.woken` with `undone: true`. `member.reprompted` stays for
   reprompts. Every reader that takes `member.reprompted` as the start of a
   generation also takes `member.woken`: the watch's check for a prompt
   typed outside teamwire, the message crate's interrupted-reprompt check,
   and the metrics' generation span and undo stack. The metrics' reprompt
   count does not count `member.woken`, so a member answered twice does not
   read as one that needed reprompts.
10. **Schema version 11.** The store's list of event types is a closed
    `CHECK`, so version 11 rebuilds `events` with `member.woken` in the
    list, exactly as version 10 did. The upgrade only goes forward: a store
    the new build has opened cannot be opened by an older build, as ADR 0038
    says of version 10. The user installs the new build only after team
    `deckh` closes, so no running team's store is upgraded under it.
11. **A message to a member that asked for a handoff is held (F3-b).** When
    the member's newest completion in its current generation has a
    `handoff:` summary, the message's delivery row is written `undelivered`
    with the error code `held_for_handoff`. Nothing is typed, and the old
    session's own reminder and Stop hold never name the row. `send` prints
    `<role>: held for handoff (<id>)` and exits 0. `resend` refuses a held
    row and names `handoff` and `reprompt` (F3-e). `teamwire status` lists
    each held row on its own line, `held for handoff: <id> -> <role>`, and
    not under deliveries that need attention. `status --json` carries the
    rows in `held_deliveries`, and the dashboard does not show them as
    needing a look.
12. **The handoff or a reprompt passes held messages on (F3-c, F3-d).**
    `teamwire handoff` already counts `undelivered` rows as unread, so the
    fresh session's first line tells it to read them. A reprompt to a member
    with held rows types one more sentence after its own text:
    `Held for you, oldest first: <id> (<type> from <sender>), .... Read them with teamwire inbox.`
    The reprompt's bump starts a new generation, which ends the hold.

## Superseded

- **WAT-6** (`docs/requirements.md:177`) and **ADR 0007 point 6**
  (`docs/adr/0007-completion-reporting.md:91-93`): an `UNDELIVERED` or
  `UNCONFIRMED` result no longer settles a member whose session works or
  waits at a dialog (decision 7).
- **INV-14** (`docs/requirements.md:57`) and **REP-3**
  (`docs/requirements.md:129`): "a reprompt starts a new generation" is
  widened to a waking message (decision 8). Any other ADR point or
  requirement that says only a reprompt starts a generation is widened the
  same way.
- **ADR 0024 point 2** (`docs/adr/0024-the-lead-marks-a-result-received.md:32-37`):
  `received` takes message ids as well as roles (decision 2).
- **ADR 0035 point 1** (`docs/adr/0035-the-lead-tool-hook-notes-member-work.md:41-49`):
  the Claude Code lead tool hook runs for every tool, not only `Edit`,
  `Write`, `MultiEdit`, `NotebookEdit` and `Bash` (decision 4).
- **ADR 0040 point 5**, its hint sentence only
  (`docs/adr/0040-user-sender.md:86-87`, `run teamwire resend <team> <id> --to lead`):
  the hint names the lead's hook or `teamwire status` instead (decision 6).

## What stays

- **ADR 0003 point 7 and INV-8.** No process types a message after the
  command that sent it has exited. The watch does not retry a delivery
  once the lead's pane is free. The lead's hooks add text to the lead's
  own turn, which types nothing into any pane.
- INV-5: a message that reaches no one is never silent. The note adds a
  second place that reports it.

## Alternatives rejected

- The watch retries once the pane is free. That would supersede ADR 0003
  point 7 and INV-8, and it is the old double-typing bug.
- Only a plain watch line. The lead would still read files by hand, and a
  watch that has ended says nothing.
- Hold every message to a finished member. Other members' questions would
  wait for the lead.
- Count the woken turn as working without a generation. A handoff would
  still refuse the member, and its next `done` would stay in the old
  generation.

## Consequences

- `docs/requirements.md`, `docs/failure-modes.md`, `docs/api/cli.md`,
  `docs/api/core.md`, `docs/metrics.md`, `docs/ARCHITECTURE.md`, the
  README, the lead skill and its reference, the handoff guide and the seat
  text state these rules, each in the commit that changes the code.
- `ui/src/shared/teamwire.ts` gains `member.woken`, `held_deliveries` and
  the `working (at a dialog)` word.
- A member answered by the lead after its `done` now needs a new `done`
  before the team completes. That is the point of the change: its woken
  turn is tracked.
