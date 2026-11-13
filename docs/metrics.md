# Metrics and traces

Every number below is computed from the `events` table (ADR 0005).
`teamwire metrics` shows them locally; `teamwire metrics export` sends them
over OpenTelemetry (OTLP). Metric names use the `teamwire.` prefix and follow
the OpenTelemetry naming rules (dot-separated, units in the unit field).

## 1. Events (the source of every metric)

| Event type | Written when | Data fields |
| --- | --- | --- |
| `team.created`, `team.closing`, `team.closed` | launch starts; close starts; close ends | `title`, `harness`, `host`, `lead_route`, `refs`; `outcome`, `failed_step` |
| `team.lead_adopted` | `teamwire lead --adopt` | `old_session`, `new_session` (null in an export taken from a member session) |
| `team.refs_added` | `launch --add` names a ticket ref the team does not hold yet (records no metric) | `added` (the refs appended, in order) |
| `member.creating`, `member.started`, `member.failed` | each launch step | `kind`, `mode`, `task_class` (creating: one of the seven classes of `TaskClass`, or null when the spec set none; also copied onto the same launch's `started`, `failed` and `refused` events); `launch`, `requested`, `dispatch`, `steps`; `reason`, `reason_code` (failed); `model`, `effort`, `plugins`, `context_handoff_tokens` (a number, or null when unset), `excluded`, `session_id` (started) |
| `member.refused` | launch turns a member away (AFK budget spent, team closing) | `reason`, `reason_code` (`afk_budget_spent`, `team_closed`), `launch`, `requested`, `dispatch`, `steps` |
| `member.replaced` | a failed member's role is launched again | the failed attempt's `reason` and times |
| `member.reprompted` | a reprompt raises the generation | `generation`, `message` |
| `member.woken` | a message other than a reprompt raises the generation of a finished member (ADR 0043); not a reprompt, so no reprompt metric counts it | `generation`, `message`; an undo after a known delivery failure adds `undone: true` |
| `member.handoff` | `teamwire handoff` gives a member a fresh session (ADR 0027) | `generation` (the new one), `old_session`, `new_session` (null when the harness picks its own id), `place` |
| `member.completed` | `done` stores a completion | `status`, `generation`, `n` |
| `member.completion_refused` | `done` refuses (invalid status, gaps on a hook-less harness) | `reason` |
| `member.held` | the report check or unread messages hold a member | `gaps`, `unread` |
| `member.removed` | removal | `unverified`, `stopped` |
| `message.sent`, `message.refused` | send stores or refuses | `type`, `sender_kind` (`lead`, `member`, `user`, `other_team`), `recipients`, `chars`; `reason` |
| `delivery.handed_off`, `.unconfirmed`, `.undelivered` | outcome per recipient | `route`, `attempts`, `code`, `seconds` since `message.sent` |
| `delivery.acked`, `delivery.reminded` | the recipient reads; a hook reminds | `seconds` since `message.sent` |
| `delivery.resent` | an explicit resend, or a send resumed by its key, starts a new attempt | `attempt`, `previous_state` |
| `delivery.reentered` | tmux delivery pressed Enter a second time because the pasted line was still in the input box | `to` (`lead` or the role), `place` (the recipient's place id, such as `%4@/private/tmp/tmux-501/default`); the `role` column is the member's role, empty for the lead |
| `delivery.recipient_stopped` | a member's turn ended while a sender still held its delivery `pending`; the sender reads it before it records `undelivered` (ADR 0028) | `to` (the role); the `role` column is the member's role, the subject the message |
| `result.delivering`, `.delivered`, `.undelivered`, `.unconfirmed` | a result's delivery steps | `route`, `code`, `seconds` since `member.completed` |
| `watch.started`, `watch.line`, `watch.stopped` | the watch starts, prints, exits | `pid`, `host` (started); `events` (list of event names in the line); `reason`, `polls` (stopped) |
| `verify.recorded`, `verify.waived` | a verify record; a waiver | `verdict`, `flags`, `exit`, `seconds`, `head_sha` (recorded) |
| `close.step`, `close.refused` | each close step; a refusal | `step`, `outcome`, `reason` |
| `store.error` | a failed transaction with code busy, full, corrupt or io (a write, or a read of a store opened for writing), written after the rollback by the command, hook, MCP tool or watch poll that hit it; never for a read-only store or a refused schema. A delivery to another team that opens that team's store records its failure there, with `op` `send` or `resend`. A failure while a store is being opened writes no row, because an open that fails has no connection, an unreadable header, or a file not yet at this binary's schema, which `record_failure` never writes to, and a busy header read means the database stayed locked past the 5 s busy timeout, so a second write would fail the same way; the failure is still reported where it happens | `op` (the command, MCP tool, `turn_end`, `post_tool`, `session_start`, `watch`, `send` or `resend`), `code` |

The `close.step` rows in an archived database end at `finish`: `archive_final` and `delete_state` run after the archive is copied.

## 2. Metrics

Labels are low-cardinality only (ADR 0005). Message ids, session ids and
paths never appear as labels.

The `task_class` label is the class launch wrote on the member's latest
`member.creating` event. A member with no class, or with a value that is not
one of the seven classes, gets `unclassified`. Every team launched before
task classes existed reads as `unclassified`. The label therefore has at most
eight values.

| Name | Type | Unit | Labels | Question it answers |
| --- | --- | --- | --- | --- |
| `teamwire.lead.wakeups` | counter | `{wakeup}` | `team`, `source` (`result`, `question`, `watch`) | How many full lead turns did this team cost? An earlier watch once burned 13.6M tokens on repeated lines. |
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
| `teamwire.completions` | counter | `{completion}` | `team`, `kind`, `status`, `task_class` | How many members finished done, blocked or failed? |
| `teamwire.completions.refused` | counter | `{completion}` | `team`, `reason` | How often is `done` called wrong? |
| `teamwire.results` | counter | `{result}` | `team`, `route`, `outcome` | Do results reach the lead? |
| `teamwire.result.duration` | histogram | `s` | `team`, `route` | How long from `done` to the lead's wake-up line? |
| `teamwire.holds` | counter | `{hold}` | `team`, `reason` (`progress_lines`, `decisions`, `report_missing`, `unread`) | How often does the report check fire, and for what? |
| `teamwire.member.run.duration` | histogram | `s` | `team`, `kind`, `status` | How long does a member generation take? |
| `teamwire.member.reprompts` | counter | `{reprompt}` | `team`, `kind` | How much extra work goes to finished members? |
| `teamwire.member.tokens` | counter | `{token}` | `team`, `role`, `kind`, `model`, `effort`, `task_class`, `type` (`input`, `output`, `cache_read`, `cache_creation`) | How many tokens does each member use? Read from `data.usage` on `result.delivering`, which holds the session's running totals. Each event counts what it adds to the previous one of the same `data.session`, so the sum is the newest totals. `effort` is the member's effort from its latest `member.started` event, or `unset` when none was recorded. Subagent transcripts, in `<session-id>/subagents/*.jsonl` beside the main transcript, are counted with it, once per `message.id`. A pi member's reading comes from its pi session file, every assistant message once, with all four types. A Codex member's comes from the newest `total_token_usage` in its rollout: `input` leaves out the cached tokens, which are `cache_read`; `output` includes reasoning; `cache_creation` is always 0, because Codex reports no cache writes; and a session that changed model counts every token under its newest model. A team whose lead route is the watch (host `none`) records no tokens. |
| `teamwire.members.live` | gauge | `{member}` | `team`, `state` | How many members are running now? |
| `teamwire.launch.members` | counter | `{member}` | `team`, `outcome` (`started`, `failed`, `refused`; a refusal of the whole spec writes no event), `reason` (the event's `reason_code`: for `failed` a step name, `registry`, `prepare`, `record`, `worktree`, `setup`, `harness`, `files`, `spawn`, `place`, `trust`, `started`; for `refused` `afk_budget_spent` or `team_closed`; else `other`; `none` when started) | Why do launches fail? |
| `teamwire.launch.duration` | histogram | `s` | `team`, `step` (`worktree`, `setup`, `spawn`, `trust`, `dispatch`) | Which launch step is slow? Read from `data.steps` (milliseconds per step) on `member.started`, `member.failed` and `member.refused`. |
| `teamwire.locator.refusals` | counter | `{refusal}` | `team`, `code` | Is the wrong-session check firing, and why? |
| `teamwire.verify.runs` | counter | `{run}` | `team`, `verdict` | How often does verify pass, flag or fail? |
| `teamwire.verify.flags` | counter | `{flag}` | `team`, `flag` (the flag's kind, the id before its first `:`, so no path or sha is a label) | Which weakening or scope flags appear? |
| `teamwire.verify.check.duration` | histogram | `s` | `team`, `verdict` | How long do member checks take? |
| `teamwire.close.runs` | counter | `{run}` | `outcome`, `failed_step` | Where do closes stop? |
| `teamwire.store.errors` | counter | `{error}` | `op`, `code` | Is the store healthy (busy, full, corrupt, io)? |
| `teamwire.hook.duration` | histogram | `s` | `hook` | What does a hook cost each member turn? Read from the `hook_ms` field that a hook run puts in the `data` of the last event it writes; a hook run that writes no event is not measured. |
| `teamwire.export.lag` | gauge | `{event}` | none | How far behind is the OTLP export? It counts the events still behind the export mark, after the send, in the local view and in the export alike. |

`teamwire metrics` prints these per team, plus three derived lines the lead
asks most: wake-ups per member, the share of messages read, and the share of
results delivered on the first try.

With a team name, a team that is closed is read from its newest archive under
`$TEAMWIRE_HOME/archive/`, found by the name in the archive's team row. A team
never seen is refused.

## 3. Traces

One trace per team. The trace id is the first 16 bytes of SHA-256 of the
team's ULID; span ids come from event ULIDs. Spans are built from event pairs
after the fact, with their recorded start and end times.

| Span | Parent | Starts at | Ends at | Attributes |
| --- | --- | --- | --- | --- |
| `teamwire.team` | none | `team.created` | `team.closed` (open while the team lives) | `teamwire.team`, `teamwire.harness`, `teamwire.host` |
| `teamwire.launch` | `teamwire.team` | first event with a given `data.launch` | the event that makes every requested member started, failed or refused | `teamwire.members.requested`, `.started` |
| `teamwire.launch.member` | `teamwire.launch` | `member.creating` of the same launch | `member.started`, `member.failed` or `member.refused` | `teamwire.role`, `teamwire.kind`, `error.type` |
| `teamwire.member.generation` | `teamwire.team` | `member.started`, `member.reprompted` or `member.handoff` | `member.completed` | `teamwire.role`, `teamwire.generation`, `teamwire.status`, `session.id` |
| `teamwire.message` | `teamwire.team` | `message.sent` | last recipient's final delivery event | `teamwire.message.id`, `teamwire.message.type`, `teamwire.from` (the event's `data.from`, else the sender's role, else `lead`; a message from another team shows `role@team`) |
| `teamwire.delivery` | `teamwire.message` | `message.sent` | that recipient's outcome | `teamwire.to`, `teamwire.route`, `teamwire.outcome`, `teamwire.attempts`, `error.type` |
| `teamwire.result` | `teamwire.member.generation` | `member.completed` | `result.delivered` or `.undelivered` | `teamwire.route`, `teamwire.outcome` |
| `teamwire.verify.member` | `teamwire.team` | verify start | `verify.recorded` | `teamwire.role`, `teamwire.verdict`, `teamwire.head_sha` |
| `teamwire.close` | `teamwire.team` | `team.closing` | `team.closed` or `close.refused` | `teamwire.failed_step` |

`session.id` on `teamwire.member.generation` is the harness's own session id,
the same key Claude Code puts on its own telemetry, so the two can be joined.

## 4. Resource attributes

On teamwire's own export: `service.name=teamwire`, `service.version`,
`teamwire.team` per exported batch.

An export claims the events up to the first unsent event whose 60-second
bucket has not ended. That event and the ones after it wait for a later
export, so one bucket of one series is always one delta point (ADR 0005,
point 4). An event's time is read before its write waits for the lock, so a
writer that waits across a minute's end can add a second point for that
minute. While the watch runs, each export request has the whole configured
timeout. Once the watch stops, a request still waiting gets at most 3 s more,
so the watch exits within about 6 s: an export makes up to two requests, traces then metrics, and each gets its own 3 s. A batch cut off that way stays unsent
until the next `teamwire metrics export` or watch.

`OTEL_EXPORTER_OTLP_HEADERS` is sent only to an endpoint from the same place,
the environment or `config.toml`. Headers from the other place are left out,
and one warning says so.

In each Claude Code member's environment (OBS-4), appended to any existing value:
`OTEL_RESOURCE_ATTRIBUTES=...,teamwire.team=<team>,teamwire.role=<role>`.
Nothing per run or per message goes there. A Codex member's environment gets no
OpenTelemetry variable (ADR 0023, decision 11), and its metrics still come from
the event rows.

## 5. Fields the event data carries for derived metrics

Decided by the lead on 2026-09-29, so that no event type was added after the
wave-1 contract:

- Every `member.creating`, `member.started`, `member.failed` and `member.refused`
  written by one `teamwire launch` run carries `data.launch` (a ULID made once per run) and
  `data.requested` (the number of members in the spec). The
  `teamwire.launch` span runs from the first event with that id to the event that makes every requested member started, failed or refused. A launch that stops before that has no span.
- `member.started`, `member.failed` and `member.refused` carry `data.steps`, the
  milliseconds each launch step took. `member.started` also carries `data.session_id`, the
  harness session id for the `session.id` attribute.
- A whole-spec refusal (exit 1) creates nothing, so it is not counted. A
  single member refused inside a launch (`afk_budget_spent`, `team_closed`)
  writes `member.refused` and is counted under `teamwire.launch.members`
  `outcome=refused` (ADR 0026). A member refused after launch recorded it keeps
  a `failed` row and has no `member.failed` event, so it is counted once.
- The last event a hook run writes (the `result.*` outcome or `member.held`)
  carries `data.hook` (`claude-stop` for a `claude-code` member,
  `codex-stop` for a `codex` member, else `<harness id>-stop`) and
  `data.hook_ms`; only then is the run's duration known. Only the turn-end
  hook is measured: the post-tool hook's reminder event
  (`delivery.reminded`) carries no duration.
- A `member.handoff` event starts a generation and is not a reprompt. The
  run duration and the generation span count from it, and the span's
  `session.id` is the new session (absent when `new_session` is null). The event
  adds to no counter, sets no member state, and is not counted as a reprompt.
  An undo of a later reprompt returns to the handoff's generation.
- A `member.reprompted` event with `data.undone: true` records an undone
  reprompt. The undone reprompt is neither counted as a reprompt nor taken as the start of a
  generation. A `team.closed` event carries
  `data.outcome`; only `completed` closes the team.

## 6. The outcome report

`outcome_report` in `teamwire-otel` reads the same databases as
`teamwire metrics` with no team named. Those databases are every team under
`$TEAMWIRE_HOME/teams/` and every whole archive under
`$TEAMWIRE_HOME/archive/`. A team seen live and archived counts once, as in
the metrics. `teamwire models --for <class>` shows the report beside the
public evidence in `examples/models/evidence.json`, and never blends the two
into one score.

The report has one row per group of members that share a harness, a model,
an effort, a task class and a kind. A member that never reached
`member.started` is left out, because a failed launch did no work a model
can be judged by. The report gives counts and medians, never percentages.

| Field | Meaning |
| --- | --- |
| `harness` | the member row's harness |
| `model`, `effort` | `data.model` and `data.effort` of the member's latest `member.started`; `unset` when none was recorded |
| `task_class` | `data.task_class` of the member's latest `member.creating`; `unclassified` when it is absent, null or not one of the seven classes |
| `kind` | the member's kind |
| `members` | how many members are in the group |
| `verify` | how many members ended on each verify verdict, from their last `verify.recorded`: `PASS`, `FLAGGED`, `FAIL`, and `none` for a member never verified |
| `reprompts` | reprompts across the group's members; a reprompt that a later undo took back is not counted |
| `tokens_median` | the median of each member's total tokens of all four types, counted as `teamwire.member.tokens` counts them; null when no member has a reading |
| `minutes_median` | the median minutes from a member's first `member.started` to its first `member.completed`; null when no member completed |
| `harder_than_class` | how many members look harder than their class, by the rule below |

The report also gives `teams`, the number of teams read, and `errors`, the
databases it could not read.

**Harder than its class.** A classified member counts when any of these
holds:

- It had 2 or more reprompts that were not undone.
- Its last verify verdict was `FAIL`.
- Its total tokens were over 3 times the median of every member of its class
  with a token reading. This check applies only when the class has at least
  5 such members.

An unclassified member never counts. The rule is a first guess, not a
measured one. One function, `harder_than_class` in
`crates/teamwire-otel/src/outcomes.rs`, holds it, so the rule changes in one
place. A class whose members often count is a sign that the lead classes
such tasks too low.
