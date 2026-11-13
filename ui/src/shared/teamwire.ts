// Schemas for the JSON that the `teamwire` CLI prints. These are the only
// shapes this app accepts from teamwire. Each one was written from the
// teamwire source (crates/teamwire-watch/src/lib.rs, teamwire-core enums)
// and first checked against real output on 2026-10-13 (teamwire 0.0.0).
// They match store schema 6 (`SCHEMA_VERSION`), and teamwire now prints its
// version as `0.0.0+<commit>`.
//
// Unknown object keys are dropped, so teamwire may add fields freely. A new
// enum value or a changed type fails the parse on purpose: the server then
// reports `shape_changed` and the page shows a banner, instead of drawing
// wrong data.

import { z } from "zod";

export const TeamState = z.enum(["open", "closing", "closed"]);
export const PlaceState = z.enum(["idle", "working", "blocked", "gone", "unknown"]);
export const ReportState = z.enum(["pending", "delivering", "delivered", "undelivered", "unconfirmed"]);
export const DeliveryState = z.enum(["pending", "handed_off", "unconfirmed", "undelivered", "acked"]);
export const MessageType = z.enum(["notice", "finding", "question", "answer", "reprompt", "note"]);
export const MemberKind = z.enum(["implementer", "reviewer", "investigator", "closer", "plan_reviewer"]);
export const Lifecycle = z.enum(["creating", "started", "failed", "removed"]);

/** Team and role names: `^[a-z][a-z0-9_]*$`, never a hyphen. */
export const NAME_RE = /^[a-z][a-z0-9_]*$/;
/** Message display ids, such as `m014`. */
/** Teamwire's own launch patterns, stricter than `NAME_RE`. The write actions use these. */
export const TEAM_NAME_RE = /^[a-z][a-z0-9]{0,11}$/;
export const ROLE_NAME_RE = /^[a-z][a-z0-9_]{0,15}$/;

export const DISPLAY_ID_RE = /^m[0-9]+$/;

/**
 * A sender or recipient: `lead`, a role, or `<role>@<team>` / `lead@<team>`
 * for an agent of another team (teamwire ADR 0016). As a sender, `user` is
 * the human user (ADR 0040), never a member; nobody sends to `user`. A
 * user's message to a member also lists `lead` in `to` and in `deliveries`:
 * that row is the lead's copy.
 */
export const Party = z.string().regex(/^[a-z][a-z0-9_]*(@[a-z][a-z0-9_]*)?$/);

/** `{"alive": "working"}`, `"dead"` or `"unknown"`. */
export const Liveness = z.union([
  z.object({ alive: PlaceState }),
  z.literal("dead"),
  z.literal("unknown"),
]);

export const WatchState = z.enum(["running", "stale", "none"]);

const count = z.number().int().nonnegative();

/** A ticket in a tracker. teamwire assumes no provider: both fields are required. */
export const TicketRef = z.object({
  /** The tracker's lowercase name, such as `linear` or `github`. */
  provider: z.string(),
  /** The ticket's id in that tracker, such as `GKA-580`. */
  id: z.string(),
});

/** How full a member's context window is, and where it hands off. */
export const ContextView = z.object({
  tokens: count,
  window: count,
  /** Whole percent of the window. */
  pct: count,
  /** The token count at which the member is told to hand off. */
  handoff_at: count,
});

/** A tool call that started and has no result yet, and how long it has run. */
export const ToolView = z.object({
  name: z.string(),
  started_at: z.string(),
  /** Whole seconds since `started_at`. */
  seconds: count,
});

/** One member in `teamwire status --json`. Removed members are left out. */
export const MemberView = z.object({
  role: z.string(),
  title: z.string(),
  /**
   * A free-form word, not an enum: `done(<status>)`, `unreported`,
   * `undelivered`, `unconfirmed`, `died?`, `blocked`, `working`, `quiet`,
   * `late`, `prompted outside teamwire`, `not started`, `failed to start`.
   * See `classifyState` in ./state.ts.
   */
  state: z.string(),
  generation: count,
  age_s: count,
  deadline_s: count,
  /** The teamwire version that started the member; null before it started. */
  started_by: z.string().nullable(),
  /** True when `started_by` differs from the running teamwire. */
  older_build: z.boolean(),
  /** Delivery state of the member's newest result in this generation. */
  result: ReportState.nullable(),
  branch: z.string().nullable(),
  /** Only set by `status <team> --role R`. */
  resume_command: z.string().nullable(),
  /** `null` when unknown. An older teamwire leaves the key out. */
  context: ContextView.nullish(),
  /**
   * Only set by `status <team> --role R`, for a working member: the call its
   * session runs now. `null` when the harness cannot tell. An older teamwire
   * leaves the key out.
   */
  current_tool: ToolView.nullish(),
});

export const LeadView = z.object({
  session_id: z.string().nullable(),
  liveness: Liveness,
});

/** The team's watch: a live lease, a lease left behind, or no lease. */
export const WatchView = z.object({
  state: WatchState,
  pid: count.nullable(),
  /** Seconds since the last heartbeat. */
  heartbeat_age_s: count.nullable(),
});

