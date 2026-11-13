# CLI contract changes

This file lists the output and argument changes the feature makes to
`teamwire`. Exact wording may change in implementation. The parts in
**bold** are what tests pin. `docs/api/cli.md` gets each change in the same
commit as its code.

## `teamwire received <team> <ROLE|ID>...` (F1)

The command now accepts message display ids (`m` followed by digits),
mixed with roles. An argument is an id only when it matches `^m[0-9]+$` and
no role of the team has that name. Only the lead may run it, as today.

```text
$ teamwire received deckh m008 m009 m010 impl
m008 read
m009 already read
m010 already delivered
impl #3 received
```

A row already `acked` prints `already read`, and a row already
`handed_off` prints `already delivered`. Neither row changes.

The command refuses an id in these cases:

- **`m099: no such message`**
- **`m005: not a message to the lead`**
- **`m12: both a role and a message; nothing recorded`**, when a role of
  the team is named `m12`

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

The key phrase is **`was never typed into your pane`**. A result line names
the same file as the watch's `Result:` path: `outbox/<role>.md`, or
`reports/<role>.md` when the result file is missing or no longer holds the
recorded text.

## The member's seat text (F1)

The rule in `talking_head.md` about NOT DELIVERED and UNCONFIRMED keeps "Do
not send it again". The sentence that tells a member to retry a message to
the lead with `resend` goes. In its place: **the lead's hooks list a message
to the lead that was never typed, so do not resend it**.

## `teamwire status` (F2, F3, F10)

- A member with an `undelivered` or `unconfirmed` result whose session works
  reads **`working`**. When its session waits at a dialog mid-turn, it reads
  **`working (at a dialog)`**.
- A held message gets its own line, outside "deliveries that need
  attention":

```text
held for handoff: m004 -> impl
```

- `status --json`: `held_deliveries` is `[{ "display_id": "m004", "to":
  "impl" }]`, and a held row is not in `stuck_deliveries`. The dashboard
  mirrors it in `ui/src/shared/teamwire.ts`.
- Under `--role <role>`, a working member gets one more line:

```text
tool: Bash, running 15m02s
tool: unknown (this harness does not report it)
```

- `status --json`: `members[].current_tool` is `{ "name": "Bash",
  "started_at": "2026-11-10T14:12:26.311Z", "seconds": 902 }` or absent.
  The dashboard mirrors it in `ui/src/shared/teamwire.ts`.

## `teamwire events --json` (F3)

A generation that an `answer`, `note`, `finding` or `notice` starts writes a
**`member.woken`** event, with `data.generation` and `data.message`. An undo
after a known delivery failure writes `member.woken` with `undone: true`.
`member.reprompted` stays for reprompts only. The metrics' reprompt count
does not include `member.woken`. The dashboard's `EventType` list gains the
value.

## The closer's seat text and preset (F8)

The closer preset has two slots, **`Local gate: CHANGEME`** and
**`Gates left to CI: CHANGEME`**. The closer seat text in
`kind_closer.md` says to run the **local gate** the brief names once on each
head that ships, to run none of the gates the brief leaves to CI, and to
report both, with the local gate's exit code.

## `teamwire close --dry-run` (F11)

```text
kit: removal deletes ignored files: .env.local; removes symlinks (targets kept): apps/ai/.venv -> /Users/.../knit-3.0/apps/ai/.venv, node_modules -> ../../node_modules
kit: removes symlinks (targets kept): old -> /gone (broken)
```

## `teamwire verify` waivers (F12)

The output format does not change. A waiver for a flag that names one file
now also counts at a later HEAD while that file is unchanged, and the flag
prints `(waived)` there:

```text
$ teamwire verify deckh --role spec
spec         PASS     exit 0 in 41s, 2 flag(s)
             ! [config-changed:.specify/memory/constitution.md] (waived) check config changed outside owns: .specify/memory/constitution.md
             ! [outside-owns:.specify/memory/constitution.md] (waived) changed outside owns: .specify/memory/constitution.md
             head 0e05a21fac...
```

The `--waive` confirmation, `waived <flag> for <role> at its current HEAD`,
does not change. `teamwire verify` has no `--json` output, so the dashboard
schema does not change.

## `teamwire speckit draft <tasks.md> [--repo <dir>]` (F4, F5, new)

The command is read only and prints text. It exits **1** with a named error
when the file is missing, cannot be read, or has no task lines. Exit 3
stays for usage errors.

It reads a path with a `:N` or `:N-M` suffix as the path alone. An indented
line under a task line belongs to that task, until the next task line,
heading or blank line. A path token that holds `*` matches the repository's
tracked files as a glob.

```text
spec-kit tasks: specs/001-deckh-fixes/tasks.md (52 tasks, 7 phases, 4 stories)

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
