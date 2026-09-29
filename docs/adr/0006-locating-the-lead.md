# ADR 0006: Finding a session's current place before typing into it

Status: accepted, 2026-09-29.

## Context

On 2026-09-14 eleven reports were typed into an unrelated conversation that
had taken over the lead's pane (INV-1). The Python fix (`team_parent.py`)
returns a pane only when three sources agree: Claude's session registry, that
process's own `HERDR_PANE_ID`, and `herdr pane list`. It follows the session
id, not the launch pid, because `claude --resume` keeps the id and changes the
pid (INV-2).

Open points from the reports:

- specmsg Q1: another harness or host may offer fewer sources. Is one trusted
  source enough when the host can address by session id, or must delivery be
  queued whenever there are fewer than two? specmsg recommends queueing.
- specmsg A9, L12: members are reached today by agent name, with no session
  check.
- specmsg L13: between "confirmed" and "sent" the pane could change hands.
- The lead is not launched by teamwire, so teamwire cannot choose its session
  id or give it hooks.

## Decision

1. **Two independent views must agree.** The locator takes evidence from the
   harness side and from the host side and delivers only when they name the
   same single place:
   - **Harness view:** which live processes run session S
     (`Harness::processes_for_session`), and which place each process says it
     is in (`Platform::env_of(pid, <host's place variable>)`, for herdr
     `HERDR_PANE_ID`).
   - **Host view:** who occupies place P (`Host::occupant`): the session id
     when the host knows it (herdr `agent_session`), otherwise the process
     ids in P's process tree (tmux).

   The place is chosen when exactly one place P is named by the harness view
   and the host view confirms S (or a pid of S) in P. Every other case refuses
   with a reason code:

   | Reason code | Case |
   | --- | --- |
   | `no_live_process` | No live process runs session S |
   | `no_place` | The process has no place variable |
   | `host_other_session` | The host says P holds another session |
   | `host_no_session` | The host says P holds nothing, or does not list P |
   | `host_listing_failed` | The host call failed or timed out |
   | `two_places` | S is confirmed by the host in more than one place |
   | `single_source` | Only one view is available for this harness and host |
   | `background_session` | The lead was recorded as a background session |

   Two processes that claim S where the host confirms only one place go to
   that place (specmsg A4). One source is never enough (specmsg Q1 decided:
   queue). A harness and host pair that cannot give two views makes launch
   print a warning and routes lead-bound items through the watch (ADR 0007).
2. **The same check applies to members** (DEL-5). A member's session id is
   chosen by teamwire at launch where the harness allows it, and recorded.
   The member's recorded place is only a hint for where to look.
3. **Check again right before the send.** The whole lookup runs, then the
   host view alone runs a second time immediately before `Host::submit`, and
   a change refuses the send. The remaining window is the time between that
   second `occupant` call and the `submit` call, a few milliseconds with
   herdr. herdr cannot address by session id, so the window cannot be closed
   from our side. We record this in the architecture.
4. **The lookup never uses the pid or place recorded at launch as evidence.**
   Following a resumed lead works because the lookup starts from the session
   id.
5. **The lead's session id and kind are recorded at launch** from the
   harness (`CLAUDE_CODE_SESSION_ID`, `CLAUDE_CODE_SESSION_KIND`). A lead
   recorded as a background session is refused at launch (LCH-1), and the
   locator refuses it too (DEL-6).

## Consequences

- Refusals are visible: every refused lead-bound item becomes `undelivered`
  with the reason code, and the watch prints it (INV-7).
- If herdr's SessionStart hook, which reports each Claude session id to herdr,
  is missing, the host view has no session ids and every lead-bound item is
  refused as `host_no_session`. `teamwire doctor` checks for that hook
  (read-only) and says so.
- The tmux host gives weaker evidence (pids, not session ids). It is still an
  independent view, because tmux learns the pane's processes from the kernel,
  not from the harness.
- The residual window of point 3 stays. It is small and named.
