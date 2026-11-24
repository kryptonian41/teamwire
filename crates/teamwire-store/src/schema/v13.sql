-- teamwire schema version 13: a sixth member kind, `reconciler`, reads the
-- reports of one set of readers and writes the one report the next step
-- acts on. Two new columns record what a member's spec said: `quiet` (1
-- makes a member of any kind quiet) and `set_label` (the set the member
-- belongs to, so a later `launch --add` can read the set's earlier
-- members). The columns go last, so every older column keeps its place.
--
-- The `members.kind` CHECK is a closed list and SQLite cannot change a
-- CHECK in place, so the table is rebuilt exactly as version 9 rebuilt it:
-- `members_new` is created with the longer list and the two new columns,
-- every row is copied column by column, the old table is dropped, and the
-- new one takes its name. A copied row takes the defaults: `quiet` 0 and
-- `set_label` NULL. The migration runs inside the one transaction that
-- also sets `user_version`, so a crash before the commit leaves version 12
-- untouched.
--
-- `completions`, `verify_records` and `waivers` refer to `members (role)`
-- by foreign key. Renaming `members_new`, not `members`, leaves those
-- references naming `members` throughout. `migrate` turns the checks off
-- before its transaction and runs `PRAGMA foreign_key_check` before the
-- commit, as for version 9.

CREATE TABLE members_new (
    role                 TEXT    PRIMARY KEY,
    title                TEXT    NOT NULL,
    kind                 TEXT    NOT NULL CHECK (kind IN ('implementer', 'reviewer', 'investigator', 'closer', 'plan_reviewer', 'reconciler')),
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
    removed_note         TEXT,
    quiet                INTEGER NOT NULL DEFAULT 0 CHECK (quiet IN (0, 1)),
    set_label            TEXT
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
