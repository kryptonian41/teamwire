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
3. `teamwire metrics export` reads events after the `metric_export` mark,
   records them into OTel instruments, force-flushes, and only then advances
   the mark in a transaction. `teamwire watch` runs the same step on a
   background thread every 60 s when an endpoint is set, so a slow collector
   never delays a watch line. A crash between flush and mark re-exports that
   batch once. We accept rare duplicates over loss.
4. Export is off unless `OTEL_EXPORTER_OTLP_ENDPOINT` is set (or the same key
   in `$TEAMWIRE_HOME/config.toml`). A missing collector never changes an exit
   code (OBS-2).
5. Traces are rebuilt from events with their recorded start and end times.
   The trace id for a team is the first 16 bytes of SHA-256 of the team's
   ULID, so every export run produces the same trace id for the same team.
   Span ids come from the event ULIDs the same way. Spans are listed in
   `docs/metrics.md`.
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
8. Crates: `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp` pinned
   to one 0.33.x, with the HTTP/protobuf exporter and a blocking client, so
   export needs no async runtime. `tracing` is used for teamwire's own logs,
   written to stderr. `tracing-opentelemetry` is not needed in v1, because
   spans are built from events, not from live `tracing` spans.

## Consequences

- Metrics cost one extra row per state change, inside a transaction that
  already exists.
- The local store answers every metric question with no network.
- An OTel crate bump touches one crate, `teamwire-otel`.
- A hosted backend needs the user's account. teamwire ships no default
  endpoint, and choosing one is an open question for the user.
