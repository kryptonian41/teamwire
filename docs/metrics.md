# Metrics and traces

Every number below is computed from the `events` table (ADR 0005).
`teamwire metrics` shows them locally; `teamwire metrics export` sends them
over OpenTelemetry (OTLP). Metric names use the `teamwire.` prefix and follow
the OpenTelemetry naming rules (dot-separated, units in the unit field).

## 1. Events (the source of every metric)

| Event type | Written when | Data fields |
| --- | --- | --- |
| `team.created`, `team.closing`, `team.closed` | launch starts; close starts; close ends | `title`, `harness`, `host`, `lead_route`; `outcome`, `failed_step` |
| `team.lead_adopted` | `teamwire lead --adopt` | `old_session`, `new_session` |
| `member.creating`, `member.started`, `member.failed` | each launch step | `kind`, `mode`, `reason`, `seconds` |
| `member.replaced` | a failed member's role is launched again | the failed attempt's `reason` and times |
| `member.reprompted` | a reprompt raises the generation | `generation`, `message` |
| `member.completed` | `done` stores a completion | `status`, `generation`, `n` |
| `member.completion_refused` | `done` refuses (invalid status, gaps on a hook-less harness) | `reason` |
| `member.held` | the report check or unread messages hold a member | `gaps`, `unread` |
| `member.removed` | removal | `unverified`, `stopped` |
| `message.sent`, `message.refused` | send stores or refuses | `type`, `sender_kind`, `recipients`, `chars`; `reason` |
| `delivery.handed_off`, `.unconfirmed`, `.undelivered` | outcome per recipient | `route`, `attempts`, `code`, `seconds` since `message.sent` |
| `delivery.acked`, `delivery.reminded` | the recipient reads; a hook reminds | `seconds` since `message.sent` |
| `delivery.resent` | an explicit resend starts a new attempt | `attempt`, `previous_state` |
| `result.delivering`, `.delivered`, `.undelivered`, `.unconfirmed` | a result's delivery steps | `route`, `code`, `seconds` since `member.completed` |
| `watch.started`, `watch.line`, `watch.stopped` | the watch starts, prints, exits | `events` (list of event names in the line) |
| `verify.recorded`, `verify.waived` | a verify record; a waiver | `verdict`, `flags`, `exit`, `seconds` |
| `close.step`, `close.refused` | each close step; a refusal | `step`, `outcome`, `reason` |
| `store.error` | a failed transaction | `op`, `code` |

## 2. Metrics

Labels are low-cardinality only (ADR 0005). Message ids, session ids and
paths never appear as labels.

| Name | Type | Unit | Labels | Question it answers |
| --- | --- | --- | --- | --- |
| `teamwire.lead.wakeups` | counter | `{wakeup}` | `team`, `source` (`result`, `question`, `watch`) | How many full lead turns did this team cost? The Python watch once burned 13.6M tokens on repeated lines. |
| `teamwire.watch.lines` | counter | `{line}` | `team` | Is the watch staying at "two lines for a healthy team"? |
| `teamwire.watch.events` | counter | `{event}` | `team`, `event` (`SILENT`, `DIED`, `LATE`, ...) | Which problems happen most? |
| `teamwire.messages.sent` | counter | `{message}` | `team`, `type`, `sender_kind` | Do members talk, and in which directions? |
| `teamwire.messages.refused` | counter | `{message}` | `team`, `type`, `reason` | Which rules do agents trip over? |
| `teamwire.deliveries` | counter | `{delivery}` | `team`, `route`, `outcome` (`handed_off`, `unconfirmed`, `undelivered`) | How reliable is each route? |
| `teamwire.deliveries.undelivered` | counter | `{delivery}` | `team`, `code` | Why do deliveries fail? |
| `teamwire.delivery.duration` | histogram | `s` | `team`, `route`, `outcome` | How long from store to hand-off? |
| `teamwire.delivery.attempts` | histogram | `{attempt}` | `team`, `route` | How often does the bounded retry fire? |
| `teamwire.messages.acked` | counter | `{message}` | `team`, `type` | Do recipients read what they get? |
| `teamwire.ack.duration` | histogram | `s` | `team`, `type` | How long before a message is read? |
| `teamwire.messages.unread` | gauge | `{message}` | `team`, `recipient_kind` | How many handed-off messages were never read? |
| `teamwire.completions` | counter | `{completion}` | `team`, `kind`, `status` | How many members finished done, blocked or failed? |
| `teamwire.completions.refused` | counter | `{completion}` | `team`, `reason` | How often is `done` called wrong? |
| `teamwire.results` | counter | `{result}` | `team`, `route`, `outcome` | Do results reach the lead? |
| `teamwire.result.duration` | histogram | `s` | `team`, `route` | How long from `done` to the lead's wake-up line? |
| `teamwire.holds` | counter | `{hold}` | `team`, `reason` (`progress_lines`, `decisions`, `report_missing`, `unread`) | How often does the report check fire, and for what? |
| `teamwire.member.run.duration` | histogram | `s` | `team`, `kind`, `status` | How long does a member generation take? |
| `teamwire.member.reprompts` | counter | `{reprompt}` | `team`, `kind` | How much extra work goes to finished members? |
| `teamwire.members.live` | gauge | `{member}` | `team`, `state` | How many members are running now? |
| `teamwire.launch.members` | counter | `{member}` | `team`, `outcome` (`started`, `refused`, `failed`), `reason` | Why do launches fail? |
| `teamwire.launch.duration` | histogram | `s` | `team`, `step` (`worktree`, `setup`, `spawn`, `trust`, `dispatch`) | Which launch step is slow? Read from `data.steps` (milliseconds per step) on `member.started` and `member.failed`. |
| `teamwire.locator.refusals` | counter | `{refusal}` | `team`, `code` | Is the wrong-session check firing, and why? |
| `teamwire.verify.runs` | counter | `{run}` | `team`, `verdict` | How often does verify pass, flag or fail? |
| `teamwire.verify.flags` | counter | `{flag}` | `team`, `flag` | Which weakening or scope flags appear? |
| `teamwire.verify.check.duration` | histogram | `s` | `team`, `verdict` | How long do member checks take? |
| `teamwire.close.runs` | counter | `{run}` | `outcome`, `failed_step` | Where do closes stop? |
| `teamwire.store.errors` | counter | `{error}` | `op`, `code` | Is the store healthy (busy, full, corrupt)? |
| `teamwire.hook.duration` | histogram | `s` | `hook` | What does a hook cost each member turn? Read from the `hook_ms` field that a hook run puts in the `data` of every event it writes; a hook run that writes no event is not measured. |
| `teamwire.export.lag` | gauge | `{event}` | none | How far behind is the OTLP export? |

