-- teamwire schema version 9: a fifth member kind, `plan_reviewer`, reads
-- the lead's plan and every brief before any implementer starts (ADR 0034).
-- The `members.kind` CHECK is a closed list and SQLite cannot change a
-- CHECK in place, so the table is rebuilt: `members_new` is created with
-- the longer list and otherwise exactly as version 1 created `members`,
-- every row is copied column by column, the old table is dropped, and the
-- new one takes its name. The migration runs inside the one transaction
-- that also sets `user_version`, so a crash before the commit leaves
-- version 8 untouched.
--
-- This is not the `events` rebuild of versions 4, 6, 7 and 8, which renames
-- the old table first. `completions`, `verify_records` and `waivers` refer
-- to `members (role)` by foreign key, and renaming `members` rewrites those
-- references to follow the old table, which is then dropped. Renaming
-- `members_new` instead leaves them naming `members` throughout. Dropping
-- `members` while their rows still name it would fail with foreign key
-- checks on, so `migrate` turns the checks off before its transaction and
-- runs `PRAGMA foreign_key_check` before the commit.

CREATE TABLE members_new (
    role                 TEXT    PRIMARY KEY,
    title                TEXT    NOT NULL,
    kind                 TEXT    NOT NULL CHECK (kind IN ('implementer', 'reviewer', 'investigator', 'closer', 'plan_reviewer')),
    mode                 TEXT    NOT NULL CHECK (mode IN ('worktree', 'pane')),
    harness              TEXT    NOT NULL,
    host                 TEXT    NOT NULL,
    place                TEXT,
    session_id           TEXT,
    token_sha256         BLOB    CHECK (token_sha256 IS NULL OR length(token_sha256) = 32),
    cwd                  TEXT    NOT NULL,
    repo                 TEXT,
    branch               TEXT,
    base                 TEXT,
    base_sha             TEXT,
    check_cmd            TEXT,
    owns                 TEXT    NOT NULL DEFAULT '[]' CHECK (json_valid(owns)),
    deadline_s           INTEGER NOT NULL CHECK (deadline_s > 0),
    state                TEXT    NOT NULL CHECK (state IN ('creating', 'started', 'failed', 'removed')),
    generation           INTEGER NOT NULL CHECK (generation >= 1),
    gen_started_at       TEXT    NOT NULL,
    gate_told_generation INTEGER,
    removed_at           TEXT,
    removed_note         TEXT
) STRICT;

INSERT INTO members_new (role, title, kind, mode, harness, host, place, session_id,
        token_sha256, cwd, repo, branch, base, base_sha, check_cmd, owns, deadline_s,
        state, generation, gen_started_at, gate_told_generation, removed_at, removed_note)
    SELECT role, title, kind, mode, harness, host, place, session_id,
        token_sha256, cwd, repo, branch, base, base_sha, check_cmd, owns, deadline_s,
        state, generation, gen_started_at, gate_told_generation, removed_at, removed_note
    FROM members;

DROP TABLE members;

ALTER TABLE members_new RENAME TO members;
