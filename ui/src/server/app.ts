// The HTTP API as a function from Request to Response. It takes its
// dependencies as arguments, so tests call it without a socket or a CLI.

import { NAME_RE, type MetricsView } from "../shared/teamwire.ts";
import { CloseMemberBody, CloseTeamBody, SendBody, type ActionResult, type LeadNote, type Problem } from "../shared/api.ts";
import type { z } from "zod";
import type { ActionOutput, CliCall, Runner } from "./cli.ts";
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
  /** Write actions are on (`TW_UI_ACTIONS=1` and the user key read). Off, every action route is 404. */
  actions?: boolean;
}

export interface App {
  /** Handles `/auth` and every `/api/*` route. */
  handle(req: Request): Promise<Response>;
}

/** `/api/teams/:team/actions/:name`, the only routes that change teamwire state. */
const ACTION_ROUTE = /^\/api\/teams\/([^/]+)\/actions\/(send|close-member|close-team)$/;
type ActionName = "send" | "close-member" | "close-team";

const notFound = (detail: string) => problem(404, { code: "not_found", detail });
const badRequest = (detail: string) => problem(400, { code: "bad_request", detail });

/** A real body is a few KiB at most; 4000 characters of text is under 16 KiB as JSON. */
const MAX_BODY_CHARS = 64 * 1024;

async function readBody<S extends z.ZodType>(req: Request, schema: S): Promise<{ ok: true; data: z.infer<S> } | { ok: false; res: Response }> {
  const text = await req.text();
  if (text.length > MAX_BODY_CHARS) return { ok: false, res: badRequest("body too large") };
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch {
    return { ok: false, res: badRequest("body is not JSON") };
  }
  const parsed = schema.safeParse(raw);
  if (!parsed.success) {
    const issue = parsed.error.issues[0];
    return { ok: false, res: badRequest(`bad body at ${issue?.path.join(".") || "(root)"}: ${issue?.message ?? "invalid"}`) };
  }
  return { ok: true, data: parsed.data };
}

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
  // One entry per team that has a directory here, so the map is as small as the home.
  const teamMetricsCache = new Map<string, { at: number; data: MetricsView }>();

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
    if (what === "decisions" && rest.length === 1) return cliResponse(await deps.run({ cmd: "decisionsTeam", team }));
    if (what === "metrics" && rest.length === 1) return await teamMetrics(team);
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

  async function teamMetrics(team: string): Promise<Response> {
    const hit = teamMetricsCache.get(team);
    if (hit && now() - hit.at < metricsTtl) return json(hit.data);
    const res = await deps.run({ cmd: "metricsTeam", team });
    if (!res.ok) return cliResponse(res);
    teamMetricsCache.set(team, { at: now(), data: res.data });
    return json(res.data);
  }

  /** `?grep=TEXT` or `?ref=provider:id`, exactly one. The CLI form checks the value. */
  async function decisionsSearch(query: URLSearchParams): Promise<Response> {
    const grep = query.get("grep");
    const ref = query.get("ref");
    if ((grep === null) === (ref === null)) {
      return problem(400, { code: "bad_request", detail: "give exactly one of grep and ref" });
    }
    return cliResponse(await deps.run(grep !== null ? { cmd: "decisionsGrep", text: grep } : { cmd: "decisionsRef", ref: ref! }));
  }

  async function actionRoute(req: Request, team: string, name: ActionName): Promise<Response> {
    if (!deps.actions) return notFound("actions are off: start the server with TW_UI_ACTIONS=1");
    if (req.method !== "POST") return json({ code: "bad_request", detail: "only POST is allowed" } satisfies Problem, 405, { allow: "POST" });
    const denied = deps.security.checkWrite(req);
    if (denied) return denied;
    switch (name) {
      case "send": {
        const body = await readBody(req, SendBody);
        if (!body.ok) return body.res;
        if (!(await teamDir(deps.home, team))) return notFound(`no team named ${team}`);
        const sent = await runAction({ cmd: "send", team, ...body.data });
        return sent.ok ? json(result(sent.out, [])) : sent.res;
      }
      case "close-member": {
        const body = await readBody(req, CloseMemberBody);
        if (!body.ok) return body.res;
        if (!(await teamDir(deps.home, team))) return notFound(`no team named ${team}`);
        return notFound("close-member is not built yet");
      }
      case "close-team": {
        const body = await readBody(req, CloseTeamBody);
        if (!body.ok) return body.res;
        if (!body.data.dryRun && body.data.confirmName !== team) return badRequest("type the team's name to confirm the close");
        if (!(await teamDir(deps.home, team))) return notFound(`no team named ${team}`);
        return notFound("close-team is not built yet");
      }
    }
  }

  /** A timeout is a result (outcome unknown), not an error: the command may have done its work. */
  async function runAction(call: Extract<CliCall, { cmd: "send" | "closeMember" | "closeTeam" }>): Promise<{ ok: true; out: ActionOutput | null } | { ok: false; res: Response }> {
    const res = await deps.run(call);
    if (res.ok) return { ok: true, out: res.data };
    if (res.problems[0]?.code === "cli_timeout") return { ok: true, out: null };
    return { ok: false, res: cliResponse(res) };
  }

  return {
    async handle(req) {
      const url = new URL(req.url);
      if (url.pathname === "/auth") return req.method === "GET" ? deps.security.auth(req) : methodNotAllowed();
      if (!url.pathname.startsWith("/api/")) return notFound("no such route");
      const denied = deps.security.checkApi(req);
      if (denied) return denied;
      const action = ACTION_ROUTE.exec(url.pathname);
      if (action) return await actionRoute(req, action[1]!, action[2] as ActionName);
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
        if (head === "decisions" && rest.length === 0) return await decisionsSearch(url.searchParams);
        if (head === "archives") return await archiveRoute(rest);
        if (head === "teams" && rest[0]) return await teamRoute(rest[0], rest.slice(1), url.searchParams);
      } catch (e) {
        return problem(500, { code: "cli_failed", detail: `server error: ${(e as Error).message}` });
      }
      return notFound("no such route");
    },
  };
}

function result(out: ActionOutput | null, leadNotes: LeadNote[]): ActionResult {
  return { exit: out?.exit ?? null, stdout: out?.stdout ?? "", stderr: out?.stderr ?? "", leadNotes };
}

function methodNotAllowed(): Response {
  return problem(405, { code: "bad_request", detail: "only GET is allowed" });
}
