# ADR 0052: The lead answers a member's permission dialog

Status: accepted, 2026-11-18. Supersedes no sentence of an earlier ADR.

- ADR 0006 point 3a ("Never type into a dialog") stays true: nothing in this
  ADR types into a dialog.
- It narrows two text rules. "A permission prompt that the harness denies on
  its own after a timeout needs nothing" (`examples/skills/teamwire/reference.md`)
  and "Nobody answers that dialog" (`crates/teamwire-launch/contracts/how_to_work.md`)
  become: only the lead's `teamwire dialog answer` answers, through the
  member's own permission hook.
- It adds the dialog kind and the permission hook to the `Harness` row of
  ADR 0004.

## Context

Claude Code can stop a member's tool call at a permission dialog even with
approvals turned off, for example a removal of a folder it treats as
critical (`rmdir /usr`). The closer of commit 8dd39c10 sat two minutes at
such a dialog. Claude Code shows a countdown and denies the call on its own
at 120 s, measured in this run with Claude Code 2.1.327. Until then the
member stalls, and nobody in the team can see why.

Claude Code runs a `PermissionRequest` hook the moment it opens the dialog,
with the tool name and the whole tool input on stdin. A hook reply can allow
that one call, or deny it with a message that the model reads as the call's
result. The turn then goes on.

teamwire must never type into a dialog (ADR 0006 point 3a), and a peer
message must never approve a permission prompt (DEL-15).

## Decision

1. **A hook, not keys.** Every Claude Code member's per-run settings carry
   a `PermissionRequest` hook, `teamwire hook permission --team T --role R`,
   with `timeout` 110 s and no matcher. The hook records the call, waits up
   to 100 s for the lead's answer, and prints Claude Code's own allow or
   deny reply. With no answer it prints nothing, so Claude Code's countdown
   denies at 120 s, as without teamwire. Nothing presses a key in the
   member's pane.
2. **The dialog id is the proof.** The hook writes one `dialog.opened`
   event; its ULID is the dialog id. `teamwire dialog answer --id ID`
   records the answer only while that id is pending for the member's current
   session and generation and before its time runs out, checked and written
   in one `Store::write`. The hook reads only the answer to its own id, so
   an answer can never reach another dialog.
3. **Allow once, or deny.** A reply allows this one call or denies it. It
   never holds `updatedPermissions`, `updatedInput` or `interrupt`, so
   teamwire never writes a lasting grant, changes a call, or stops a turn.
4. **Only calls the lead can read whole.** teamwire answers only Bash, Write,
   Edit and WebFetch calls whose input is at most 64 KiB as JSON. Any other
   call is recorded `unanswerable` at once and left to Claude Code. Every text
   the lead reads about a call shows each control, invisible or
   bidirectional character as `\u{XXXX}` (`teamwire_core::shown`), so a
   command cannot hide or reorder what the lead sees.
5. **Only the proven lead session answers.** `dialog answer` refuses a
   member session, then proves the caller with the function `send --from
   lead` uses, and records a refusal as `message.refused`. The human user
   answers in the member's own pane.
6. **One watch line per dialog.** The watch prints `BLOCKED <role> permission
   <tool>: <summary> (dialog <id>, answer by <HH:MM:SSZ>)` once per recorded
   dialog, and `BLOCKED <role> <kind>` once per episode for any other dialog
   (`trust`, `login`, `update`, `unknown`, or `permission (teamwire cannot
   answer it)`), reading the screen once per episode.
7. **Three event types in schema 12.** `dialog.opened`, `dialog.answered`
   and `dialog.closed` record each dialog, answer and end. A dialog's state
   is read from its events, never stored.
8. **Every harness answers.** The `Harness` trait gains `dialog_kind`,
   `parse_permission_request` and `permission_reply`. Claude Code
   implements all three. Codex and pi name the dialog kind from the screen
   and report no permission hook, so `dialog answer` refuses their members.
9. **A denial travels only in the hook's reply.** The deny message starts
   `DIALOG DENIED:`, quotes the call, gives the lead's reason, and tells the
   member not to run the command another way. Neither `dialog show` nor
   `dialog answer` types into a pane or sends a message.
10. **A pending dialog waits for the lead.** A member with a pending
    recorded dialog counts as waiting for the lead, like a member in a
    question round, so it never makes the watch print `TEAM-STALLED`. Once
    its hook gives up, it counts as stuck as before.

WAT-10's source column names this ADR, because ADR 0050 decision 2 lists
WAT-10.

## What it does not do

- No allow list, and no lasting grant of any kind.
- No answer to a trust, login or update dialog.
- No message as an answer, and no denial message sent by teamwire.
- No answer for a Codex or pi member.
- No MCP tool for either command.

## Consequences

- The lead has 80 to 100 s from the watch line to answer. A busy lead misses
  it, and Claude Code then denies the call on its own, as before.
- A dialog from an ask rule has no countdown. After the hook gives up, it
  waits for a person in the pane, as before.
- Members launched before this change have no hook. Their dialogs print
  `BLOCKED <role> permission (teamwire cannot answer it)` until a handoff or
  relaunch gives them one.
- A member with shell access could still write the database directly and
  forge an answer, as for every lead-only action.
