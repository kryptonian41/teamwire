# ADR 0007: How a member's completion reaches the lead

Status: accepted, 2026-09-29.

## Context

Today a member writes `done/<role>.json` with a plain write. Claude Code's
Stop hook (`team-report.sh`) sees it at the end of the turn, checks the report
once, writes `outbox/<role>.md` with the final message from the hook input,
finds the lead's pane, types one line, and writes a marker keyed by the done
file's mtime. The split is deliberate: deciding "I am done" is the model's
job, delivering is the hook's, so a forgetful member is still reported.

Weak points found by the reports:

- A torn or invalid done file reads as `done` (speclife F5, specmsg L6).
- The marker compares a float mtime (specmsg L5), and is written after
  delivery, so a crash re-sends (L4).
- Only Claude Code has a Stop hook. speclife Q2 asks what replaces "hold once
  and deliver on the next stop" for Codex, Gemini CLI or opencode.
- specmsg Q5 asks whether the progress-line and decisions check stays in the
  hook or moves to `verify`.

## Decision

1. **Completion is a store record.** A member runs `teamwire done --status
   <done|blocked|failed> --summary "<text>"` or the MCP tool `team_done`.
   It validates the status, stores the summary (up to 2000 characters; the
   wake-up line shows 200 after sanitising), and commits a completion row:
   the member, its current generation, a completion number that rises by one
   per member, and `report_state = pending`. There is no done file. An
   invalid call is refused and records nothing (REP-1).
2. **Report state per completion**: `pending`, `delivering`, `delivered`,
   `undelivered` (with reason), `unconfirmed`. Only the newest completion of
   the member's current generation is ever delivered; older ones stay as
   history.
3. **Route A, harness with a turn-end hook (Claude Code).** The member's
   per-run Stop hook runs `teamwire hook claude-stop`, which:
   1. exits 0 at once if `TEAMWIRE_TEAM` is unset or the stop is a
      subagent's (`agent_id` present) (REP-10);
   2. checks that the hook input's `session_id` equals the member's recorded
      session id, else exits 0 (REP-11);
   3. if the newest completion is `pending` and `stop_hook_active` is false,
      runs the report check once. Gaps hold the member with
      `additionalContext` that states the facts: which file, which commits
      lack a line, that the decisions list is missing (REP-5, INV-18). Unread
      messages are added to the same note. Any error in the check means "no
      gaps";
   4. otherwise writes the result file, commits `delivering`, locates the
      lead (ADR 0006), types the line, and commits the outcome. A completion
      found in `delivering` is a crash left over; it is delivered again, the
      one duplicate the design allows (REP-2);
   5. exits 0 on every path, whatever happened (REP-10).
4. **Route B, harness without a turn-end hook.** `teamwire done` runs the
   report check itself and, when there are gaps, returns the facts in its
   result and records nothing the first time for that generation. The second
   call records the completion whatever the report says. This keeps "told
   once, then delivered" without a hook. The completion is then reported by
   the watch: it writes nothing into any session, it prints `COMPLETED <role>
   (<status>) <summary> Result: <path>` once and marks the completion
   `delivered` with route `watch`. The result file says the final message was
   not available from this harness.
5. **Route C, lead unreachable.** When the locator refuses or the host fails,
   the completion becomes `undelivered` with the reason, and the watch prints
   `UNDELIVERED <role> <reason> Result: <path>` once (INV-7). A lead with no
   locatable place (no host, or fewer than two views) gets route B from the
   start, chosen at launch and recorded on the team.
6. **The watch does not repeat route A.** A completion delivered by the hook
   produces no watch line (WAT-1). A completion still `pending` for two polls
   produces `DECLARED` once (WAT-12).
7. **The report check stays at completion time**, in route A and route B, and
   also runs in `verify` (VER-3). It is cheap and exists for two incidents
   (INV-18, INV-19). specmsg Q5 is decided: keep it.
8. **Result file** `outbox/<role>.md` is written before any delivery attempt,
   atomically, and starts with the marker
   `<!-- teamwire:generated completion=<ulid> -->`. An unmarked file found
   there is moved to `reports/<role>.rescued-<utc>.md` and linked from the new
   result (REP-8). The result holds: status, generation, completion number,
   time, summary, the source of the final message (`hook`, `transcript`,
   `none`), the member's report path, the first 300 characters of the task,
   then the full final message.
9. **Wake-up line**:
   `[teamwire:<team>] <role> finished (<status>). <summary, 200 chars> Full result: <path>`.
   The prefix is `teamwire:`, not `team:`, so while both tools run a lead can
   tell which one wrote a line.
10. **Reprompt** (`send --type reprompt`, lead only) raises the member's
    generation in the same transaction that stores the message. A known
    delivery failure lowers it again, if no completion exists yet in the new
    generation. An `unconfirmed` delivery keeps it (REP-3).

## Consequences

- A torn completion cannot exist: it is one row, committed or not.
- "Reported once" is keyed by a completion id and a state, never by a time
  stamp.
- A harness without hooks loses only the final message, which the result file
  says plainly. Its members are told about report gaps by the `done` command.
- A member that calls `done` mid-turn and keeps working is reported at the
  end of that turn (route A), with `DECLARED` from the watch if it takes more
  than two polls.