`teamwire metrics` prints these per team, plus three derived lines the lead
asks most: wake-ups per member, the share of messages read, and the share of
results delivered on the first try.

## 3. Traces

One trace per team. The trace id is the first 16 bytes of SHA-256 of the
team's ULID; span ids come from event ULIDs. Spans are built from event pairs
after the fact, with their recorded start and end times.

| Span | Parent | Starts at | Ends at | Attributes |
| --- | --- | --- | --- | --- |
| `teamwire.team` | none | `team.created` | `team.closed` (open while the team lives) | `teamwire.team`, `teamwire.harness`, `teamwire.host` |
| `teamwire.launch` | `teamwire.team` | first event with a given `data.launch` | last event with that `data.launch` | `teamwire.members.requested`, `.started` |
| `teamwire.launch.member` | `teamwire.launch` | `member.creating` | `member.started` or `member.failed` | `teamwire.role`, `teamwire.kind`, `error.type` |
| `teamwire.member.generation` | `teamwire.team` | `member.started` or `member.reprompted` | `member.completed` | `teamwire.role`, `teamwire.generation`, `teamwire.status`, `session.id` |
| `teamwire.message` | `teamwire.team` | `message.sent` | last recipient's final delivery event | `teamwire.message.id`, `teamwire.message.type`, `teamwire.from` |
| `teamwire.delivery` | `teamwire.message` | `message.sent` | that recipient's outcome | `teamwire.to`, `teamwire.route`, `teamwire.outcome`, `teamwire.attempts`, `error.type` |
| `teamwire.result` | `teamwire.member.generation` | `member.completed` | `result.delivered` or `.undelivered` | `teamwire.route`, `teamwire.outcome` |
| `teamwire.verify.member` | `teamwire.team` | verify start | `verify.recorded` | `teamwire.role`, `teamwire.verdict`, `teamwire.head_sha` |
| `teamwire.close` | `teamwire.team` | `team.closing` | `team.closed` or `close.refused` | `teamwire.failed_step` |

`session.id` on `teamwire.member.generation` is the harness's own session id,
the same key Claude Code puts on its own telemetry, so the two can be joined.

## 4. Resource attributes

On teamwire's own export: `service.name=teamwire`, `service.version`,
`teamwire.team` per exported batch.

In each member's environment (OBS-4), appended to any existing value:
`OTEL_RESOURCE_ATTRIBUTES=...,teamwire.team=<team>,teamwire.role=<role>`.
Nothing per run or per message goes there.

## 5. Fields the event data carries for derived metrics

Decided by the lead on 2026-09-29, so that no event type was added after the
wave-1 contract:

- Every `member.creating`, `member.started` and `member.failed` written by one
  `teamwire launch` run carries `data.launch` (a ULID made once per run) and
  `data.requested` (the number of members in the spec). The
  `teamwire.launch` span runs from the first to the last event with that id.
- `member.started` and `member.failed` carry `data.steps`, the milliseconds
  each launch step took. `member.started` also carries `data.session_id`, the
  harness session id for the `session.id` attribute.
- A whole-spec refusal (exit 1) creates nothing, so it is not counted under
  `teamwire.launch.members` `outcome=refused`; only refusals of single
  members inside a launch that also stored something are.
- Every event a hook run writes carries `data.hook` (`claude-stop`,
  `claude-post-tool`) and `data.hook_ms`.
- A `member.reprompted` event with `data.undone: true` records an undone
  reprompt and is not counted as a reprompt. A `team.closed` event carries
  `data.outcome`; only `completed` closes the team.
