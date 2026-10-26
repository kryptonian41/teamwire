-- teamwire schema version 8: a new event type, `delivery.recipient_stopped`,
-- records that a member's turn ended while a sender still held a delivery
-- to it `pending`, so the turn-end hook left that message to the sender
-- (ADR 0028). The `events.type` CHECK is a closed list and SQLite cannot
-- change a CHECK in place, so the table is rebuilt exactly as versions 4, 6
-- and 7 rebuilt it: the old one is renamed, the new one is created with the
-- longer list, and every row is copied with its `seq`. The migration runs
-- inside the one transaction that also sets `user_version`, so a crash
-- before the commit leaves version 7 untouched.
--
-- Nothing else refers to `events` (no foreign key, trigger or view), so
-- the rename rewrites no other object. The `AUTOINCREMENT` counter lives in
-- `sqlite_sequence` under the table's name. The rename carries it to
-- `events_old`, the copy leaves it at the highest copied `seq`, and the
-- UPDATE restores the old value, which is higher when the newest rows were
-- deleted. A `seq` already given out, and already read by `metric_export`,
-- is never given out again.

ALTER TABLE events RENAME TO events_old;

CREATE TABLE events (
    seq            INTEGER PRIMARY KEY AUTOINCREMENT,
    ulid           TEXT    NOT NULL UNIQUE,
    time           TEXT    NOT NULL,
    type           TEXT    NOT NULL CHECK (type IN (
        'team.created', 'team.closing', 'team.closed', 'team.lead_adopted',
        'member.creating', 'member.started', 'member.failed', 'member.refused',
        'member.replaced',
        'member.removed', 'member.reprompted', 'member.handoff', 'member.completed',
        'member.completion_refused', 'member.held',
        'message.sent', 'message.refused',
        'delivery.handed_off', 'delivery.unconfirmed', 'delivery.undelivered',
        'delivery.acked', 'delivery.reminded', 'delivery.resent',
        'delivery.reentered', 'delivery.recipient_stopped',
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
