# Data Model: Fixes from team deckh's live run

The feature adds no table. It adds one schema step, version 11, which
rebuilds `events` with one more type in its closed list. It changes the
rules that move existing rows, and the rule that reads waivers. It adds one
event type, one error code, one core type and two view fields.

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
| `undelivered`, `unconfirmed` or `pending` to `lead` | `acked` | `received <team> <id>` | Only the lead may run it. `<id>` matches `^m[0-9]+$` and no role of the team has that name; when a role has it too, the command refuses and names both. A `handed_off` or `acked` row changes nothing, and the output says `already delivered` or `already read` (FR-003). |
| any state but `acked` of question `q` to recipient R | `acked` | `send --type answer --re q` from R | In the same write as the answer (FR-005). |

Unchanged: no process types a row after the command that sent it has exited
(ADR 0003 point 7, INV-8).

## Member generation (`members.generation`, `bump_generation`)

| Event | Generation | Rule |
|---|---|---|
| `reprompt` to the member | +1 | As today (REP-3). |
| Any other message type to a member whose newest completion is in its current generation (`done`, `blocked` or `failed`), when the summary does not start with `handoff:` | +1 | New (FR-008). The bump happens in the same write as the message, and writes `member.woken`, not `member.reprompted`. |
| The same, with a `handoff:` summary | unchanged | The message is held. |
| `handoff` | +1 | As today (ADR 0027, ADR 0032). |
| A known delivery failure of the message that bumped | undone | The same undo as a reprompt (REP-3), written as the same event type as the bump with `undone: true`. An `unconfirmed` delivery keeps the bump. |

## Event type `member.woken` (new; `events`, schema version 11)

| Field | Value |
|---|---|
| `type` | `member.woken` (`EventType::MemberWoken`) |
| `role` | the woken member |
| `data` | `generation` and `message`, as `member.reprompted` has; an undo adds `undone: true` |

Readers that take `member.reprompted` as a generation start also take
`member.woken`: the watch's `is_unprompted`, the message crate's
interrupted-reprompt check, and teamwire-otel's generation span and undo
stack. The reprompt counter and the outcome report's `reprompts` do not
count it. Schema version 11 rebuilds `events` with the longer CHECK list, as
version 10 did. A store at version 11 cannot be opened by an older binary.

## Member verdict in the watch (`crates/teamwire-watch/src/plan.rs`)

The verdict for the newest completion with report state `Undelivered` or
`Unconfirmed`:

| Session seen by `look` | Verdict | Status word |
|---|---|---|
| `Alive(Working)` | waiting (not settled) | `working` |
| `Alive(Blocked)` | waiting (not settled) | `working (at a dialog)` (the dashboard's `classifyState` sorts it as `active`) |
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
  number, the result file path, and `teamwire received <team> <role>`. The
  path follows the watch's `Result:` rule: `outbox/<role>.md`, or
  `reports/<role>.md` when the result file is missing or no longer holds
  the recorded text.

## StatusView field (new, `crates/teamwire-watch/src/lib.rs:190`)

`held_deliveries: Vec<HeldDelivery>`. A `HeldDelivery` holds `display_id`
and `to`. A row with code `held_for_handoff` goes here and not into
`stuck_deliveries`. The CLI prints each as `held for handoff: <id> -> <role>`.
It appears in `status --json` and is mirrored in the zod `StatusView` in
`ui/src/shared/teamwire.ts`. The dashboard's "needs a look" panel reads only
`stuck_deliveries`, so it does not show held rows.

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

## Waiver (`waivers`, `crates/teamwire-store/src/schema/v1.sql:115-122`; US4)

Fields: `role`, `flag_id`, `head_sha`, `reason`, `created_at`. The key is
(`role`, `flag_id`, `head_sha`). The table does not change. `waive` still
records a row at the member's HEAD now.

**When a waiver counts at HEAD `h`** (new rule, ADR 0045):

| The waived flag | The waiver at `head_sha` = `w` counts when |
|---|---|
| any flag | `w` equals `h` (as today) |
| a flag that names one file `p`: `test-deleted`, `config-changed`, `outside-owns`, `weakened`, `lint-removed`, `test-removed`, `asserts-removed` | git knows `w`, and the blob of `p` at `w` equals the blob of `p` at `h`. A file absent at both counts as equal. |
| a flag that names no file (`no-commits`, `no-owns`, `no-base`, `report-*`, `bad-owns:*`, `diff-failed`, `status-failed`) | only when `w` equals `h` |
| `uncommitted:<path>` | only when `w` equals `h` |
| `head-moved` | never (as today) |

**Raw flag** (in memory, `crates/teamwire-verify/src/inspect.rs`): gains
`path: Option<String>`, set for the file flags in the table and `None` for
every other flag. The stored and printed `Flag` does not change.

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
