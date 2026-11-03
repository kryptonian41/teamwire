// The HTTP API as a function from Request to Response. It takes its
// dependencies as arguments, so tests call it without a socket or a CLI.

import { NAME_RE, type MetricsView } from "../shared/teamwire.ts";
import type { Problem } from "../shared/api.ts";
import type { Runner } from "./cli.ts";
import { archiveDir, listArchives, listFiles, parseArchiveName, readKindFile, teamDir } from "./files.ts";
import type { Poller } from "./poller.ts";
import { json, problem, type Security } from "./security.ts";

export interface AppDeps {
  security: Security;
  poller: Poller;
  run: Runner;
  home: string;
  now?: () => number;
  /** How long `metrics` output is reused. It reads every archive. */
  metricsTtlMs?: number;
  keepaliveMs?: number;
}

export interface App {
  /** Handles `/auth` and every `/api/*` route. */
  handle(req: Request): Promise<Response>;
}

const notFound = (detail: string) => problem(404, { code: "not_found", detail });

/** The status for a CLI failure: the server is fine, the program behind it is not. */
function cliFailure(problems: Problem[]): Response {
  return problem(502, problems[0] ?? { code: "cli_failed", detail: "teamwire failed" });
}

/** Turns a runner result into a response; a refused argument is the caller's mistake, not the CLI's. */
function cliResponse(res: { ok: true; data: unknown } | { ok: false; problems: Problem[] }): Response {
  if (res.ok) return json(res.data);
  return res.problems[0]?.code === "bad_request" ? problem(400, res.problems[0]) : cliFailure(res.problems);
}

export function createApp(deps: AppDeps): App {
  const now = deps.now ?? Date.now;
  const metricsTtl = deps.metricsTtlMs ?? 60_000;
  const keepaliveMs = deps.keepaliveMs ?? 15_000;
  const encoder = new TextEncoder();
  let metricsCache: { at: number; data: MetricsView } | null = null;

  function events(req: Request): Response {
    let cleanup = () => {};
    const stream = new ReadableStream<Uint8Array>({
      start(controller) {
        const send = (text: string) => {
          try {
            controller.enqueue(encoder.encode(text));
          } catch {
            cleanup();
          }
        };
        const sendSnapshot = (s: unknown) => send(`event: snapshot\ndata: ${JSON.stringify(s)}\n\n`);
        const latest = deps.poller.latest();
        if (latest) sendSnapshot(latest);
        const off = deps.poller.subscribe(sendSnapshot);
        const timer = setInterval(() => send(": keepalive\n\n"), keepaliveMs);
        cleanup = () => {
          clearInterval(timer);
          off();
        };
        req.signal.addEventListener("abort", cleanup);
      },
      cancel() {
        cleanup();
      },
    });
    return new Response(stream, {
      headers: { "content-type": "text/event-stream", "cache-control": "no-store", connection: "keep-alive" },
    });
  }

  async function teamRoute(team: string, rest: string[], query: URLSearchParams): Promise<Response> {
    const dir = await teamDir(deps.home, team);
    if (!dir) return notFound(`no team named ${team}`);
    const [what, a, b] = rest;
    if (what === "log" && rest.length === 1) {
      const res = await deps.run({ cmd: "log", team });
      return res.ok ? json(res.data) : cliFailure(res.problems);
    }
    if (what === "timeline" && rest.length === 1) {
      const raw = query.get("after");
      // Digits only: Number() would accept "", " 1", "1e3" and "0x10".
      if (raw !== null && !/^\d{1,15}$/.test(raw)) {
        return problem(400, { code: "bad_request", detail: "after must be a non-negative integer" });
      }
      return cliResponse(await deps.run(raw === null ? { cmd: "events", team } : { cmd: "events", team, after: Number(raw) }));
    }
    if (what === "files" && rest.length === 1) return json(await listFiles(dir));
    if (what === "file" && rest.length === 3 && a && b) return fileResponse(dir, a, b);
    if (what === "resume" && rest.length === 2 && a) {
      if (!NAME_RE.test(a)) return problem(400, { code: "bad_request", detail: "bad role name" });
      const res = await deps.run({ cmd: "statusRole", team, role: a });
      if (!res.ok) return res.problems[0]?.code === "bad_request" ? problem(400, res.problems[0]) : cliFailure(res.problems);
      const member = res.data.members.find((m) => m.role === a);
      if (!member) return notFound(`no member ${a} in team ${team}`);
      const problems: Problem[] = res.data.errors.map((detail) => ({ code: "cli_failed", detail }));
      return json({ command: member.resume_command, problems });
    }
    return notFound("no such route");
  }

  async function fileResponse(base: string, kind: string, name: string): Promise<Response> {
    const res = await readKindFile(base, kind, name);
    if (res.ok) return json(res.file);
    return problem(res.problem.code === "bad_request" ? 400 : 404, res.problem);
  }

  async function archiveRoute(rest: string[]): Promise<Response> {
    if (rest.length === 0) return json(await listArchives(deps.home));
    const [dirName, what, kind, name] = rest;
    if (rest.length === 4 && dirName && what === "file" && kind && name) {
      const dir = await archiveDir(deps.home, dirName);
      if (!dir) return notFound(`no archive named ${dirName}`);
      return fileResponse(dir, kind, name);
    }
    if (rest.length === 2 && dirName && what === "timeline") {
      const archive = await archiveDir(deps.home, dirName);
      const parsed = parseArchiveName(dirName);
      if (!archive || !parsed) return notFound(`no archive named ${dirName}`);
      return cliResponse(await deps.run({ cmd: "eventsArchive", team: parsed.team, archive: dirName }));
    }
    return notFound("no such route");
  }

  async function metrics(): Promise<Response> {
    if (metricsCache && now() - metricsCache.at < metricsTtl) return json(metricsCache.data);
    const res = await deps.run({ cmd: "metrics" });
    if (!res.ok) return cliFailure(res.problems);
    metricsCache = { at: now(), data: res.data };
    return json(res.data);
  }

  return {
    async handle(req) {
      const url = new URL(req.url);
      if (url.pathname === "/auth") return req.method === "GET" ? deps.security.auth(req) : methodNotAllowed();
      if (!url.pathname.startsWith("/api/")) return notFound("no such route");
      const denied = deps.security.checkApi(req);
      if (denied) return denied;
      if (req.method !== "GET") return methodNotAllowed();

      let segments: string[];
      try {
        segments = url.pathname.slice("/api/".length).split("/").map(decodeURIComponent);
      } catch {
        return problem(400, { code: "bad_request", detail: "bad path encoding" });
      }
      const [head, ...rest] = segments;
      try {
        if (head === "snapshot" && rest.length === 0) {
          const latest = deps.poller.latest();
          return latest ? json(latest) : problem(503, { code: "cli_failed", detail: "the first poll has not finished" });
        }
        if (head === "events" && rest.length === 0) return events(req);
        if (head === "metrics" && rest.length === 0) return await metrics();
        if (head === "archives") return await archiveRoute(rest);
        if (head === "teams" && rest[0]) return await teamRoute(rest[0], rest.slice(1), url.searchParams);
      } catch (e) {
        return problem(500, { code: "cli_failed", detail: `server error: ${(e as Error).message}` });
      }
      return notFound("no such route");
    },
  };
}

function methodNotAllowed(): Response {
  return problem(405, { code: "bad_request", detail: "only GET is allowed" });
}
