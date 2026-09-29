# ADR 0005: Metrics and OpenTelemetry

Status: accepted, 2026-09-29.

## Context

The brief fixes that metrics are stored locally, shown by the CLI, and
exported over OpenTelemetry (OTel). Metrics rank third, after stability and
message integrity.

Facts from the reports:

- teamwire commands are short-lived. An OTel batch exporter in a command that
  exits in 50 ms may never flush (rust 1.10).
- The Rust OTel crates are 0.33.x, pre-1.0. The metrics API and SDK are
  stable, the OTLP metrics exporter is a release candidate, traces are beta
  (rust 1.5, 1.10). `tracing-opentelemetry` 0.34 must move with them.
- Claude Code exports its own metrics and events when told to, and copies
  `OTEL_RESOURCE_ATTRIBUTES` onto every signal. Resource attributes become
  metric labels, so a high-cardinality value there is expensive (harness 1.6).
- protocols R13 wants messages and lifecycle events in one log, so metrics can
  be recomputed from the log alone.

Options:

| | A. Commands export OTel directly | B. Store first, export from the store |
| --- | --- | --- |
| Short-lived commands | Must flush before exit; a slow collector slows every command | Commands never talk to a collector |
| Collector down | Data lost or commands delayed | Data waits in the store |
| Local metrics without a collector | Needs a second path | The same rows |
| Duplicates | None | Possible if the exporter crashes between send and mark |

## Decision

**B. The events table is the source of truth; OTel export is a separate step.**

1. Every state change writes an event row in the same transaction as the
   change (OBS-1). Event types are listed in `docs/metrics.md`.
2. `teamwire metrics` computes counts, rates and durations from the event rows
   with SQL, per team or across every team and archive, with no collector.
3. `teamwire metrics export` and the watch's background thread (every 60 s
   when an endpoint is set, so a slow collector never delays a watch line)
   run the same step, and two of them may run at once. Each export:
   1. **Claims** a batch with one conditional update:
      `UPDATE metric_export SET claimed_to = :to, claimed_by = :pid,
      claimed_at = :now WHERE last_event_seq = :mark AND claimed_to IS NULL`.
      Zero rows changed means another exporter holds the batch; this one
      exits with nothing sent. A claim older than 5 minutes whose pid is dead
      may be taken over.
   2. Sends the batch.
   3. Commits `last_event_seq = claimed_to, claimed_to = NULL` in one
      transaction. A crash between 2 and 3 re-exports that batch once, after
      the claim expires. Two live exporters never send the same batch.
4. **Metric points carry event times, as deltas.** Events are grouped into
   60-second buckets by their own `time`, per metric and label set, and each
   bucket is sent as one delta data point (`start_time` and `time` are the
   bucket bounds). A backlog sent after a collector outage therefore keeps
   its real times, and an exporter process that starts fresh never looks like
   a counter reset. Cumulative totals are the backend's job.
4a. Export is off unless `OTEL_EXPORTER_OTLP_ENDPOINT` is set (or the same key
   in `$TEAMWIRE_HOME/config.toml`). A missing collector never changes an exit
   code (OBS-2).
5. Traces are rebuilt from events with their recorded start and end times.
   The trace id for a team is the first 16 bytes of SHA-256 of the team's
   ULID, so every export run produces the same trace id for the same team.
   Span ids come from the event ULIDs the same way. A span is sent when its
   **end** event is in the batch; its start event is looked up by id in the
   whole events table, not only in the batch, so a span whose start went out
   in an earlier batch is still whole. A span whose end has not happened yet
   is not sent. Parent span ids are computed, so a child can be sent before
   its parent. Spans are listed in `docs/metrics.md`.
6. Labels on metrics are low-cardinality only: team, role, kind, type,
   state, route, reason, harness, host. Message ids and session ids go on
   spans and events, never on metric labels.
7. At launch each member's environment gets `teamwire.team=<team>` and
   `teamwire.role=<role>` appended to any existing `OTEL_RESOURCE_ATTRIBUTES`,
   and, when export is on, the same `OTEL_EXPORTER_OTLP_*` values teamwire
   uses. teamwire does not turn on the harness's own telemetry; the user does
   that with the harness's own switch (for Claude,
   `CLAUDE_CODE_ENABLE_TELEMETRY=1`). The member's harness session id is
   stored on teamwire's spans, so the two can be joined (OBS-4).
8. Crates: the exporter builds OTLP/HTTP protobuf requests itself with
   `opentelemetry-proto` (pinned to the same 0.33.x family, features for
   metrics and traces) and `prost`, and posts them with a blocking HTTP client
   (`ureq`), so export needs no async runtime. The SDK's in-process
   instruments are not used: they stamp points with the export time and keep
   cumulative state per process, which is the wrong shape for points rebuilt
   from stored events. `tracing` is used for teamwire's own logs,
   written to stderr. `tracing-opentelemetry` is not needed in v1, because
   spans are built from events, not from live `tracing` spans.

## Consequences

- Metrics cost one extra row per state change, inside a transaction that
  already exists.
- The local store answers every metric question with no network.
- An OTel crate bump touches one crate, `teamwire-otel`.
- A hosted backend needs the user's account. teamwire ships no default
  endpoint, and choosing one is an open question for the user.
