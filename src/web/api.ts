// The page's only door to data. `httpClient` talks to the Bun server;
// `mockClient` serves fixtures/ so the page can be developed and checked
// without a server or a real teamwire.

import {
  api,
  type ArchiveEntry,
  type FileKind,
  type FileRef,
  type FileText,
  type LogView,
  type MetricsView,
  type Problem,
  type ResumeCommand,
  type Snapshot,
} from "../shared/api.ts";
import { LogView as LogViewSchema, MetricsView as MetricsViewSchema, StatusAll, TeamsList } from "../shared/teamwire.ts";
import statusFixture from "../../fixtures/status.json";
import teamsFixture from "../../fixtures/teams.json";
import logFixture from "../../fixtures/log.json";
import metricsFixture from "../../fixtures/metrics.json";
import archivesFixture from "../../fixtures/archives.json";

export type Connection = "connecting" | "connected" | "reconnecting" | "forbidden";

/** A failed request. `problem` is the server's JSON body when it sent one. */
export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly problem: Problem,
  ) {
    super(problem.detail);
  }
}

export interface ApiClient {
  /**
   * Calls `onSnapshot` for the latest snapshot and for each new one, and
   * `onConnection` whenever the link changes. Returns a function that stops both.
   */
  subscribe(onSnapshot: (s: Snapshot) => void, onConnection: (c: Connection) => void): () => void;
  teamLog(team: string): Promise<LogView>;
  resume(team: string, role: string): Promise<ResumeCommand>;
  teamFiles(team: string): Promise<FileRef[]>;
  teamFile(team: string, kind: FileKind, name: string): Promise<FileText>;
  archives(): Promise<ArchiveEntry[]>;
  archiveFile(dir: string, kind: FileKind, name: string): Promise<FileText>;
  metrics(): Promise<MetricsView>;
}

async function getJson(url: string, signal?: AbortSignal): Promise<unknown> {
  const res = await fetch(url, { credentials: "same-origin", signal });
  const body: unknown = await res.json().catch(() => null);
  if (!res.ok) {
    const p = body as Partial<Problem> | null;
    throw new ApiError(res.status, {
      code: p?.code ?? (res.status === 403 ? "forbidden" : "cli_failed"),
      detail: p?.detail ?? `HTTP ${res.status}`,
    });
  }
  return body;
}

export const httpClient: ApiClient = {
  subscribe(onSnapshot, onConnection) {
    let es: EventSource | null = null;
    let retry: ReturnType<typeof setTimeout> | undefined;
    let stopped = false;

    const open = () => {
      es = new EventSource(api.events());
      es.addEventListener("snapshot", (e) => {
        try {
          onSnapshot(JSON.parse((e as MessageEvent<string>).data) as Snapshot);
          onConnection("connected");
        } catch {
          // A broken event is skipped; the next poll sends a new one.
        }
      });
      es.onerror = () => {
        es?.close();
        es = null;
        if (stopped) return;
        // EventSource hides the HTTP status, so ask a plain request why it failed.
        getJson(api.snapshot()).then(
          (s) => {
            onSnapshot(s as Snapshot);
            onConnection("reconnecting");
          },
          (err) => onConnection(err instanceof ApiError && err.status === 403 ? "forbidden" : "reconnecting"),
        );
        retry = setTimeout(open, 3000);
      };
    };
    open();
    return () => {
      stopped = true;
      clearTimeout(retry);
      es?.close();
    };
  },
  teamLog: async (team) => LogViewSchema.parse(await getJson(api.teamLog(team))),
  resume: async (team, role) => (await getJson(api.resume(team, role))) as ResumeCommand,
  teamFiles: async (team) => (await getJson(api.teamFiles(team))) as FileRef[],
  teamFile: async (team, kind, name) => (await getJson(api.teamFile(team, kind, name))) as FileText,
  archives: async () => (await getJson(api.archives())) as ArchiveEntry[],
  archiveFile: async (dir, kind, name) => (await getJson(api.archiveFile(dir, kind, name))) as FileText,
  metrics: async () => MetricsViewSchema.parse(await getJson(api.metrics())),
};

/** The mock moves on every 5 s, like the server's poll. */
export const MOCK_INTERVAL_MS = 5000;

const baseStatus = StatusAll.parse(statusFixture);
const baseTeams = TeamsList.parse(teamsFixture).teams;
const baseLog = LogViewSchema.parse(logFixture);
const baseMetrics = MetricsViewSchema.parse(metricsFixture);
const baseArchives = archivesFixture as ArchiveEntry[];

/** A snapshot `elapsedS` seconds after the fixtures were taken. */
export function mockSnapshot(elapsedS: number, now = new Date()): Snapshot {
  return {
    at: now.toISOString(),
    durationMs: 120,
    intervalMs: MOCK_INTERVAL_MS,
    stale: false,
    lastGoodAt: now.toISOString(),
    status: baseStatus.map((t) => ({
      ...t,
      members: t.members.map((m) => ({ ...m, age_s: m.age_s + elapsedS })),
    })),
    teams: baseTeams,
    problems: [],
  };
}

function mockText(kind: FileKind, name: string, where: string): FileText {
  const text = `# ${kind} for ${name}\n\nThis is invented text from mock mode (${where}).\nIt is shown as plain text, never as HTML: <b>not bold</b>.\n`;
  return { kind, name, bytes: text.length, modifiedAt: new Date().toISOString(), text, truncated: false };
}

export const mockClient: ApiClient = {
  subscribe(onSnapshot, onConnection) {
    const started = Date.now();
    const push = () => onSnapshot(mockSnapshot(Math.floor((Date.now() - started) / 1000)));
    push();
    onConnection("connected");
    const timer = setInterval(push, MOCK_INTERVAL_MS);
    return () => clearInterval(timer);
  },
  teamLog: async (team) => (team === "payments" ? baseLog : { messages: [] }),
  resume: async (team, role) => ({ command: `teamwire resume ${team} ${role}`, problems: [] }),
  teamFiles: async (team) => {
    const roster = baseTeams.find((t) => t.name === team);
    const now = new Date().toISOString();
    const files: FileRef[] = [];
    for (const m of roster?.members ?? []) {
      if (m.lifecycle === "removed") continue;
      for (const kind of ["brief", "contract", "outbox", "report"] as const) {
        files.push({ kind, name: m.role, bytes: 400, modifiedAt: now });
      }
      if (m.role === "api") files.push({ kind: "verify", name: m.role, bytes: 90, modifiedAt: now });
      if (m.role === "ledger") files.push({ kind: "setup", name: m.role, bytes: 90, modifiedAt: now });
    }
    if (team === "payments") {
      for (const msg of baseLog.messages) files.push({ kind: "comms", name: msg.display_id, bytes: 300, modifiedAt: msg.time });
    }
    return files;
  },
  teamFile: async (team, kind, name) => mockText(kind, name, team),
  archives: async () => baseArchives,
  archiveFile: async (dir, kind, name) => mockText(kind, name, dir),
  metrics: async () => baseMetrics,
};

/** `?mock=1` in the URL selects the mock client. */
export function selectClient(search = location.search): ApiClient {
  return new URLSearchParams(search).get("mock") === "1" ? mockClient : httpClient;
}
