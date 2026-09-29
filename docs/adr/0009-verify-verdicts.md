# ADR 0009: Verify verdicts, owns patterns and Rust rules

Status: accepted, 2026-09-29.

## Context

`team-verify.py` records `verdict: PASS` whenever the check exits 0, even when
it raised flags (speclife F6). `team-close.py --role` trusts only that field,
so an implementer that deleted a test and passed its check can be removed
without `--unverified`. The record holds no HEAD sha, so commits made after
verification keep the old PASS. The weakening patterns know Python, JS/TS, Go
and Java, not Rust (F7), and teamwire is a Rust project. `owns` uses Python's
`fnmatch`, where `*` crosses `/` (speclife V3, LC-44).

speclife Q1 leaves the verdict change to the architect and the user: it
recommends the change, and notes that the user may want the old behaviour.

## Decision

1. **Five verdicts:**

   | Verdict | When |
   | --- | --- |
   | `PASS` | The check exited 0 and no flag is raised, or every raised flag is waived |
   | `FLAGGED` | The check exited 0 and at least one flag is not waived |
   | `FAIL` | The check exited non-zero (124 on timeout) |
   | `NO-CHECK` | A worktree member has no `check` |
   | `SKIPPED` | A removed member, or a pane member with no check |

2. **A waiver is explicit and recorded.** `teamwire verify <team> --role r
   --waive <flag-id> --reason "<text>"` stores the waiver with the lead's
   reason and the HEAD it applies to. A waiver does not carry over to a new
   HEAD.
3. **Every record stores the HEAD sha it verified.** Removing a member (and
   `close`) accepts a record only when its sha equals the member's HEAD now
   (VER-5, CLS-5).
4. **Rules are data.** Test-file patterns, weakening patterns and config-file
   names ship as one TOML file per language inside `teamwire-verify`, loaded at
   start. v1 ships Rust, Python, JS/TS, Go and Java (VER-4). The weakening
   patterns run only on added lines of files that match that language's
   source extensions, not on prose, which cuts the false positives speclife
   F7 noted.
5. **`owns` semantics:** a pattern ending in `/` matches everything under
   that directory. Any other pattern is a glob where `*` matches within one
   path segment and `**` matches across segments. This differs from Python on
   purpose, because `*` crossing `/` is easy to misread. Launch prints a
   warning for a pattern that contains `*` but not `**` and names a directory
   that has subdirectories.
6. **Check runs kill the whole process group on timeout** (VER-1).

## Consequences

- An implementer that weakened a test is `FLAGGED` until the lead reads the
  flag and waives it or has it fixed. That is stricter than Python. The user
  may prefer the old rule; it is listed under open questions, with this as the
  default.
- A member reprompted after verification must be verified again before
  removal.
- Specs written for the Python tool that relied on `*` crossing `/` need `**`.
  teamwire has no old specs, so nothing breaks today.