export const MessageView = z.object({
  display_id: z.string(),
  /** UTC RFC 3339 with milliseconds. */
  time: z.string(),
  type: MessageType,
  from: Party,
  to: z.array(Party),
  excerpt: z.string(),
});

export const StuckDelivery = z.object({
  display_id: z.string(),
  to: Party,
  state: DeliveryState,
  age_s: count,
});

/** Held for a member whose newest result asked for a handoff: stored, not typed, and not stuck. */
export const HeldDelivery = z.object({
  display_id: z.string(),
  to: Party,
});

/** Handed to the recipient's session, not acknowledged after 10 minutes. */
export const UnreadDelivery = z.object({
  display_id: z.string(),
  to: Party,
  age_s: count,
});

/** A question that no answer message points at. */
export const UnansweredQuestion = z.object({
  display_id: z.string(),
  from: Party,
  to: z.array(Party),
  age_s: count,
});

export const UndeliveredResult = z.object({
  role: z.string(),
  n: count,
  report_state: ReportState,
  path: z.string(),
});

/** The AFK budget and decisions of a team started by `teamwire afk start`. */
export const AfkView = z.object({
  budget_ends_at: z.string(),
  /** Zero once the budget is spent. */
  seconds_left: count,
  spent: z.boolean(),
  /** The `D<n>` entries in the team's `afk/decisions.md`. */
  decisions: count,
});

/** `teamwire status <team> --json`, and one element of `status --json`. */
export const StatusView = z.object({
  team: z.string().nullable(),
  title: z.string().nullable(),
  state: TeamState.nullable(),
  /** Tickets the team works on. Older teamwire omits it. */
  refs: z.array(TicketRef).default([]),
  lead: LeadView.nullable(),
  watch: WatchView,
  /** `null` for an ordinary team. An older teamwire leaves the key out. */
  afk: AfkView.nullish(),
  members: z.array(MemberView),
  /** The last five messages, oldest first. The fixtures list them newest first, so do not rely on the order. */
  messages: z.array(MessageView),
  stuck_deliveries: z.array(StuckDelivery),
  /** Older teamwire omits it. The "needs a look" panel reads only `stuck_deliveries`. */
  held_deliveries: z.array(HeldDelivery).default([]),
  unread_deliveries: z.array(UnreadDelivery),
  unanswered_questions: z.array(UnansweredQuestion),
  undelivered_results: z.array(UndeliveredResult),
  /** Why part of the view could not be read. */
  errors: z.array(z.string()),
});

/** `teamwire status --json` with no team: every team under the home. */
export const StatusAll = z.array(StatusView);

/** One member in `teamwire teams --json`. Includes removed members. */
export const RosterMember = z.object({
  role: z.string(),
  title: z.string(),
  kind: MemberKind,
  lifecycle: Lifecycle,
  generation: count,
  owns: z.array(z.string()),
  accepts_from_members: z.array(MessageType),
  can_be_typed_to: z.boolean(),
});

export const TeamEntry = z.object({
  name: z.string(),
  title: z.string(),
  state: TeamState,
  /** Tickets the team works on. Older teamwire omits it. */
  refs: z.array(TicketRef).default([]),
  /** When an AFK run's budget ends; `null` for an ordinary team. An older teamwire leaves the key out. */
  afk_budget_ends_at: z.string().nullish(),
  members: z.array(RosterMember),
});

/** `teamwire teams --json`. */
export const TeamsList = z.object({ teams: z.array(TeamEntry) });

export const LogDelivery = z.object({
  to: Party,
  state: DeliveryState,
  attempts: count,
  /** The error code of an undelivered or unconfirmed delivery. An older teamwire leaves the key out. */
  reason: z.string().nullish(),
});

export const LogMessage = z.object({
  display_id: z.string(),
  time: z.string(),
  type: MessageType,
  from: Party,
  to: z.array(Party),
  /** At most 300 characters. The full text is the `comms` file. */
  excerpt: z.string(),
  deliveries: z.array(LogDelivery),
  /** The display id of the message this one answers. An older teamwire leaves the key out. */
  re: z.string().nullish(),
  /** The commit a `notice` names. An older teamwire leaves the key out. */
  commit: z.string().nullish(),
});

/** `teamwire log <team> --json`, oldest first. */
export const LogView = z.object({ messages: z.array(LogMessage) });

export const Metric = z.object({
  name: z.string(),
  unit: z.string(),
  /** `[label, value]` pairs, such as `["team", "deckreview"]`. */
  labels: z.array(z.tuple([z.string(), z.string()])),
  value: z.number(),
});

/** `teamwire metrics [<team>] --json`: live teams and archives alike. */
export const MetricsView = z.object({
  metrics: z.array(Metric),
  errors: z.array(z.string()),
});

/** One "Decisions I made" block of a report. `text` is untrusted Markdown: the page shows it as plain text. */
export const DecisionBlock = z.object({
  /** The line that opened the block, such as `## Decisions I made`. */
  heading: z.string(),
  /** Line number in the report. */
  line: count,
  text: z.string(),
});

