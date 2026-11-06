// The HTTP API between the Bun server (src/server/) and the page
// (src/web/). Both sides import these types and path builders; neither side
// changes them alone. Ask the lead for a change.
//
// Every route is GET, except the three write actions under
// `/api/teams/:team/actions/`. Those exist only while the server runs with
// `TW_UI_ACTIONS=1`; otherwise they answer 404. They send as the human user
// (`teamwire send --from user`, teamwire ADR 0040) and close members or
// teams. They never speak as the lead: teamwire accepts lead actions only
// from the lead's own Claude Code session (teamwire ADR 0008).

import { z } from "zod";
import { ROLE_NAME_RE, type DecisionsView, type EventsView, type LogView, type MetricsView, type StatusView, type TeamEntry } from "./teamwire.ts";

/** Default port. The server binds 127.0.0.1 only. */
export const DEFAULT_PORT = 7878;
/** Default time between two polls of `teamwire status --json`. */
export const DEFAULT_INTERVAL_MS = 5000;

/**
 * Name of the cookie that carries the session token. Browsers share
 * cookies across every port of a host (RFC 6265, section 8.5), so the
 * name carries the port: two dashboards on different ports keep their own
 * tokens. A cookie for 127.0.0.1 is still sent to any server on any port
 * of 127.0.0.1. No cookie setting prevents that for plain-HTTP localhost.
 */
export const tokenCookie = (port: number) => `tw_ui_${port}`;

export type ProblemCode =
  /** `teamwire` exited non-zero, or could not be started. */
  | "cli_failed"
  /** `teamwire` ran past its time limit and was killed. */
  | "cli_timeout"
  /** `teamwire` printed JSON that does not match src/shared/teamwire.ts. */
  | "shape_changed"
  | "not_found"
  | "bad_request"
  /** Missing or wrong token cookie, or a Host header that is not ours. */
  | "forbidden"
  | "too_large";

/** The body of every non-200 API response, and an entry in `Snapshot.problems`. */
export interface Problem {
  code: ProblemCode;
  /** One line for a person: what failed and, when known, why. */
  detail: string;
}

/**
 * What the server knows about every team after one poll. One poll runs
 * `teamwire status --json` and `teamwire teams --json`.
 */
export interface Snapshot {
  /** ISO time at which this poll finished, whether it worked or failed. */
  at: string;
  /**
   * ISO time at which the last poll that worked finished, or null when no
   * poll has worked since the server started. When `stale` is true, this
   * is the age of the data on screen.
   */
  lastGoodAt: string | null;
  /** How long the poll took. */
  durationMs: number;
  /** The server's poll interval. */
  intervalMs: number;
  /**
   * True when this poll failed and `status` and `teams` are left over from
   * the last poll that worked. `problems` says why.
   */
  stale: boolean;
  /** `teamwire status --json`: one view per team under the home. */
  status: StatusView[];
  /** `teamwire teams --json`: the rosters, including removed members. */
  teams: TeamEntry[];
  problems: Problem[];
  /** True when the server started with `TW_UI_ACTIONS=1` and read the user key. The page shows no action control otherwise. */
  actions: boolean;
}

/**
 * Files the page may read, and where each one lives inside a team's
 * directory (`$TEAMWIRE_HOME/teams/<team>/` or an archive directory).
 * `name` is a role for every kind except `comms`, where it is a display id,
 * and `afk`, where it is one of `AFK_FILE_NAMES`. Nothing outside this table
 * is ever served. In particular the server never
 * serves `members/<role>/`, which holds each member's token.
 */
export const FILE_KINDS = {
  brief: { dir: "briefs", ext: ".md", name: "role" },
  contract: { dir: "contract", ext: ".md", name: "role" },
  outbox: { dir: "outbox", ext: ".md", name: "role" },
  report: { dir: "reports", ext: ".md", name: "role" },
  /** A member's handoff document. The role pattern refuses a dot, so the dotted name is never a name here: only the extension carries it. */
  handoff: { dir: "reports", ext: ".handoff.md", name: "role" },
  verify: { dir: "verify", ext: ".log", name: "role" },
  setup: { dir: "setup", ext: ".log", name: "role" },
  comms: { dir: "comms", ext: ".md", name: "display_id" },
  /** An AFK run's lead files (teamwire ADR 0025). Only the names in `AFK_FILE_NAMES`. */
  afk: { dir: "afk", ext: ".md", name: "afk_file" },
} as const;

