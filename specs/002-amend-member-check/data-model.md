# Data model: Amend a running member's check and owns

No table, column, index or event type is added. The store stays at schema
version 11 (user decision 2). Everything below is an existing record that
this feature writes or reads in a new way.

## Member scope (existing `members` columns)

| Field | Type | Written by | Read by |
| --- | --- | --- | --- |
| `check_cmd` | `TEXT`, nullable | launch (`insert_member`), **amend** (`Tx::set_member_scope`) | verify at the start of each run; verify again inside the write that stores the record (`scope-changed`) |
| `owns` | `TEXT`, JSON array of strings | launch, **amend** | verify (`Owns::new`), `teamwire teams --json`, launch's overlap check |

Rules:

- `amend --check CMD` sets `check_cmd` to `CMD`. `CMD` is not empty and not
  all spaces. A member with no check gets one.
- `amend --owns PATTERN ...` replaces the whole `owns` array with the given
  patterns, in the order given, each kept whole (no comma splitting).
- A field the command does not name keeps its value.
- `set_member_scope` writes no event, like the other member setters
  (`set_member_place`, `clear_member_branch`).

## Amend note (an existing `messages` row of type `note`)

| Field | Value |
| --- | --- |
| `sender` | `lead` |
| `type` | `note` |
| recipients | the amended role, one `deliveries` row |
| body, line 1 | `CHECK AMENDED: <reason>` |
| body, then | one line per changed field: `check: <old or (none)> -> <new>` and `owns: [<old patterns>] -> [<new patterns>]` |
| body, last | the instruction to run the new check and record the result again |
| `created_at` | the clock time of the transaction that also changed the scope |

The note and the scope change are written in one `Store::write`
transaction, through `teamwire_msg::send_with_write`. Delivery runs after
the commit.

Identification rule (used by `close --role`): the newest message whose
sender is `lead`, whose type is `note`, that has a delivery row for the
role, and whose body starts with `CHECK AMENDED:`.

## Verify record (existing `verify_records` row)

Unchanged columns. Two new readings:

- **Counts for close** only when its `recorded_at` is after the
  `created_at` of the role's newest amend note (strictly later) and verify
  raised no `scope-changed` on it. With no
  amend note, the rule of CLS-5 applies as before.
- **`scope-changed` flag**: the record step compares the `check_cmd` and
  `owns` it ran with against the row it re-reads in the storing
  transaction. When they differ, the flag `scope-changed` is in `flags`,
  and the verdict is `FLAGGED` (or `FAIL` when the check failed). The flag
  cannot be waived.

## Seat rules file (existing `<team dir>/contract/<role>.md`)

| Written by | Read by |
| --- | --- |
| launch (`write_contract`), **amend** (`rewrite_contract`) | each harness's `launch_spec` at launch, handoff and reopen |

`rewrite_contract` renders the whole file again with `render_contract`, from
the stored member's team, role, kind, check and owns, the team's report and
outbox paths, and the running binary's path. It writes with
`Platform::write_atomic` and mode `0644`, as launch does.

## State transitions

`amend` changes no member state. Its note follows the existing rules
(ADR 0043, REP-3):

| Member's newest result in its generation | Effect of the amend note |
| --- | --- |
| none, or `pending` (turn not ended) | typed into the session; no new generation |
| `done`, `blocked` or `failed` | wakes the member: `member.woken`, new generation, the watch tracks it |
| summary starts with `handoff:` | held for the fresh session; never typed |
