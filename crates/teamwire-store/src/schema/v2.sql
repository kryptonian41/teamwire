-- teamwire schema version 2: a process is its pid plus its start time
-- (ADR 0002 decision 8). A pid is reused once its process exits, so a lock
-- that records the pid alone can be taken for a live owner's when an
-- unrelated process has the pid now. Each column holds the start time of
-- the pid beside it, as a UTC RFC 3339 string; NULL means the platform
-- could not read it, and the pid alone then decides, as in version 1.

ALTER TABLE team ADD COLUMN closing_pid_started_at TEXT;

ALTER TABLE metric_export ADD COLUMN claimed_by_started_at TEXT;