/**
 * The only `afk/` files served: the lead's decisions log, its final report
 * and the task it was given. The other files there (briefs, specs, scratch)
 * are the lead's working copies and are left out. `REPORT` is upper-case on
 * disk, so these names are matched exactly and not with `NAME_RE`.
 */
export const AFK_FILE_NAMES = ["decisions", "REPORT", "task"] as const;
export type AfkFileName = (typeof AFK_FILE_NAMES)[number];

export type FileKind = keyof typeof FILE_KINDS;
export const FILE_KIND_NAMES = Object.keys(FILE_KINDS) as FileKind[];

/** Largest file the server returns. A longer file is cut and `truncated` is set. */
export const MAX_FILE_BYTES = 1_000_000;

/** A file that exists, as listed by the server. */
export interface FileRef {
  kind: FileKind;
  /** A role, a display id for `comms`, or one of `AFK_FILE_NAMES` for `afk`. */
  name: string;
  bytes: number;
  /** ISO time of the last change. */
  modifiedAt: string;
}

/**
 * One node of a parsed Markdown file. The server parses with
 * `Bun.markdown.render`, and the page builds React elements from the tree,
 * so no Markdown string is ever parsed as HTML in the page.
 */
export interface MdNode {
  /** `text`, or the name of the `Bun.markdown` render callback that made the node. */
  t: string;
  /** The text of a `text` node. */
  v?: string;
  c?: MdNode[];
  /** The callback's metadata, such as `level`, `href`, `src`, `language`, `align` or `checked`. */
  m?: Record<string, string | number | boolean>;
}

/** The text of one file. */
export interface FileText extends FileRef {
  text: string;
  truncated: boolean;
  /** The Markdown tree of `text` for a `.md` kind, or null for a log or when parsing failed. */
  doc: MdNode[] | null;
}

/** `GET /api/teams/:team/resume/:role`. */
export interface ResumeCommand {
  /** The command that resumes the member's session, or null when teamwire has none. */
  command: string | null;
  /** From `StatusView.errors`, when teamwire said why there is no command. */
  problems: Problem[];
}

/**
 * A closed team's archive: a directory directly under
 * `$TEAMWIRE_HOME/archive/` named `<team>-<YYYY-MM-DD>`,
 * `<team>-<YYYY-MM-DD>-<HHMMSS>` or `<team>-<YYYY-MM-DD>-<HHMMSS>-<n>`, the
 * three names `teamwire close` writes. Other directories there are skipped.
 */
export interface ArchiveEntry {
  /** The directory name, used as the id in archive routes. */
  dir: string;
  team: string;
  /** `YYYY-MM-DD` from the directory name. */
  date: string;
  /** ISO mtime of the archive's `SHA256SUMS`, or null when that file is missing. */
  closedAt: string | null;
  files: FileRef[];
}

export type { DecisionsView, EventsView, LogView, MetricsView };

/** The dashboard's cap on a message. The CLI sets none for `reprompt` and `note`. */
export const MAX_TEXT_CHARS = 4000;

/** 1 to 4000 characters, counted as code points. A NUL cannot travel in a process argument. */
export const MessageText = z.string().refine((t) => {
  const n = [...t].length;
  return n >= 1 && n <= MAX_TEXT_CHARS && !t.includes("\0");
}, `1 to ${MAX_TEXT_CHARS} characters, no NUL`);

/** `POST /api/teams/:team/actions/send`. The CLI enforces who may get which type (plan D4). */
export const SendBody = z.strictObject({
  to: z.string().regex(ROLE_NAME_RE),
  type: z.enum(["reprompt", "note"]),
  text: MessageText,
});

/** `POST /api/teams/:team/actions/close-member`. */
export const CloseMemberBody = z.strictObject({
  role: z.string().regex(ROLE_NAME_RE),
  stop: z.boolean(),
  unverified: z.boolean(),
  dryRun: z.boolean(),
});

