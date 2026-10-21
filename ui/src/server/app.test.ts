import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { Snapshot } from "../shared/api.ts";
import { createApp } from "./app.ts";
import type { CliCall, Runner } from "./cli.ts";
import { createPoller } from "./poller.ts";
import { createSecurity } from "./security.ts";

const token = "cd".repeat(32);
const port = 7878;
let home: string;

beforeAll(() => {
  home = mkdtempSync(join(tmpdir(), "twui-app-"));
  const team = join(home, "teams", "alpha");
  mkdirSync(join(team, "briefs"), { recursive: true });
  writeFileSync(join(team, "team.db"), "");
  writeFileSync(join(team, "briefs", "impl.md"), "do it");
  mkdirSync(join(home, "archive", "alpha-2026-10-07", "reports"), { recursive: true });
  writeFileSync(join(home, "archive", "alpha-2026-10-07", "reports", "impl.md"), "archived");
});
afterAll(() => rmSync(home, { recursive: true, force: true }));

const ok = (data: unknown, problems: unknown[] = []) => ({ ok: true, data, problems });
const emptyStatus = { team: "alpha", title: null, state: null, lead: null, messages: [], stuck_deliveries: [], undelivered_results: [] };

function setup(opts: { keepaliveMs?: number; metricsTtlMs?: number; now?: () => number; failEvents?: boolean } = {}) {
  const { failEvents, ...appOpts } = opts;
  const calls: CliCall[] = [];
  const run = (async (call: CliCall) => {
    calls.push(call);
    switch (call.cmd) {
      case "status":
        return ok([]);
      case "teams":
        return ok({ teams: [] });
      case "metrics":
        return ok({ metrics: [], errors: [] });
      case "log":
        return ok({ messages: [] });
      case "events":
      case "eventsArchive":
        if (failEvents) return { ok: false, problems: [{ code: "cli_failed", detail: "teamwire events exited 1: boom" }] };
        return ok({ team: call.team, source: call.cmd === "events" ? "live" : "archive", archive: null, last_seq: 7, events: [], errors: [] });
      case "statusRole":
        return ok({
          ...emptyStatus,
          members: [{ role: "impl", title: "t", state: "working", generation: 1, age_s: 0, deadline_s: 0, result: null, branch: null, resume_command: "claude --resume abc" }],
          errors: ["herdr unreachable"],
        });
      default:
        return { ok: false, problems: [] };
    }
  }) as unknown as Runner;
  const poller = createPoller({ run, intervalMs: 5000 });
  const app = createApp({ security: createSecurity({ token, port }), poller, run, home, ...appOpts });
  const get = (path: string, headers: Record<string, string> = { cookie: `tw_ui_7878=${token}` }) =>
    app.handle(new Request(`http://127.0.0.1:${port}${path}`, { headers: { host: `127.0.0.1:${port}`, ...headers } }));
  return { app, poller, get, calls };
}

describe("access", () => {
  test("403 without the cookie", async () => {
    const { get } = setup();
    const res = await get("/api/snapshot", {});
    expect(res.status).toBe(403);
    expect(await res.json()).toMatchObject({ code: "forbidden" });
  });
  test("403 with a wrong Host", async () => {
    const { get } = setup();
    expect((await get("/api/snapshot", { cookie: `tw_ui_7878=${token}`, host: "evil.example:7878" })).status).toBe(403);
  });
  test("403 with a foreign Origin", async () => {
    const { get } = setup();
    expect((await get("/api/snapshot", { cookie: `tw_ui_7878=${token}`, origin: "https://evil.example" })).status).toBe(403);
  });
  test("every API route is protected", async () => {
    const { get } = setup();
    for (const p of ["/api/events", "/api/metrics", "/api/archives", "/api/teams/alpha/log", "/api/teams/alpha/files", "/api/nope"]) {
      expect((await get(p, {})).status).toBe(403);
    }
  });
  test("/auth redirects with the cookie", async () => {
    const { get } = setup();
    const res = await get(`/auth?token=${token}`, {});
    expect(res.status).toBe(303);
    expect(res.headers.get("set-cookie")).toContain("HttpOnly");
  });
  test("non-GET is refused", async () => {
    const { app } = setup();
    const res = await app.handle(new Request(`http://127.0.0.1:${port}/api/snapshot`, { method: "POST", headers: { host: `127.0.0.1:${port}`, cookie: `tw_ui_7878=${token}` } }));
    expect(res.status).toBe(405);
  });
});

