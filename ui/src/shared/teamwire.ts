// Schemas for the JSON that the `teamwire` CLI prints. These are the only
// shapes this app accepts from teamwire. Each one was written from the
// teamwire source (crates/teamwire-watch/src/lib.rs, teamwire-core enums)
// and checked against real output on 2026-10-13 (teamwire 0.0.0, schema 3);
// teamwire now prints its version as `0.0.0+<commit>`.
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
export const MemberKind = z.enum(["implementer", "reviewer", "investigator", "closer"]);
export const Lifecycle = z.enum(["creating", "started", "failed", "removed"]);

/** Team and role names: `^[a-z][a-z0-9_]*$`, never a hyphen. */
export const NAME_RE = /^[a-z][a-z0-9_]*$/;
/** Message display ids, such as `m014`. */
export const DISPLAY_ID_RE = /^m[0-9]+$/;

/**
 * A sender or recipient: `lead`, a role, or `<role>@<team>` / `lead@<team>`
 * for an agent of another team (teamwire ADR 0016).
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

/** `teamwire status <team> --json`, and one element of `status --json`. */
export const StatusView = z.object({
  team: z.string().nullable(),
  title: z.string().nullable(),
  state: TeamState.nullable(),
  lead: LeadView.nullable(),
  watch: WatchView,
  members: z.array(MemberView),
  /** The last five messages, newest first. */
  messages: z.array(MessageView),
  stuck_deliveries: z.array(StuckDelivery),
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
  members: z.array(RosterMember),
});

/** `teamwire teams --json`. */
export const TeamsList = z.object({ teams: z.array(TeamEntry) });

export const LogDelivery = z.object({
  to: Party,
  state: DeliveryState,
  attempts: count,
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

export type TeamState = z.infer<typeof TeamState>;
export type PlaceState = z.infer<typeof PlaceState>;
export type ReportState = z.infer<typeof ReportState>;
export type DeliveryState = z.infer<typeof DeliveryState>;
export type MessageType = z.infer<typeof MessageType>;
export type MemberKind = z.infer<typeof MemberKind>;
export type Lifecycle = z.infer<typeof Lifecycle>;
export type Liveness = z.infer<typeof Liveness>;
export type MemberView = z.infer<typeof MemberView>;
export type LeadView = z.infer<typeof LeadView>;
export type MessageView = z.infer<typeof MessageView>;
export type StuckDelivery = z.infer<typeof StuckDelivery>;
export type UndeliveredResult = z.infer<typeof UndeliveredResult>;
export type StatusView = z.infer<typeof StatusView>;
export type RosterMember = z.infer<typeof RosterMember>;
export type TeamEntry = z.infer<typeof TeamEntry>;
export type TeamsList = z.infer<typeof TeamsList>;
export type LogDelivery = z.infer<typeof LogDelivery>;
export type LogMessage = z.infer<typeof LogMessage>;
export type LogView = z.infer<typeof LogView>;
export type Metric = z.infer<typeof Metric>;
export type MetricsView = z.infer<typeof MetricsView>;
