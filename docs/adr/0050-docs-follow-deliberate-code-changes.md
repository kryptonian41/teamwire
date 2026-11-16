# ADR 0050: Corrections to accepted ADRs that later code and ADRs changed on purpose

Status: accepted, 2026-11-16. Supersedes these sentences, and no others:

- ADR 0021, decision 3 (lines 33-35), the sentence that `acked` "still says
  only that the recipient read the message through `inbox` or `ack`".
- ADR 0022, decision 4 (line 37), "`acked` keeps its meaning, as ADR 0021
  point 3 says", where it carries that same sentence.

## Context

AGENTS.md says an ADR changes only by adding one that supersedes it. ADR 0026
set the form for such a correction. The re-audit of teamwire at `b8d10cbb`
(team `reaudit`, 2026-11-16) found more sentences that the code no longer
follows. In each case a later ADR, a requirement row, or a code comment calls
the new behaviour deliberate, but no ADR names the old sentence as
superseded. The lead of team `reaudit` decided that such a sentence stays
wrong until an ADR names it. This ADR names each one. It changes no program
behaviour.

## Decision

1. **A forward table.** Each sentence listed above is shown with what the
   program does now and what made the change. The old ADR files stay as they
   are.

   | Old ADR and point | What the program does now | Changed by |
   | --- | --- | --- |
   | 0021, decision 3 | `acked` says that the recipient took the message in by one of its own actions: `inbox`, `ack`, the lead's `teamwire received <id>` for a row that was never typed (`crates/teamwire-report/src/received.rs:96-99`), or an answer whose `re` names the message (`crates/teamwire-msg/src/send.rs:387-405`). Each writes `delivery.acked`, and `teamwire.messages.unread` counts none of them | ADR 0043, decisions 2 and 3; DEL-19 |
   | 0022, decision 4 | The same as the row above. ADR 0022's own rule still adds no way to reach `acked` | ADR 0043, decisions 2 and 3 |

2. **Requirement rows follow the code.** These rows of `docs/requirements.md`
   now say what the program does, and each names this ADR or the ADR that made
   the change:

   | Row | Changed by |
   | --- | --- |
   | MSG-11 (what sets `acked`) | ADR 0043, decisions 2 and 3; this ADR |

## Consequences

- No existing file under `docs/adr/` changes. A reader of an old ADR finds
  the replacement here.
- The comment at the top of `crates/teamwire-msg/src/inbox.rs` no longer
  calls `inbox` and `ack` the only way to reach `acked`.