/** `POST /api/teams/:team/actions/close-team`. A real close needs `confirmName` equal to the team. */
export const CloseTeamBody = z.strictObject({
  dryRun: z.boolean(),
  confirmName: z.string(),
});

export type SendBody = z.infer<typeof SendBody>;
export type CloseMemberBody = z.infer<typeof CloseMemberBody>;
export type CloseTeamBody = z.infer<typeof CloseTeamBody>;

/** A decisions search across every team and archive: report text, or a ticket ref as `provider:id`. */
export type DecisionQuery = { grep: string } | { ref: string };

/**
 * Every route. The page builds URLs only through these functions, and the
 * server matches exactly these paths.
 *
 * | Route | Response |
 * | --- | --- |
 * | `GET /auth?token=T` | Sets the token cookie, then 303 to `/`. Wrong token: 403 |
 * | `GET /api/snapshot` | `Snapshot` (the latest poll) |
 * | `GET /api/events` | `text/event-stream`. Event `snapshot`, data a `Snapshot` as JSON. The latest one is sent on connect, then one after every poll |
 * | `GET /api/teams/:team/log` | `LogView` |
 * | `GET /api/teams/:team/resume/:role` | `ResumeCommand` |
 * | `GET /api/teams/:team/files` | `FileRef[]` |
 * | `GET /api/teams/:team/file/:kind/:name` | `FileText` |
 * | `GET /api/archives` | `ArchiveEntry[]`, newest first |
 * | `GET /api/archives/:dir/file/:kind/:name` | `FileText` |
 * | `GET /api/metrics` | `MetricsView` for every live team and archive |
 * | `GET /api/teams/:team/timeline?after=SEQ` | `EventsView` from `teamwire events <team> [--after SEQ] --json`. `after` is optional, a non-negative integer |
 * | `GET /api/archives/:dir/timeline` | `EventsView` from `teamwire events <team> --archive <dir> --json`, the team taken from the archive entry |
 * | `GET /api/teams/:team/decisions` | `DecisionsView` from `teamwire decisions <team> --json` |
 * | `GET /api/decisions?grep=TEXT` | `DecisionsView` from `teamwire decisions --grep TEXT --json`, across all teams. TEXT is 1 to 200 characters and does not start with `-` |
 * | `GET /api/decisions?ref=PROVIDER:ID` | `DecisionsView` from `teamwire decisions --ref=PROVIDER:ID --json`. Exactly one of `grep` and `ref` |
 * | `GET /api/teams/:team/metrics` | `MetricsView` from `teamwire metrics <team> --json`, cached 60 s per team |
 *
 * Any other status than 200 carries a `Problem` as JSON.
 */
export const api = {
  auth: (token: string) => `/auth?token=${encodeURIComponent(token)}`,
  snapshot: () => "/api/snapshot",
  events: () => "/api/events",
  teamLog: (team: string) => `/api/teams/${team}/log`,
  resume: (team: string, role: string) => `/api/teams/${team}/resume/${role}`,
  teamFiles: (team: string) => `/api/teams/${team}/files`,
  teamFile: (team: string, kind: FileKind, name: string) => `/api/teams/${team}/file/${kind}/${name}`,
  archives: () => "/api/archives",
  archiveFile: (dir: string, kind: FileKind, name: string) => `/api/archives/${dir}/file/${kind}/${name}`,
  metrics: () => "/api/metrics",
  teamTimeline: (team: string, after?: number) =>
    after === undefined ? `/api/teams/${team}/timeline` : `/api/teams/${team}/timeline?after=${after}`,
  archiveTimeline: (dir: string) => `/api/archives/${dir}/timeline`,
  teamDecisions: (team: string) => `/api/teams/${team}/decisions`,
  decisions: (q: DecisionQuery) =>
    "grep" in q ? `/api/decisions?grep=${encodeURIComponent(q.grep)}` : `/api/decisions?ref=${encodeURIComponent(q.ref)}`,
  teamMetrics: (team: string) => `/api/teams/${team}/metrics`,
} as const;