describe("snapshot", () => {
  test("200 after a poll, 503 before", async () => {
    const { get, poller } = setup();
    expect((await get("/api/snapshot")).status).toBe(503);
    await poller.poll();
    const res = await get("/api/snapshot");
    expect(res.status).toBe(200);
    expect(((await res.json()) as Snapshot).stale).toBe(false);
  });
});

describe("team routes", () => {
  test("unknown team and bad team name are 404", async () => {
    const { get, calls } = setup();
    expect((await get("/api/teams/nope/log")).status).toBe(404);
    expect((await get("/api/teams/..%2Fx/log")).status).toBe(404);
    expect((await get("/api/teams/Alpha/files")).status).toBe(404);
    expect(calls).toHaveLength(0);
  });
  test("log runs the CLI for a known team", async () => {
    const { get, calls } = setup();
    expect((await get("/api/teams/alpha/log")).status).toBe(200);
    expect(calls).toEqual([{ cmd: "log", team: "alpha" }]);
  });
  test("file: 200, 404 when missing, 400 for a bad kind or name", async () => {
    const { get } = setup();
    const ok = await get("/api/teams/alpha/file/brief/impl");
    expect(ok.status).toBe(200);
    expect(await ok.json()).toMatchObject({ kind: "brief", name: "impl", text: "do it", truncated: false });
    expect((await get("/api/teams/alpha/file/brief/ghost")).status).toBe(404);
    expect((await get("/api/teams/alpha/file/members/impl")).status).toBe(400);
    expect((await get("/api/teams/alpha/file/brief/..%2F..%2Fteam")).status).toBe(400);
  });
  test("files lists what exists", async () => {
    const { get } = setup();
    expect(await (await get("/api/teams/alpha/files")).json()).toEqual([expect.objectContaining({ kind: "brief", name: "impl" })]);
  });
  test("resume returns the command and turns errors into problems", async () => {
    const { get, calls } = setup();
    const res = await get("/api/teams/alpha/resume/impl");
    expect(await res.json()).toEqual({ command: "claude --resume abc", problems: [{ code: "cli_failed", detail: "herdr unreachable" }] });
    expect(calls).toEqual([{ cmd: "statusRole", team: "alpha", role: "impl" }]);
    expect((await get("/api/teams/alpha/resume/ghost")).status).toBe(404);
    expect((await get("/api/teams/alpha/resume/Bad-Role")).status).toBe(400);
  });
});

describe("archives", () => {
  test("lists and reads", async () => {
    const { get } = setup();
    const list = (await (await get("/api/archives")).json()) as { dir: string }[];
    expect(list.map((a) => a.dir)).toEqual(["alpha-2026-10-07"]);
    const f = await get("/api/archives/alpha-2026-10-07/file/report/impl");
    expect(await f.json()).toMatchObject({ text: "archived" });
    expect((await get("/api/archives/nope-2026-01-01/file/report/impl")).status).toBe(404);
    expect((await get("/api/archives/alpha-2026-10-07/file/report/ghost")).status).toBe(404);
    expect((await get("/api/archives/alpha-2026-10-07/file/bogus/impl")).status).toBe(400);
    expect((await get("/api/archives/..%2Fteams/file/brief/impl")).status).toBe(404);
  });
});

