# Interface: `teamwire amend`

## Synopsis

```text
teamwire amend <team> --role R [--check CMD] [--owns PATTERN]... --reason TEXT
```

A lead command. It changes one started member's check, owns or both, sends
the member a `CHECK AMENDED:` note in the same store transaction, rewrites
the member's seat rules file, and prints what changed.

## Arguments

| Argument | Required | Meaning |
| --- | --- | --- |
| `<team>` | yes | The team name. |
| `--role R` | yes | The member to amend. |
| `--check CMD` | one of `--check`, `--owns` | The new check command, replacing the old one. |
| `--owns PATTERN` | one of `--check`, `--owns`; repeatable | One owns pattern per flag. All given patterns replace the old list. A value is never split on commas. |
| `--reason TEXT` | yes | Why. Goes into the note's first line. |

## Exit codes

| Code | When |
| --- | --- |
| 0 | Scope saved, note stored and delivered (or held for a handoff), seat rules rewritten. |
| 1 | Refused, nothing changed: `refused.identity` (run from a member session), unknown role, member not `started`, team not open. |
| 2 | Partial: scope and note saved, but the seat rules file could not be rewritten. Run the same amend again. Exit 2 wins over 4 and 5: when the rewrite fails and the note is also undelivered or unconfirmed, the exit is 2, because a wrong seat rules file misleads every later handoff. |
| 3 | Usage: no `--role`, no `--reason`, neither `--check` nor `--owns`, an empty or all-space `--reason` or `--check`, an empty `--owns` pattern (`--owns ''`; `amend` cannot clear the owns list), a bad team or role name. |
| 4 | Scope and note saved; the note is `undelivered`. |
| 5 | Scope and note saved; the note is `unconfirmed`. |
| 6 | Store error; nothing changed. |

## Output (stdout, exit 0)

```text
amended api in team alpha: check (note m007)
  check: cargo test -p teamwire-store -> cargo test -p teamwire-store --test member
run `teamwire verify alpha --role api` again: close --role counts only a verify recorded after this amend
```

With `--owns`, an `owns:` line shows the old and new lists.

On exit 4 or 5 the scope is saved. The scope line and the field lines print
first, then the delivery line `send` prints for that recipient, then the
verify line:

```text
amended api in team alpha: owns (note m007)
  owns: [src/] -> [src/, crates/{b,c}/**]
api: undelivered (no_live_process)
run `teamwire verify alpha --role api` again: close --role counts only a verify recorded after this amend
```

On exit 2 the same stdout prints, with the delivery line when the note was
not delivered, and stderr adds:

```text
error: amend: the seat rules file <home>/teams/alpha/contract/api.md could not be rewritten (<error>); the check and owns are saved; run the same amend again
```

## The note (body)

```text
CHECK AMENDED: the old check ran every test file
The lead changed your scope.
check: cargo test -p teamwire-store -> cargo test -p teamwire-store --test member
Verify now runs the new check. Run it yourself, then record your result again with teamwire done.
```

An `owns:` line, `owns: [crates/a/] -> [crates/a/, crates/b/]`, replaces or
follows the `check:` line when owns changed. A check that was not set shows
as `(none)`.

## `close --role` refusal added (exit 1)

```text
api's verify at HEAD 1a2b3c4 was recorded before the lead amended its check or owns (m007); run teamwire verify alpha --role api again
```

`--unverified` overrides it, as it overrides every CLS-5 reason.

## Verify flag added

| Flag | Raised when | Waivable |
| --- | --- | --- |
| `scope-changed` | the member's stored check or owns differ, when verify stores its record, from the values verify ran with | no |

## Not offered

- No MCP tool. `team_*` tools stay the ten of `docs/api/mcp.md`.
- No `--json`.
- No `--dry-run`. The command's effect is one transaction and a file
  rewrite; the output shows both.
