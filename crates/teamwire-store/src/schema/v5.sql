-- teamwire schema version 5: an AFK run is a team whose lead works with no
-- person present (ADR 0025). A value means the team is an AFK run, and the
-- value is when its wall-clock budget ends, as a UTC RFC 3339 string. Past
-- it, launch refuses new implementers. NULL means an ordinary team.

ALTER TABLE team ADD COLUMN afk_budget_ends_at TEXT;