describe("timeline routes", () => {
  test("live: 200 with the runner's data, with and without after", async () => {
    const { get, calls } = setup();
    const res = await get("/api/teams/alpha/timeline");
    expect(res.status).toBe(200);
    expect(await res.json()).toMatchObject({ team: "alpha", source: "live", last_seq: 7 });
    expect((await get("/api/teams/alpha/timeline?after=0")).status).toBe(200);
    expect((await get("/api/teams/alpha/timeline?after=41")).status).toBe(200);
    expect(calls).toEqual([
      { cmd: "events", team: "alpha" },
      { cmd: "events", team: "alpha", after: 0 },
      { cmd: "events", team: "alpha", after: 41 },
    ]);
  });
  test("live: 400 for an after that is negative, fractional or not a number, with no CLI call", async () => {
    const { get, calls } = setup();
    for (const after of ["-1", "abc", "1.5", "", "1e3", "0x10", "%201", "99999999999999999999"]) {
      const res = await get(`/api/teams/alpha/timeline?after=${after}`);
      expect(res.status).toBe(400);
      expect(await res.json()).toMatchObject({ code: "bad_request" });
    }
    expect(calls).toHaveLength(0);
  });
  test("live: 404 for an unknown team, with no CLI call", async () => {
    const { get, calls } = setup();
    expect((await get("/api/teams/nope/timeline")).status).toBe(404);
    expect((await get("/api/teams/..%2Fx/timeline")).status).toBe(404);
    expect(calls).toHaveLength(0);
  });
  test("live: 502 when the CLI fails", async () => {
    const { get } = setup({ failEvents: true });
    const res = await get("/api/teams/alpha/timeline");
    expect(res.status).toBe(502);
    expect(await res.json()).toMatchObject({ code: "cli_failed" });
  });
  test("archive: 200 with the team taken from the archive name", async () => {
    const { get, calls } = setup();
    const res = await get("/api/archives/alpha-2026-10-07/timeline");
    expect(res.status).toBe(200);
    expect(calls).toEqual([{ cmd: "eventsArchive", team: "alpha", archive: "alpha-2026-10-07" }]);
  });
  test("archive: 404 for a name that is not listed, and no CLI call for a refused name", async () => {
    const { get, calls } = setup();
    for (const name of ["nope-2026-01-01", "..%2Fteams", "alpha-2026-10-07%2F..", "bad-name", "alpha"]) {
      expect((await get(`/api/archives/${name}/timeline`)).status).toBe(404);
    }
    expect(calls).toHaveLength(0);
  });
  test("archive: 502 when the CLI fails", async () => {
    const { get } = setup({ failEvents: true });
    expect((await get("/api/archives/alpha-2026-10-07/timeline")).status).toBe(502);
  });
  test("both routes need the cookie", async () => {
    const { get } = setup();
    expect((await get("/api/teams/alpha/timeline", {})).status).toBe(403);
    expect((await get("/api/archives/alpha-2026-10-07/timeline", {})).status).toBe(403);
  });
});

describe("metrics cache", () => {
  test("a second call within the ttl does not run the CLI", async () => {
    let t = 0;
    const { get, calls } = setup({ now: () => t, metricsTtlMs: 60_000 });
    await get("/api/metrics");
    t = 59_000;
    await get("/api/metrics");
    expect(calls.filter((c) => c.cmd === "metrics")).toHaveLength(1);
    t = 61_000;
    await get("/api/metrics");
    expect(calls.filter((c) => c.cmd === "metrics")).toHaveLength(2);
  });
});

describe("events", () => {
  async function readChunk(reader: ReadableStreamDefaultReader<Uint8Array>) {
    return new TextDecoder().decode((await reader.read()).value);
  }
  test("sends the latest snapshot at once, then one per poll, then keepalives", async () => {
    const { get, poller } = setup({ keepaliveMs: 20 });
    await poller.poll();
    const res = await get("/api/events");
    expect(res.headers.get("content-type")).toBe("text/event-stream");
    const reader = res.body!.getReader();
    const first = await readChunk(reader);
    expect(first).toStartWith("event: snapshot\ndata: ");
    await poller.poll();
    expect(await readChunk(reader)).toStartWith("event: snapshot\ndata: ");
    expect(await readChunk(reader)).toBe(": keepalive\n\n");
    await reader.cancel();
  });
  test("removes the subscriber when the client goes away", async () => {
    const { get, poller } = setup({ keepaliveMs: 1000 });
    let subscribed = 0;
    const origSubscribe = poller.subscribe;
    let active = 0;
    poller.subscribe = (fn) => {
      subscribed++;
      active++;
      const off = origSubscribe(fn);
      return () => {
        active--;
        off();
      };
    };
    const res = await get("/api/events");
    await res.body!.cancel();
    expect(subscribed).toBe(1);
    expect(active).toBe(0);
  });
});
