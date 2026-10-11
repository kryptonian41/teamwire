# ADR 0015: A read-only open reads an older schema as it is

Status: accepted, 2026-10-11. Supersedes the part of WAT-11 and of
`docs/api/core.md` (`Store::open_read_only`) that refuses a database whose
schema is older than the binary's. Adds the effect on archives that ADR 0014
left out.

## Context

Schema version 2 (ADR 0014) was the first migration. A read-only open
refused every database at an older schema, and nothing ever opens an archive
read-write, so nothing migrated an archive. `teamwire metrics` then skipped
every archive made before version 2, and `status`, `doctor`, `export` and the
MCP `team_status` tool refused a live team until a hook or the watch wrote to
it. Archives must not be written: each has a `SHA256SUMS` file that close
verified.

## Decision

1. A read-only open accepts any older schema it knows. It never migrates and
   never writes.
2. For each column a later version added, the open creates a TEMP view named
   like the table, with `NULL AS <column>`. SQLite searches the temp schema
   before `main`, so every reader runs its usual query. `temp_store = MEMORY`
   keeps the views off the disk. The list of added columns lives in
   `crates/teamwire-store/src/older.rs`.
3. A test reads every older version through a read-only open and checks
   every current column is present. A new schema version does not compile
   until its added columns are listed.
4. A newer schema is still refused with both versions named. An empty file
   is still refused.
5. A read-write open still migrates forward, as before.

## Consequences

- Metrics cover every archive again, and read-only commands work on a live
  team at an older schema. `doctor` reports such a team as passing, with a
  note that the next write migrates it.
- Every later migration that adds a column must add it to `older.rs`; the
  test enforces that.
- A migration that changes a column's meaning, rather than adding one, needs
  its own read path. None exists today.
