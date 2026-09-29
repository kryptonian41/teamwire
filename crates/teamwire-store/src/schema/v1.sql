-- teamwire schema version 1. Tables and columns: docs/ARCHITECTURE.md,
-- "Data model". Constraints that carry integrity: docs/api/core.md section 7.
-- Every enum column holds the lower-case string form of its teamwire-core
-- enum, and a CHECK lists exactly those values. Times are UTC RFC 3339
-- strings with milliseconds. Hashes are 32-byte SHA-256 BLOBs.

CREATE TABLE team (
    id               INTEGER PRIMARY KEY CHECK (id = 1),
    ulid             TEXT    NOT NULL UNIQUE,
    name             TEXT    NOT NULL,
    title            TEXT    NOT NULL,
    state            TEXT    NOT NULL CHECK (state IN ('open', 'closing', 'closed')),
    closing_pid      INTEGER,
    lead_session_id  TEXT,
    lead_harness     TEXT    NOT NULL,
    lead_host        TEXT    NOT NULL,
    lead_kind        TEXT    CHECK (lead_kind IN ('interactive', 'background')),
    lead_route       TEXT    NOT NULL CHECK (lead_route IN ('sink', 'watch')),
    sleep_hold_pid   INTEGER,
    sleep_hold_owned INTEGER CHECK (sleep_hold_owned IN (0, 1)),
    created_at       TEXT    NOT NULL
) STRICT;

CREATE TABLE members (
    role                 TEXT    PRIMARY KEY,
    title                TEXT    NOT NULL,
    kind                 TEXT    NOT NULL CHECK (kind IN ('implementer', 'reviewer', 'investigator', 'closer')),
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

CREATE TABLE messages (
    ulid        TEXT    PRIMARY KEY,
    display_id  TEXT    NOT NULL,
    sender      TEXT    NOT NULL,
    client_key  TEXT,
    seq         INTEGER NOT NULL CHECK (seq >= 1),
    type        TEXT    NOT NULL CHECK (type IN ('notice', 'finding', 'question', 'answer', 'reprompt', 'note')),
    re          TEXT    REFERENCES messages (ulid),
    commit_sha  TEXT,
    body        TEXT    NOT NULL,
    body_sha256 BLOB    NOT NULL CHECK (length(body_sha256) = 32),
    created_at  TEXT    NOT NULL
) STRICT;

CREATE UNIQUE INDEX messages_sender_key ON messages (sender, client_key);
CREATE UNIQUE INDEX messages_sender_seq ON messages (sender, seq);
CREATE UNIQUE INDEX messages_display ON messages (display_id);

CREATE TABLE deliveries (
    message_ulid TEXT    NOT NULL REFERENCES messages (ulid),
    recipient    TEXT    NOT NULL,
    state        TEXT    NOT NULL CHECK (state IN ('pending', 'handed_off', 'unconfirmed', 'undelivered', 'acked')),
    attempts     INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    route        TEXT    CHECK (route IN ('host_typing', 'harness_push', 'inbox', 'watch')),
    error_code   TEXT,
    error_detail TEXT,
    reminded_at  TEXT,
    acked_at     TEXT,
    PRIMARY KEY (message_ulid, recipient)
) STRICT;

CREATE INDEX deliveries_recipient_state ON deliveries (recipient, state);

CREATE TABLE completions (
    ulid             TEXT    PRIMARY KEY,
    role             TEXT    NOT NULL REFERENCES members (role),
    generation       INTEGER NOT NULL CHECK (generation >= 1),
    n                INTEGER NOT NULL CHECK (n >= 1),
    status           TEXT    NOT NULL CHECK (status IN ('done', 'blocked', 'failed')),
    summary          TEXT    NOT NULL,
    final_message    TEXT,
    final_source     TEXT    NOT NULL CHECK (final_source IN ('hook', 'transcript', 'none')),
    report_state     TEXT    NOT NULL CHECK (report_state IN ('pending', 'delivering', 'delivered', 'undelivered', 'unconfirmed')),
    delivering_since TEXT,
    route            TEXT    CHECK (route IN ('host_typing', 'harness_push', 'inbox', 'watch')),
    reason           TEXT,
    reason_detail    TEXT,
    result_sha256    BLOB    CHECK (result_sha256 IS NULL OR length(result_sha256) = 32),
    created_at       TEXT    NOT NULL
) STRICT;

CREATE UNIQUE INDEX completions_role_n ON completions (role, n);

CREATE TABLE verify_records (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    role        TEXT    NOT NULL REFERENCES members (role),
    head_sha    TEXT    NOT NULL,
    verdict     TEXT    NOT NULL,
    exit_code   INTEGER,
    seconds     INTEGER,
    flags       TEXT    NOT NULL DEFAULT '[]' CHECK (json_valid(flags)),
    log_sha256  BLOB    CHECK (log_sha256 IS NULL OR length(log_sha256) = 32),
    recorded_at TEXT    NOT NULL
) STRICT;

CREATE TABLE waivers (
    role       TEXT NOT NULL REFERENCES members (role),
    flag_id    TEXT NOT NULL,
    head_sha   TEXT NOT NULL,
    reason     TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (role, flag_id, head_sha)
) STRICT;

CREATE TABLE watch_lease (
    id           INTEGER PRIMARY KEY CHECK (id = 1),
    pid          INTEGER NOT NULL,
    host         TEXT    NOT NULL,
    started_at   TEXT    NOT NULL,
    heartbeat_at TEXT    NOT NULL
) STRICT;

CREATE TABLE announcements (
    key TEXT PRIMARY KEY,
    at  TEXT NOT NULL
) STRICT;

CREATE TABLE counters (
    name  TEXT    PRIMARY KEY,
    value INTEGER NOT NULL CHECK (value >= 0)
) STRICT;

CREATE TABLE events (
    seq            INTEGER PRIMARY KEY AUTOINCREMENT,
    ulid           TEXT    NOT NULL UNIQUE,
    time           TEXT    NOT NULL,
    type           TEXT    NOT NULL CHECK (type IN (
        'team.created', 'team.closing', 'team.closed', 'team.lead_adopted',
        'member.creating', 'member.started', 'member.failed', 'member.replaced',
        'member.removed', 'member.reprompted', 'member.completed',
        'member.completion_refused', 'member.held',
        'message.sent', 'message.refused',
        'delivery.handed_off', 'delivery.unconfirmed', 'delivery.undelivered',
        'delivery.acked', 'delivery.reminded', 'delivery.resent',
        'result.delivering', 'result.delivered', 'result.undelivered', 'result.unconfirmed',
        'watch.started', 'watch.line', 'watch.stopped',
        'verify.recorded', 'verify.waived',
        'close.step', 'close.refused',
        'store.error')),
    role           TEXT,
    subject        TEXT,
    data           TEXT    NOT NULL DEFAULT '{}' CHECK (json_valid(data)),
    writer_version TEXT    NOT NULL
) STRICT;

CREATE INDEX events_type ON events (type);

CREATE TABLE metric_export (
    id             INTEGER PRIMARY KEY CHECK (id = 1),
    last_event_seq INTEGER NOT NULL DEFAULT 0 CHECK (last_event_seq >= 0),
    claimed_to     INTEGER,
    claimed_by     TEXT,
    claimed_at     TEXT
) STRICT;
