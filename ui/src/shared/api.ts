// The HTTP API between the Bun server (src/server/) and the page
// (src/web/). Both sides import these types and path builders; neither side
// changes them alone. Ask the lead for a change.
//
// Every route is GET. The server has no route that changes teamwire state,
// and it never will: teamwire accepts lead actions only from the lead's own
// Claude Code session (teamwire ADR 0008).

import type { LogView, MetricsView, StatusView, TeamEntry } from "./teamwire.ts";

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
}

/**
 * Files the page may read, and where each one lives inside a team's
 * directory (`$TEAMWIRE_HOME/teams/<team>/` or an archive directory).
 * `name` is a role for every kind except `comms`, where it is a display id.
 * Nothing outside this table is ever served. In particular the server never
 * serves `members/<role>/`, which holds each member's token.
 */
export const FILE_KINDS = {
  brief: { dir: "briefs", ext: ".md", name: "role" },
  contract: { dir: "contract", ext: ".md", name: "role" },
  outbox: { dir: "outbox", ext: ".md", name: "role" },
  report: { dir: "reports", ext: ".md", name: "role" },
  verify: { dir: "verify", ext: ".log", name: "role" },
  setup: { dir: "setup", ext: ".log", name: "role" },
  comms: { dir: "comms", ext: ".md", name: "display_id" },
} as const;

export type FileKind = keyof typeof FILE_KINDS;
export const FILE_KIND_NAMES = Object.keys(FILE_KINDS) as FileKind[];

/** Largest file the server returns. A longer file is cut and `truncated` is set. */
export const MAX_FILE_BYTES = 1_000_000;

/** A file that exists, as listed by the server. */
export interface FileRef {
  kind: FileKind;
  /** A role, or a display id for `comms`. */
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
 * `$TEAMWIRE_HOME/archive/` named `<team>-<YYYY-MM-DD>` or
 * `<team>-<YYYY-MM-DD>-<HHMMSS>`. Other directories there are skipped.
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

export type { LogView, MetricsView };

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
} as const;
