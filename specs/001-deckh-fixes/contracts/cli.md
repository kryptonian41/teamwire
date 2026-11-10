# CLI contract changes

This file lists the output and argument changes the feature makes to
`teamwire`. Exact wording may change in implementation. The parts in
**bold** are what tests pin. `docs/api/cli.md` gets each change in the same
commit as its code.

## `teamwire received <team> <ROLE|ID>...` (F1)

The command now accepts message display ids (`m` followed by digits),
mixed with roles. Only the lead may run it, as today.

```text
$ teamwire received deckh m008 m009 impl
m008 read
m009 already read
impl #3 received
```

The command refuses an id in these cases:

- **`m099: no such message`**
- **`m005: not a message to the lead`**

The command types nothing into any pane.

## `teamwire send --type answer --re <id>` (F1)

The output does not change. A side effect is new: the question's delivery
row for the answer's sender becomes `acked`.

## `teamwire send` to a member that asked for a handoff (F3)

```text
$ teamwire send deckh --to impl --type note --text "..."
impl: held for handoff (m012)
```

The exit code is 0. Nothing is typed.

A reprompt to that member types its own text and then one more sentence:
**`Held for you, oldest first: m010 (answer from lead), m012 (note from lead). Read them with teamwire inbox.`**

## `teamwire resend` of a held message (F3)

```text
m012 -> impl: refused: held for handoff; run teamwire handoff deckh --role impl, or reprompt impl
```

## Watch lines (F1, F2)

| Line | Change |
|---|---|
| `QUESTION-UNDELIVERED <id> from <role>` | appends **` File: <team dir>/comms/<id>.md`** |
| `QUESTION-UNCONFIRMED <id> from <role>` | same |
| `UNDELIVERED <role> <reason> Result: <path>` | unchanged (already names the file) |
| `TEAM-COMPLETE ...` | not printed while a member with an `UNDELIVERED` or `UNCONFIRMED` result has a working or blocked session |
| `UNDELIVERED` / `UNCONFIRMED`, when the session cannot be seen | the line says **`session not visible`** |

## The lead's hook note (F1)

The note is added to the lead's turn by the lead's tool hook and Stop hook,
on Claude Code, Codex and pi. Nothing is typed. The note has one line per
item:

```text
teamwire: m008 (question from impl) was never typed into your pane. Read /Users/.../teams/deckh/comms/m008.md, then run teamwire received deckh m008.
teamwire: impl's result #3 was never typed into your pane. Read /Users/.../teams/deckh/outbox/impl.md, then run teamwire received deckh impl.
```

The key phrase is **`was never typed into your pane`**.

## `teamwire status` (F2, F3, F10)

- A member with an `undelivered` or `unconfirmed` result whose session works
  reads **`working`**.
- Under `--role <role>`, a working member gets one more line:

```text
tool: Bash, running 15m02s
tool: unknown (this harness does not report it)
```

- `status --json`: `members[].current_tool` is `{ "name": "Bash",
  "started_at": "2026-11-10T14:12:26.311Z", "seconds": 902 }` or absent.
  The dashboard mirrors it in `ui/src/shared/teamwire.ts`.

## `teamwire close --dry-run` (F11)

```text
kit: removal deletes ignored files: .env.local; removes symlinks (targets kept): apps/ai/.venv -> /Users/.../knit-3.0/apps/ai/.venv, node_modules -> ../../node_modules
kit: removes symlinks (targets kept): old -> /gone (broken)
```

## `teamwire speckit draft <tasks.md> [--repo <dir>]` (F4, F5, new)

The command is read only and prints text. It exits non-zero when the file
is missing or has no task lines.

```text
spec-kit tasks: specs/001-deckh-fixes/tasks.md (57 tasks, 6 phases, 3 stories)

Shared files between phases:
  Phase 3 and Phase 4: crates/teamwire-watch/src/status.rs, docs/api/cli.md
Stories that share files: US1 and US3 (crates/teamwire-watch/src/status.rs, ...)

Tasks that change no file: T048
Tasks whose files no other task names: T051 (apps/copilot-runtime/src/x.ts)

Unread workflows: .github/workflows/odd.yml (anchors are not supported)

DRAFT owns lists. Check each one before you launch: these are drafts, not final lists.
US1:
  crates/teamwire-msg/src/send.rs            (task path)
  .github/workflows/evals-harness-replay.yml  (CI paths glob apps/ai/harness/**)
  apps/ai/.importlinter                       (names module harness.tools)
  apps/ai/tests/test_harness_targets.py       (names file tools.py)
US2:
  ...
Setup and polish:
  ...
```
