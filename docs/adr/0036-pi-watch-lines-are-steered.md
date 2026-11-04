# ADR 0036: A pi lead's watch line is steered

Status: accepted, 2026-11-04. Supersedes these sentences of ADR 0030, and no
others:

- ADR 0030, context (lines 31-35), the sentence that an extension hands text
  to the model with `deliverAs: "followUp"`, which "waits for the end of the
  run when it is not" idle.
- ADR 0030, decision 5, the clause `deliverAs: "followUp"`.
- ADR 0030, "Facts still unchecked", point 2, which is now checked. A watch
  line queued with `deliverAs: "followUp"` during a run does reach the model
  when that run ends, and this ADR records the proof.

Every other sentence of ADR 0030 stands. The watch still runs as a child of
pi inside the extension, the tool still waits up to 2 s, `session_shutdown`
still stops every watch of the process, and `reads_watch_lines` is still
true for pi.

## Context

A pi lead saw a watch line minutes after the watch printed it. The worst case
on this machine was a `TEAM-COMPLETE` delivered 29 minutes and 46 seconds
late, by which time the lead had added members, so the line's count was wrong
and the lead read it as a false alarm. A read-only survey on 2026-11-04 found
53 of 53 watch lines in every pi lead session on this machine entering the
transcript within 2 ms of an assistant message that made no tool call, which
is the moment the run ended. Across the five teams it could pair without
doubt, the median delay was 107 seconds and 16 lines were over 5 minutes
late. A Claude Code lead's median for the same measurement was 0.3 seconds.

Every kind of watch line was affected, not only `TEAM-COMPLETE`. A
`TEAM-STALLED` arrived 8 minutes 49 seconds late, and the lead's skill tells
it to report `TEAM-STALLED` to the user, so a late one makes that report
wrong.

The cause is the delivery mode. pi offers three
(`pi-coding-agent/docs/extensions.md:1643-1646`, pi 1.5.0):

| mode | when the model sees the message |
| --- | --- |
| `"steer"`, the default | after the current turn's tool calls, before the next model call |
| `"followUp"` | only once the agent has no more tool calls |
| `"nextTurn"` | with the next user prompt; starts nothing |

pi's agent loop reads the two queues in different places. The steering queue
is read at the end of each turn (`pi-agent-core/dist/agent-loop.js:191`,
through the closure at `pi-agent-core/dist/agent.js:339-341`). The follow-up
queue is read only after the loop has finished every tool call
(`pi-agent-core/dist/agent-loop.js:197`, under the comment "Agent would stop
here"). So a follow-up waits for the whole run, and a long run holds every
line it printed.

## Decision

1. **The lead extension hands each watch line to pi with
   `deliverAs: "steer"`.** `triggerTurn: true` and
   `customType: "teamwire.watch"` are unchanged. The extension's own "the
   watch stopped" line goes through the same single `pi.sendMessage` call, so
   one mode decides when every line arrives.

2. **A steered line never lands between a tool call and its result.** This is
   checked, not assumed. `pi-agent-core/dist/agent-loop.js:170` runs the
   turn's tool calls, and lines 171-178 push every result into the context.
   Only then does line 191 read the steering queue, and the loop pushes the
   steered message into the context at lines 121-125, at the top of the next
   turn. `Agent.steer` (`pi-agent-core/dist/agent.js:187-189`) only pushes
   onto a queue; it aborts nothing. pi guards the same ordering for a
   message sent with `triggerTurn: false`, and says why
   (`pi-coding-agent/dist/core/agent-session.js:1888-1891`): a message
   appended mid-turn "would put the message between an assistant tool call
   and its result, which providers that validate message order reject on
   replay".

3. **A steered line is never lost while the lead is idle.** An idle session
   takes the `triggerTurn` branch
   (`pi-coding-agent/dist/core/agent-session.js:1882`), which calls
   `_startRun` and starts a turn at once. While pi emits `agent_settled` the
   whole call is deferred (`agent-session.js:1884-1886`) and replayed when
   that emit finishes (`agent-session.js:708-713`), so such a line is
   delayed by one event and not dropped. Losing a line needs
   `triggerTurn: false` or `deliverAs: "nextTurn"`, and the extension uses
   neither.

4. **A test pins the mode in the extension that `install` writes.**
   `crates/teamwire-harness-pi/tests/lead_setup.rs` reads the rendered
   package files and asserts the text holds
   `{ triggerTurn: true, deliverAs: "steer" }`, holds no `followUp`, and
   holds exactly one `pi.sendMessage(` call.

5. **teamwire changes no pi setting.** `steeringMode` and `followUpMode` both
   default to `"one-at-a-time"` (`pi-agent-core/dist/agent.js:141`), and the
   user's `settings.json` sets neither. teamwire reads pi's settings for its
   package list and writes nothing else there.

## Rejected

- **Set `followUpMode: "all"` in the user's pi settings.** The whole backlog
  would arrive in one go instead of one line per run, so the burst would be
  shorter. The first line would still wait for the run to end, which is the
  actual problem. Writing into the user's pi settings is also outside what
  `teamwire install` does.
- **Keep `"followUp"` and tell the lead in its skill to run
  `teamwire status` before acting on any watch line.** No code would change,
  but every line would cost the lead a tool call, and a line would still be
  minutes old when it arrived.
- **Have the extension drop a line from a watch it has replaced.** By the
  time a new watch starts, the old line is already inside pi's queue and the
  extension has no handle on it. Each line carrying its print time solves
  the same problem for every harness.

## Consequences

- A pi lead reads a watch line within the run that was going on when the
  watch printed it, at the end of the current turn, instead of at the end of
  the whole run.
- A backlog still drains one line per read, because `steeringMode` defaults
  to `"one-at-a-time"`. The read now happens at the end of every turn rather
  than once per run, so six queued lines clear over six turns of one run
  instead of six run endings.
- The refusal "a watch of `<team>` already runs in this session" gets rarer.
  A lead asked for it after a stale `TEAM-COMPLETE` told it the watch had
  exited, and the line is no longer stale.
- A watch that exits on its first poll is a separate fault and this ADR does
  not touch it. A watch of a settled team printing `TEAM-COMPLETE` at once
  left one team unwatched for three and a half minutes.
- An installed pi package is out of date until `teamwire install` runs again,
  and `teamwire doctor` says so.
- The file:line references above are pi 1.5.0, the version
  `~/.pi/agent/install/current-version` names. pi may move them. The claim
  that matters is the one pi documents: a steered message arrives after the
  current turn's tool calls and before the next model call.