export const DecisionMember = z.object({
  role: z.string(),
  kind: MemberKind,
  title: z.string(),
  /** The report's path; null for a member with no report file. */
  report: z.string().nullable(),
  /** Empty for a member whose report holds no such block. */
  blocks: z.array(DecisionBlock),
});

/** One team in `teamwire decisions --json`. The team's `path` and `ulid` are left out on purpose: the page needs neither. */
export const DecisionTeam = z.object({
  team: z.string(),
  title: z.string(),
  created_at: z.string(),
  /** True for a running team, false for an archive. */
  live: z.boolean(),
  /** An archive from before schema 10 has none. */
  refs: z.array(TicketRef).default([]),
  members: z.array(DecisionMember),
});

/** `teamwire decisions [<team>] [--ref REF] [--grep TEXT] --json`, oldest team first. */
export const DecisionsView = z.object({
  teams: z.array(DecisionTeam),
  errors: z.array(z.string()),
});

/**
 * The closed list of event types (teamwire `EventType`, docs/metrics.md
 * section 1). Adding one changes teamwire's store schema, so a new value
 * fails the parse like every other enum here.
 */
export const EventType = z.enum([
  "team.created", "team.closing", "team.closed", "team.lead_adopted", "team.refs_added",
  "member.creating", "member.started", "member.failed", "member.refused", "member.replaced",
  "member.removed", "member.reprompted", "member.woken", "member.handoff", "member.completed", "member.completion_refused", "member.held",
  "message.sent", "message.refused",
  "delivery.handed_off", "delivery.unconfirmed", "delivery.undelivered", "delivery.acked",
  "delivery.reminded", "delivery.resent", "delivery.reentered", "delivery.recipient_stopped",
  "result.delivering", "result.delivered", "result.undelivered", "result.unconfirmed",
  "watch.started", "watch.line", "watch.stopped",
  "verify.recorded", "verify.waived",
  "close.step", "close.refused",
  "store.error",
]);

/**
 * One row of `teamwire events <team> --json`. `data` keys differ per type
 * and are not part of this schema: the page reads each key it needs with
 * its own check and shows the row plainly when a key is missing, because
 * older rows carry fewer keys (for example `usage` on `result.delivering`).
 * A `message.sent` row's `sender_kind` is `lead`, `member` or `user`, and its
 * `role` is null for the user; a `message.refused` row's `sender` may be `user`.
 */
export const EventRow = z.object({
  seq: z.number().int().positive(),
  /** UTC RFC 3339 with milliseconds. */
  time: z.string(),
  type: EventType,
  role: z.string().nullable(),
  /** Display id of the message the row is about, such as `m014`. */
  message: z.string().nullable(),
  data: z.record(z.string(), z.unknown()),
});

/** `teamwire events <team> [--archive DIR] [--after SEQ] --json`, oldest first. */
export const EventsView = z.object({
  team: z.string(),
  source: z.enum(["live", "archive"]),
  /** The archive directory name, or null for a live store. */
  archive: z.string().nullable(),
  /** The highest seq in the store, also when `--after` left every row out. */
  last_seq: count,
  events: z.array(EventRow),
  errors: z.array(z.string()),
});

export type TeamState = z.infer<typeof TeamState>;
export type PlaceState = z.infer<typeof PlaceState>;
export type ReportState = z.infer<typeof ReportState>;
export type DeliveryState = z.infer<typeof DeliveryState>;
export type MessageType = z.infer<typeof MessageType>;
export type MemberKind = z.infer<typeof MemberKind>;
export type Lifecycle = z.infer<typeof Lifecycle>;
export type Liveness = z.infer<typeof Liveness>;
export type TicketRef = z.infer<typeof TicketRef>;
export type ContextView = z.infer<typeof ContextView>;
export type MemberView = z.infer<typeof MemberView>;
export type LeadView = z.infer<typeof LeadView>;
export type MessageView = z.infer<typeof MessageView>;
export type StuckDelivery = z.infer<typeof StuckDelivery>;
export type HeldDelivery = z.infer<typeof HeldDelivery>;
export type UndeliveredResult = z.infer<typeof UndeliveredResult>;
export type AfkView = z.infer<typeof AfkView>;
export type StatusView = z.infer<typeof StatusView>;
export type RosterMember = z.infer<typeof RosterMember>;
export type TeamEntry = z.infer<typeof TeamEntry>;
export type TeamsList = z.infer<typeof TeamsList>;
export type LogDelivery = z.infer<typeof LogDelivery>;
export type LogMessage = z.infer<typeof LogMessage>;
export type LogView = z.infer<typeof LogView>;
export type Metric = z.infer<typeof Metric>;
export type MetricsView = z.infer<typeof MetricsView>;
export type DecisionBlock = z.infer<typeof DecisionBlock>;
export type DecisionMember = z.infer<typeof DecisionMember>;
export type DecisionTeam = z.infer<typeof DecisionTeam>;
export type DecisionsView = z.infer<typeof DecisionsView>;
export type EventType = z.infer<typeof EventType>;
export type EventRow = z.infer<typeof EventRow>;
export type EventsView = z.infer<typeof EventsView>;
