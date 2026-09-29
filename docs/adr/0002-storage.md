# ADR 0002: Storage — one SQLite database per team, with readable copies

Status: accepted, 2026-09-29.

## Context

The Python tooling keeps state in JSON and JSONL files. The manifest is
rewritten whole by three programs with no lock and no atomic rename
(speclife F1, F2). The message log derives ids from its line count, so one
corrupt line repeats an id (specmsg L2). The done file is written by the model
with a plain write, and a torn file is read as `done` (speclife F5).

Two proposals:

| | protocols: append-only JSON lines log with a lock file | rust: SQLite in WAL mode |
| --- | --- | --- |
| Crash safety | Torn-tail recovery must be written and tested by us | Tested by rust: `kill -9` mid-loop, `integrity_check ok`, every acknowledged row present |
| Many processes writing | `flock` plus careful appends | Built in: one writer, waits up to `busy_timeout` |
| Per-member updates | Rewrite or replay | One row per member, one transaction |
| Queries (unread per recipient, undelivered, metrics) | Rebuild an index or scan | SQL with indexes |
| Readable when teamwire is broken | `jq`, `tail` | `sqlite3` CLI, plus the readable copies below |

A second split, inside the SQLite option: one database for all teams, or one
per team (rust open question 2).

| | One database | One per team |
| --- | --- | --- |
| Cross-team metrics | One query | Iterate over files |
| A corrupt file | Takes every team down | Takes one team down |
| Archive and delete at close | Delete rows, export them | Back up the file, remove the directory |
| Write lock | Shared by all teams | Per team |

## Decision

1. **SQLite, WAL mode, one database per team** at
   `$TEAMWIRE_HOME/teams/<team>/team.db`. It holds members, messages,
   deliveries, completions, verify records, watch state and events. Metrics
   are computed from the events table.
2. Every connection sets `journal_mode=WAL`, `synchronous=FULL`,
   `foreign_keys=ON`, `busy_timeout=5000` (STO-2). Every write starts with
   `BEGIN IMMEDIATE` (STO-3). No write transaction stays open during a call to
   a host, harness, git or any other program (STO-4).
3. `rusqlite` with the `bundled` feature, so the SQLite version is fixed by
   the build, not by macOS.
4. The schema has `PRAGMA application_id` set to a fixed teamwire value and
   `PRAGMA user_version` set to the schema version. A binary migrates forward
   in one transaction. A binary that finds a newer version refuses, names both
   versions and exits with code 6, changing nothing (STO-6).
5. **Readable copies** sit beside the database: `comms/<display-id>.md` for
   each message body, `outbox/<role>.md` for each result, `reports/<role>.md`
   written by the member itself. teamwire writes its copies after the commit,
   through a temp file and a rename, and stores their SHA-256 in the database.
   `teamwire verify --repair-files` rewrites any copy that is missing or
   differs (STO-10).
6. No team index file. A member finds its team from `TEAMWIRE_TEAM` in its
   environment. The lead names the team on the command line. `teamwire status`
   with no team lists the directories under `$TEAMWIRE_HOME/teams/`.
7. Archive at close uses SQLite's backup API for the database and copies the
   other files, then writes `SHA256SUMS` and checks every file against it
   (CLS-2).
8. The close lock is a row in the database (`team.state = 'closing'` with the
   owner's pid and start time), not a lock file. Launch, reprompt and removal
   check that row inside their own write transactions (CLS-3). A row whose
   owner pid is dead is reported by `doctor` and can be cleared with
   `teamwire close --clear-stale-lock`.

## Consequences

- Stability: a crash at any instruction leaves the old or the new state,
  never a mix. Two versions can run at once as long as neither has migrated
  past the other; the one that is older refuses cleanly.
- Integrity: ids, sequence numbers and hashes live in one place and are
  checked there.
- Cross-team metrics mean opening every team database, including archived
  ones. With a handful of teams at a time this is cheap.
- A human can still read everything with `sqlite3` or the `.md` files. A plain
  `cp` of a live database can be corrupt, so docs and `doctor` point to
  `teamwire export` and the archive instead (STO-9).
- WAL needs a local disk. `doctor` warns when `TEAMWIRE_HOME` is on a network
  filesystem (STO-8).
- macOS does not use `F_FULLFSYNC` unless asked, so power loss can drop the
  last commits (rust 1.4). Process crash and `kill -9` lose nothing
  acknowledged. We accept that; `fullfsync` would slow every write.
