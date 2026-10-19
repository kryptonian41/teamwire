-- teamwire schema version 5: a team records the tickets it works on
-- (ADR 0025), and a new event type, `team.refs_added`, records refs that a
-- later `launch --add` appended. The column holds a JSON array of
-- `{"provider": ..., "id": ...}` objects in first-recorded order; a team
-- launched without refs holds `[]`.
--
-- The `events.type` CHECK is a closed list and SQLite cannot change a CHECK
-- in place, so the table is rebuilt exactly as version 4 did: the old one is
-- renamed, the new one is created with the longer list, every row is copied
-- with its `seq`, and the `AUTOINCREMENT` counter is restored so that a `seq`
-- already given out is never given out again. Nothing else refers to
-- `events`, so the rename rewrites no other object. The migration runs inside
-- the one transaction that also sets `user_version`.

ALTER TABLE team ADD COLUMN refs TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(refs));

ALTER TABLE events RENAME TO events_old;

CREATE TABLE events (
    seq            INTEGER PRIMARY KEY AUTOINCREMENT,
    ulid           TEXT    NOT NULL UNIQUE,
    time           TEXT    NOT NULL,
    type           TEXT    NOT NULL CHECK (type IN (
        'team.created', 'team.closing', 'team.closed', 'team.lead_adopted',
        'team.refs_added',
        'member.creating', 'member.started', 'member.failed', 'member.replaced',
        'member.removed', 'member.reprompted', 'member.completed',
        'member.completion_refused', 'member.held',
        'message.sent', 'message.refused',
        'delivery.handed_off', 'delivery.unconfirmed', 'delivery.undelivered',
        'delivery.acked', 'delivery.reminded', 'delivery.resent',
        'delivery.reentered',
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

INSERT INTO events (seq, ulid, time, type, role, subject, data, writer_version)
    SELECT seq, ulid, time, type, role, subject, data, writer_version FROM events_old;

UPDATE sqlite_sequence
   SET seq = (SELECT seq FROM sqlite_sequence WHERE name = 'events_old')
 WHERE name = 'events'
   AND seq < (SELECT seq FROM sqlite_sequence WHERE name = 'events_old');

DROP TABLE events_old;

CREATE INDEX events_type ON events (type);
