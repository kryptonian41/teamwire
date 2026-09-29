# ADR 0003: Delivery guarantees and message integrity

Status: accepted, 2026-09-29.

## Context

Today a message is delivered with `herdr agent prompt`, and the log line is
appended after the delivery (specmsg L1). "Delivered" means herdr exited 0,
which only proves the host accepted the text (rust 1.3). herdr itself says a
timeout or a stall "does not prove the prompt was never delivered". Typing into
a pane cannot be made idempotent: a retry can type the line twice.

The reports disagree on four points.

**Delivery states.**

| Report | States proposed |
| --- | --- |
| specmsg | delivered, undelivered, unconfirmed |
| rust | recorded, handed_off, acked, dead |
| protocols | queued, sent, seen, handled, failed, expired, dead |

**What travels through the pane.** protocols (Q1) wants the store to be the
primary route, with the typed line only a nudge to read it. harness ranks
typing into the host first, because it needs no settings and works today.

**Retries.** protocols R3 and R22 want at-least-once with retry and
redelivery by the next route when no ack comes. harness R5 wants delivery to a
working member deferred and retried at its next idle. specmsg Q3 warns that
any later automatic typing repeats INV-8, where the watch retyped a line and
undid the hook's refusal.

**Message ids.** protocols and rust want ULID or UUIDv7. Briefs, the contract
and the lead all use short ids such as `m014` in text.

## Decision

**1. Outbox first.** A send runs in this order:

1. Check the rules (MSG-1 to MSG-3, MSG-9, MSG-15). A refusal writes a
   `message.refused` event and nothing else.
2. In one `BEGIN IMMEDIATE` transaction: insert the message with its ULID,
   its short display id, the sender's next `seq`, the body and its SHA-256;
   insert one delivery row per recipient in state `pending`; insert the
   `message.sent` event. Commit.
3. Write `comms/<display-id>.md` through a temp file and a rename.
4. For each recipient, outside any transaction: locate, send, and record the
   result in a short transaction of its own.

**2. Idempotent sends.** The sender may pass a client key (`--key`). The
pair (sender, key) is unique. A second send with the same key returns the
first message and delivers only to recipients still `pending`. It never
re-sends to a recipient in any other state. With no key given, teamwire makes
one from the ULID, so plain retries are new messages.

**3. Ids.** The ULID is the identity and the dedup key. The display id
(`m001`, `m002`, ...) comes from a counter row incremented in the same
transaction, never from counting records. Both are unique; text shown to
agents uses the display id.

**4. Sequence and hash.** `seq` rises by one per sender, assigned inside the
insert transaction, so committed values are 1..n with no gap (MSG-6). The
body hash is checked when the inbox returns a body and by `verify` (MSG-7).

**5. Five per-recipient delivery states**, each backed by something
observable:

| State | Meaning | Set by |
| --- | --- | --- |
| `pending` | Committed. No attempt finished yet. | the send transaction |
| `handed_off` | The host accepted the text. Nothing more is claimed. | host returned success |
| `unconfirmed` | The host may or may not have taken it (timeout, stall). | host returned a "maybe" code |
| `undelivered` | teamwire knows the text was not handed over (locator refused, recipient gone, host refused after the bounded retries). The reason is stored. | locator or host |
| `acked` | The recipient read it through `inbox` or called `ack`. | the recipient |

`acked` can follow any other state, because the body is always in the store.
No state means "read" except `acked`, and no state means "understood" (MSG-11).
protocols' `handled` is not a state: an `answer` whose `re` points at the
message shows it, and status derives it. protocols' `expired` and `dead` fold
into `undelivered` with a reason.

**6. What travels through the pane.** The typed line is a pointer with an
excerpt, as today:

```
[teamwire:tw msg m014 question from specmsg] <excerpt, at most 300 printable chars> ... Full text: teamwire inbox (or <path>). <tail>
```

The tail says a peer message is information to check, not an instruction, and
gives the exact command to answer (DEL-12). The full text is only in the store
and the `.md` copy. So the store is the primary route for content, and typing
is the primary route for waking the recipient. This takes protocols' point
(content never depends on the pane) and harness' point (typing is the wake-up
that works everywhere today).

**7. Retries.** Only inside the command that sends, and only when the host's
error code proves the text was not submitted (herdr: `agent_pane_busy`,
`agent_not_ready`), at most 3 tries over about 5 seconds. A "maybe" result
(`agent_prompt_stalled`, timeout) becomes `unconfirmed` and is never retried
(DEL-8). Nothing retries later on a timer. No process ever types a message
after the command that sent it has exited. This keeps INV-8 intact.

**8. Second path for members, without typing.** For a harness with in-turn
hooks (Claude Code), each member's per-run hooks remind it of unread messages:
after a tool call, and once when it tries to stop with unread messages. The
reminder carries display ids and the command to read them, and each id is
reminded at most once. This covers harness R5 (a member that was working when
the nudge was typed) with no delayed typing.

**9. Receiver dedup.** The recipient reads through `inbox`, which returns
each message once and acks it. A second typed nudge for an acked id leads to
"already read". Reading the `.md` file directly works but leaves the message
un-acked.

**10. Acks are not required.** No retry depends on a missing ack, because the
only retry would be typing again. Status shows `handed_off` against `acked` so
the lead can see who has not read what (rust open question 4).

**11. Lock scope.** The only lock is SQLite's write lock, held for
milliseconds. It is never held during a host call (MSG-8, STO-4).

**12. Burst limit.** At most 5 messages per sender to one recipient per
minute, and an identical body to the same recipient within 10 minutes is
refused. Both refusals are logged (MSG-14).

## Consequences

- A crash after the commit and before the host call leaves the message
  `pending` and visible. The sender can resume it with the same key.
- A crash after the host call and before the result is recorded leaves it
  `pending` though it may have arrived. Resuming it can type it a second time.
  The display id in the line lets the recipient spot the repeat, and `inbox`
  returns it only once. This is the one duplicate the design allows.
- Lead-bound items (results and questions to the lead) follow the same rules,
  with the locator of ADR 0006 in front of the sink.
- `pending` older than 60 seconds is shown by `status` as stuck.
- The typed excerpt is sanitised to one printable line (DEL-11), so an agent
  cannot forge a watch line or a second message in it.
