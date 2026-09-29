# ADR 0010: What version 1 does not build, and why

Status: accepted, 2026-09-29.

## Context

The brief asks for the simplest design that meets the requirements, and for
an ADR that says what was left out. Each item below was proposed or discussed
by at least one report. Leaving it out keeps v1 smaller; none of the items is
needed to meet a MUST.

## Decision

| Not built in v1 | Proposed by | Why not | What stands in its place |
| --- | --- | --- | --- |
| A daemon that owns state | rust option A | More ways to fail, stalls on wake, an upgrade drops work (ADR 0001) | Short commands on a shared SQLite file |
| Automatic redelivery on a timer or at the recipient's next idle | protocols R3, R22; harness R5 | Any later typing repeats INV-8. The body is in the store anyway | Member-side hook reminders (ADR 0003, point 8); `status` shows `pending` and `undelivered` |
| Claude Channels push | harness R16; protocols R22 | Research preview, a warning dialog at every start, silent drops, no ack, needs claude.ai auth | Typed nudge plus `inbox`. The flag and adapter hook are designed for (ADP-11) but not written |
| Claude cross-session `SendMessage` socket as a teamwire route | harness R5 step 3 | Undocumented wire format; a repo setting can hold it (INV-4) | Host typing |
| A headless host (`claude -p --input-format stream-json`) | harness R15 | The input line shape and the replay ack are untested (harness Q5); dialogs cannot be answered | The trait allows it; a member with no place works through `inbox` |
| Codex CLI, Gemini CLI and opencode adapters | brief (must fit) | Not installed here; facts are docs-only (harness Q6) | The `Harness` trait, its capability table, and the fake harness that proves the core never names Claude |
| A2A server per agent, ACP as the team protocol | protocols 1.3, 1.4 | Network-service and editor-to-agent shapes, not team messaging | A2A words (task, message, artifact) in names; an ACP harness adapter can come later |
| MCP Tasks extension | protocols 5 | A second state model beside ours | Plain tools |
| MCP over HTTP | rust 24 | No proven need; stdio needs no port | stdio server per session |
| A process that watches the lead | speclife Q8 | Nothing outside the lead survives it without a daemon | `status` shows lead liveness; open question for the user |
| Renaming the team's host workspace when the lead's workspace is renamed | speclife W13 | Cosmetic and herdr-only | Titles stored and shown by `status`; the herdr adapter labels once at launch |
| Windows | rust Q7 | Process-group, environment-reading and sleep-hold calls all differ, and no team runs there | macOS and Linux |
| An importer for Python teams | specmsg A5 | teamwire has no old teams; running Python teams finish under Python | Side-by-side running (see Migration in `ARCHITECTURE.md`) |
| `loom`, `madsim`, `turmoil` | rust 1.11 | teamwire ships no hand-written atomics and no network code | proptest, crash tests, fault-injecting fakes |
| `tracing-opentelemetry` live spans | rust 1.10 | Spans are rebuilt from events after the fact | Event rows with start and end times |
| Hard refusal of prose-only planning rules | speclife LC-19, Q7 | Python never enforced them; hard refusals would block the lead's real overrides | Warnings at launch (LCH-18) |

## Consequences

Each row is a place a later version can extend without changing the core:
new adapters implement existing traits, and new routes are new entries in the
capability table. The open questions for the user list the rows where the
user's preference could change the answer (Channels, lead watching, the
supported platforms).
