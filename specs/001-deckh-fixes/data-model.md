# Data Model: Fixes from team deckh's live run

The feature adds no table and no schema migration. It changes the rules
that move existing rows. It adds one error code, one core type and one view
field.

## Delivery row (`deliveries`, `crates/teamwire-store/src/schema/v1.sql:66-80`)

Fields used: `message_ulid`, `recipient`, `state` (`pending`, `handed_off`,
`unconfirmed`, `undelivered`, `acked`), `attempts`, `error_code`, `acked_at`.

**New error code**: `held_for_handoff`. This is a new `DeliveryErrorCode`
variant, in `crates/teamwire-core/src/message.rs:112-145`. The column is free
`TEXT`, so the code needs no migration.

**New and changed moves:**

| From | To | Who moves it | Rule |
|---|---|---|---|
| (new row) | `undelivered` + `held_for_handoff` | `send` | The recipient member's newest completion is in its current generation and its summary starts with `handoff:` (FR-009). Nothing is typed. |
| `undelivered` + `held_for_handoff` | `acked` | the member's `inbox` read | After a handoff, or after a reprompt that named it (FR-009). |
| `undelivered` + `held_for_handoff` | (unchanged) | `resend` | `resend` refuses the row and names `handoff` or `reprompt`. |
| `undelivered`, `unconfirmed` or `pending` to `lead` | `acked` | `received <team> <id>` | Only the lead may run it. An already acked row changes nothing (FR-003). |
| any state but `acked` of question `q` to recipient R | `acked` | `send --type answer --re q` from R | In the same write as the answer (FR-005). |

Unchanged: no process types a row after the command that sent it has exited
(ADR 0003 point 7, INV-8).

## Member generation (`members.generation`, `bump_generation`)

| Event | Generation | Rule |
|---|---|---|
| `reprompt` to the member | +1 | As today (REP-3). |
| Any other message type to a member whose newest completion is in its current generation (`done`, `blocked` or `failed`), when the summary does not start with `handoff:` | +1 | New (FR-008). The bump happens in the same write as the message. |
| The same, with a `handoff:` summary | unchanged | The message is held. |
| `handoff` | +1 | As today (ADR 0027, ADR 0032). |
| A known delivery failure of the message that bumped | undone | The same undo as a reprompt (REP-3). An `unconfirmed` delivery keeps the bump. |

## Member verdict in the watch (`crates/teamwire-watch/src/plan.rs`)

The verdict for the newest completion with report state `Undelivered` or
`Unconfirmed`:

| Session seen by `look` | Verdict | Status word |
|---|---|---|
| `Alive(Working)` | waiting (not settled) | `working` |
| `Alive(Blocked)` | waiting (not settled) | `undelivered` / `unconfirmed` (as today) |
| `Alive(Idle)` | settled | `undelivered` / `unconfirmed` |
| died (`look` returns `None`) | settled | as today |
| `Absent` or `NoInfo` | settled, and the line says `session not visible` | `undelivered` / `unconfirmed` |

The announcement line still prints once per completion.

## Lead's undelivered note (new, `crates/teamwire-report/src/lead_stop.rs`)

The note is computed and never stored. It has one line for each item of the
open teams that the lead session leads:

- a delivery to `lead` in state `undelivered`: the message id, type,
  sender, `comms/<id>.md` path, and `teamwire received <team> <id>`;
- a completion with report state `Undelivered`: the role, the completion
  number, the result file path, and `teamwire received <team> <role>`.

## ToolRun (new core type, `crates/teamwire-core/src/tool_run.rs`)

| Field | Type | Meaning |
|---|---|---|
| `name` | `String` | The tool's name as the harness writes it, such as `Bash` |
| `started_at` | `Timestamp` | When the call started, from the session file or the pi state file |

The type is returned by `Harness::current_tool`. The default
implementation returns `Ok(None)`.

## MemberView field (new, `crates/teamwire-watch/src/lib.rs:57-81`)

`current_tool: Option<ToolView>`. A `ToolView` holds `name`, `started_at`
and `seconds`. The field is set only under `status --role` for a working
member. It appears in `status --json` and is mirrored as
`current_tool: ToolView.nullish()` in `ui/src/shared/teamwire.ts:71-95`.

## Task line and owns draft (US2, in memory only)

- **Phase**: `n`, `title`, and its task lines.
- **Task line**: `id` (`T<digits>`), `parallel` (`[P]`), `story`
  (`US<n>`, or none, which means setup and polish), `description`,
  `paths` (a set).
- **Owns draft**: one per story, plus one for setup and polish. It holds
  `paths` from the tasks, plus `mentions`: (`file`, `reason`) pairs. A
  reason is one of `names path`, `names module <m>`, `names file <f>`, or
  `CI paths glob <g>`.
- **Unread workflow**: a `.github/workflows` file that the YAML reader
  refused, with the reader's error.
