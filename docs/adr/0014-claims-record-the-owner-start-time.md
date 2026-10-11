# ADR 0014: The metric export claim records the claimer's start time

Status: accepted, 2026-10-11. Supersedes the claim `UPDATE` in ADR 0005,
decision 3.1, which records `claimed_by = :pid` alone.

## Context

ADR 0002 decision 8 says a lock records its owner's pid and start time, so a
pid that the operating system has given to a new process is not taken for
the owner. Schema version 1 stored the pid alone for both the close lock and
the metric export claim. A review on 2026-10-11 found that a dead close or
exporter whose pid was reused by a live process kept its lock for as long as
that process lived.

## Decision

1. Schema version 2 adds `team.closing_pid_started_at` and
   `metric_export.claimed_by_started_at`.
2. The claim becomes `UPDATE metric_export SET claimed_to = :to,
   claimed_by = :pid, claimed_by_started_at = :started, claimed_at = :now
   WHERE last_event_seq = :mark AND claimed_to IS NULL`.
3. A claim or close lock belongs to a live owner only when its pid is alive
   and, when both start times are known, the process now holding that pid
   started within 2 s of the recorded time (`START_TIME_SLACK`).
4. When the platform cannot read a start time, the column is NULL and the
   pid alone decides, as in version 1.

## Consequences

- A stale close lock or export claim is cleared even when its pid was
  reused.
- A binary built before version 2 refuses a version-2 database, with the
  existing "newer than this binary" error.
- On Linux, start times are derived from the boot time, so a wall-clock step
  larger than 2 s can make a live owner look gone. macOS stores the start
  time itself.
