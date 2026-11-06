import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { Snapshot } from "../shared/api.ts";
import { createApp } from "./app.ts";
import { buildArgs, type CliCall, type Runner } from "./cli.ts";
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

type Act = (call: CliCall) => unknown;
function setup(opts: { keepaliveMs?: number; metricsTtlMs?: number; now?: () => number; failEvents?: boolean; actions?: boolean; act?: Act } = {}) {
  const { failEvents, act, ...appOpts } = opts;
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
      case "metricsTeam":
        return ok({ metrics: [{ name: "teamwire.member.tokens", unit: "{token}", labels: [["team", call.team]], value: 1 }], errors: [] });
      case "decisionsTeam":
        return ok({ teams: [{ team: call.team, title: "t", created_at: "x", live: true, members: [] }], errors: [] });
      case "decisionsGrep":
      case "decisionsRef":
        // The real runner refuses a bad form before it starts a process; the fake uses the same check.
        try {
          buildArgs(call);
        } catch (e) {
          return { ok: false, problems: [{ code: "bad_request", detail: (e as Error).message }] };
        }
        return ok({ teams: [], errors: [] });
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
        return act ? act(call) : { ok: false, problems: [] };
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
    for (const p of ["/api/events", "/api/metrics", "/api/archives", "/api/teams/alpha/log", "/api/teams/alpha/files", "/api/teams/alpha/decisions", "/api/teams/alpha/metrics", "/api/decisions?grep=x", "/api/nope"]) {
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

const ACTION_PATHS = ["/api/teams/alpha/actions/send", "/api/teams/alpha/actions/close-member", "/api/teams/alpha/actions/close-team"];

describe("actions off (the default)", () => {
  test("every action route is 404 for every method, and no command runs", async () => {
    const { app, calls } = setup();
    for (const path of ACTION_PATHS) {
      for (const method of ["GET", "POST", "PUT", "DELETE"]) {
        const res = await app.handle(
          new Request(`http://127.0.0.1:${port}${path}`, {
            method,
            headers: { host: `127.0.0.1:${port}`, origin: `http://127.0.0.1:${port}`, cookie: `tw_ui_7878=${token}`, "content-type": "application/json" },
            body: method === "GET" ? null : JSON.stringify({ to: "lead", type: "note", text: "hi", dryRun: true, confirmName: "alpha", role: "impl", stop: false, unverified: false }),
          }),
        );
        expect(res.status).toBe(404);
      }
    }
    expect(calls).toHaveLength(0);
  });
});

const writeHeaders = { host: `127.0.0.1:${port}`, origin: `http://127.0.0.1:${port}`, cookie: `tw_ui_7878=${token}`, "content-type": "application/json" };
function post(app: { handle(r: Request): Promise<Response> }, path: string, body: unknown, headers: Record<string, string> = writeHeaders, method = "POST") {
  return app.handle(new Request(`http://127.0.0.1:${port}${path}`, { method, headers, body: typeof body === "string" ? body : JSON.stringify(body) }));
}
const without = (key: string) => Object.fromEntries(Object.entries(writeHeaders).filter(([k]) => k !== key));

describe("the write check on action routes", () => {
  const sendPath = "/api/teams/alpha/actions/send";
  const goodSend = { to: "impl", type: "note", text: "hello" };
  const refused = async (res: Promise<Response>, status: number, calls: CliCall[]) => {
    const r = await res;
    expect(r.status).toBe(status);
    expect(calls).toHaveLength(0);
  };
  test("GET is 405 on every action route", async () => {
    const { app, calls } = setup({ actions: true });
    for (const path of ACTION_PATHS) {
      const res = await app.handle(new Request(`http://127.0.0.1:${port}${path}`, { headers: writeHeaders }));
      expect(res.status).toBe(405);
    }
    expect(calls).toHaveLength(0);
  });
  test("PUT, PATCH and DELETE are 405", async () => {
    const { app, calls } = setup({ actions: true });
    for (const method of ["PUT", "PATCH", "DELETE"]) await refused(post(app, sendPath, goodSend, writeHeaders, method), 405, calls);
  });
  test("a non-action route still refuses POST with 405", async () => {
    const { app, calls } = setup({ actions: true });
    await refused(post(app, "/api/teams/alpha/log", {}), 405, calls);
  });
  test("403 without the token cookie", async () => {
    const { app, calls } = setup({ actions: true });
    await refused(post(app, sendPath, goodSend, without("cookie")), 403, calls);
  });
  test("403 with a wrong Host", async () => {
    const { app, calls } = setup({ actions: true });
    await refused(post(app, sendPath, goodSend, { ...writeHeaders, host: "evil.example:7878" }), 403, calls);
  });
  test("403 without an Origin", async () => {
    const { app, calls } = setup({ actions: true });
    await refused(post(app, sendPath, goodSend, without("origin")), 403, calls);
  });
  test("403 with a foreign Origin", async () => {
    const { app, calls } = setup({ actions: true });
    await refused(post(app, sendPath, goodSend, { ...writeHeaders, origin: "http://evil.example" }), 403, calls);
  });
  test("400 without a JSON content type", async () => {
    const { app, calls } = setup({ actions: true });
    await refused(post(app, sendPath, goodSend, { ...writeHeaders, "content-type": "text/plain" }), 400, calls);
  });
  test("400 for a body that is not JSON", async () => {
    const { app, calls } = setup({ actions: true });
    await refused(post(app, sendPath, "{to: impl"), 400, calls);
  });
  test("400 for a send body the schema refuses", async () => {
    const { app, calls } = setup({ actions: true });
    for (const body of [
      {},
      { ...goodSend, extra: 1 },
      { ...goodSend, to: "Bad-Role" },
      { ...goodSend, to: "api@other" },
      { ...goodSend, type: "answer" },
      { ...goodSend, text: "" },
      { ...goodSend, text: "x".repeat(4001) },
      { ...goodSend, text: "a\u0000b" },
    ]) {
      await refused(post(app, sendPath, body), 400, calls);
    }
  });
  test("400 for a close-member body the schema refuses", async () => {
    const { app, calls } = setup({ actions: true });
    const good = { role: "impl", stop: false, unverified: false, dryRun: true };
    for (const body of [{ ...good, role: "-x" }, { ...good, stop: "yes" }, { role: "impl" }]) {
      await refused(post(app, "/api/teams/alpha/actions/close-member", body), 400, calls);
    }
  });
  test("400 for a real team close whose confirm name is not the team", async () => {
    const { app, calls } = setup({ actions: true });
    for (const body of [{ dryRun: false, confirmName: "alph" }, { dryRun: false, confirmName: "" }, { dryRun: false }]) {
      await refused(post(app, "/api/teams/alpha/actions/close-team", body), 400, calls);
    }
  });
  test("404 for a team that does not exist", async () => {
    const { app, calls } = setup({ actions: true });
    await refused(post(app, "/api/teams/nope/actions/send", goodSend), 404, calls);
  });
});

const out = (exit: number, stdout = "", stderr = "") => ok({ exit, stdout, stderr });

describe("send action", () => {
  const path = "/api/teams/alpha/actions/send";
  test("runs one send as the user and returns its exit code and streams; no extra lead note", async () => {
    const { app, calls } = setup({ actions: true, act: () => out(0, "m009\n  impl: handed_off\n  lead: handed_off", "") });
    const res = await post(app, path, { to: "impl", type: "reprompt", text: "- stop and commit" });
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({ exit: 0, stdout: "m009\n  impl: handed_off\n  lead: handed_off", stderr: "", leadNotes: [] });
    expect(calls).toEqual([{ cmd: "send", team: "alpha", to: "impl", type: "reprompt", text: "- stop and commit" }]);
  });
  test("a note to the lead is one send to lead", async () => {
    const { app, calls } = setup({ actions: true, act: () => out(0) });
    expect((await post(app, path, { to: "lead", type: "note", text: "hi" })).status).toBe(200);
    expect(calls).toEqual([{ cmd: "send", team: "alpha", to: "lead", type: "note", text: "hi" }]);
  });
  test("a non-zero exit is still 200, with the CLI's text", async () => {
    for (const exit of [1, 3, 4, 5, 6]) {
      const { app } = setup({ actions: true, act: () => out(exit, "", "REFUSED: recipient_not_allowed") });
      const res = await post(app, path, { to: "rev", type: "note", text: "hi" });
      expect(res.status).toBe(200);
      expect(await res.json()).toMatchObject({ exit, stderr: "REFUSED: recipient_not_allowed" });
    }
  });
  test("a send past its time limit is 200 with exit null: the outcome is unknown", async () => {
    const { app } = setup({ actions: true, act: () => ({ ok: false, problems: [{ code: "cli_timeout", detail: "teamwire send passed 60000 ms and was killed" }] }) });
    const res = await post(app, path, { to: "impl", type: "note", text: "hi" });
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({ exit: null, stdout: "", stderr: "", leadNotes: [] });
  });
  test("a form the runner refuses is 400, and a CLI that cannot start is 502", async () => {
    const refused = setup({ actions: true, act: () => ({ ok: false, problems: [{ code: "bad_request", detail: "bad team name" }] }) });
    expect((await post(refused.app, path, { to: "impl", type: "note", text: "hi" })).status).toBe(400);
    const broken = setup({ actions: true, act: () => ({ ok: false, problems: [{ code: "cli_failed", detail: "could not start teamwire" }] }) });
    expect((await post(broken.app, path, { to: "impl", type: "note", text: "hi" })).status).toBe(502);
  });
});

/** Answers each write call from a script, by command. */
const script = (byCmd: Partial<Record<CliCall["cmd"], (call: CliCall) => unknown>>): Act => (call) => byCmd[call.cmd]?.(call) ?? { ok: false, problems: [] };
const note = (team: string, text: string): CliCall => ({ cmd: "send", team, to: "lead", type: "note", text });

describe("close-member action", () => {
  const path = "/api/teams/alpha/actions/close-member";
  const body = (over: Record<string, boolean> = {}) => ({ role: "impl", stop: false, unverified: false, dryRun: true, ...over });
  test("a dry run runs only the dry-run form and tells nobody", async () => {
    const { app, calls } = setup({ actions: true, act: script({ closeMember: () => out(0, "[dry-run] would remove impl") }) });
    const res = await post(app, path, body({ stop: true }));
    expect(await res.json()).toEqual({ exit: 0, stdout: "[dry-run] would remove impl", stderr: "", leadNotes: [] });
    expect(calls).toEqual([{ cmd: "closeMember", team: "alpha", role: "impl", stop: true, unverified: false, dryRun: true }]);
  });
  test("a successful real close is followed by one note to the lead that names the flags", async () => {
    for (const [flags, suffix] of [
      [{}, ""],
      [{ stop: true }, " (--stop)"],
      [{ unverified: true }, " (--unverified)"],
      [{ stop: true, unverified: true }, " (--stop --unverified)"],
    ] as const) {
      const { app, calls } = setup({ actions: true, act: script({ closeMember: () => out(0, "removed impl"), send: () => out(0, "m010") }) });
      const res = await post(app, path, body({ ...flags, dryRun: false }));
      const text = `user closed member impl from the dashboard${suffix}`;
      expect(await res.json()).toEqual({ exit: 0, stdout: "removed impl", stderr: "", leadNotes: [{ text, told: true, detail: "" }] });
      expect(calls).toEqual([{ cmd: "closeMember", team: "alpha", role: "impl", stop: false, unverified: false, dryRun: false, ...flags }, note("alpha", text)]);
    }
  });
  test("a refused or timed-out real close sends no note", async () => {
    for (const close of [() => out(1, "", "REFUSED: impl is busy"), () => ({ ok: false, problems: [{ code: "cli_timeout", detail: "passed" }] })]) {
      const { app, calls } = setup({ actions: true, act: script({ closeMember: close, send: () => out(0) }) });
      const res = (await (await post(app, path, body({ dryRun: false }))).json()) as { leadNotes: unknown[] };
      expect(res.leadNotes).toEqual([]);
      expect(calls.map((c) => c.cmd)).toEqual(["closeMember"]);
    }
  });
  test("a note that fails still returns the close's own result, and says why the lead was not told", async () => {
    const { app } = setup({ actions: true, act: script({ closeMember: () => out(0, "removed impl"), send: () => out(4, "", "NOT DELIVERED to lead: gone") }) });
    expect(await (await post(app, path, body({ dryRun: false }))).json()).toEqual({
      exit: 0,
      stdout: "removed impl",
      stderr: "",
      leadNotes: [{ text: "user closed member impl from the dashboard", told: false, detail: "teamwire send exited 4: NOT DELIVERED to lead: gone" }],
    });
    const timedOut = setup({ actions: true, act: script({ closeMember: () => out(0), send: () => ({ ok: false, problems: [{ code: "cli_timeout", detail: "teamwire send passed 60000 ms and was killed" }] }) }) });
    expect(((await (await post(timedOut.app, path, body({ dryRun: false }))).json()) as { leadNotes: unknown[] }).leadNotes).toEqual([
      { text: "user closed member impl from the dashboard", told: false, detail: "teamwire send passed 60000 ms and was killed" },
    ]);
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

describe("team metrics", () => {
  test("200 with the runner's data for a known team, 404 for an unknown or bad name, with no CLI call", async () => {
    const { get, calls } = setup();
    const res = await get("/api/teams/alpha/metrics");
    expect(res.status).toBe(200);
    expect(await res.json()).toMatchObject({ metrics: [{ name: "teamwire.member.tokens" }] });
    expect(calls).toEqual([{ cmd: "metricsTeam", team: "alpha" }]);
    calls.length = 0;
    for (const team of ["nope", "..%2Fx", "Alpha"]) expect((await get(`/api/teams/${team}/metrics`)).status).toBe(404);
    expect(calls).toHaveLength(0);
  });
  test("each team is cached for the ttl on its own, and the all-teams cache is a separate one", async () => {
    let t = 0;
    const { get, calls } = setup({ now: () => t, metricsTtlMs: 60_000 });
    const count = (cmd: string) => calls.filter((c) => c.cmd === cmd).length;
    await get("/api/teams/alpha/metrics");
    t = 59_000;
    await get("/api/teams/alpha/metrics");
    expect(count("metricsTeam")).toBe(1);
    await get("/api/metrics");
    expect(count("metrics")).toBe(1);
    t = 61_000;
    await get("/api/teams/alpha/metrics");
    expect(count("metricsTeam")).toBe(2);
  });
  test("a failure is not cached", async () => {
    let fail = true;
    const calls: CliCall[] = [];
    const run = (async (call: CliCall) => {
      calls.push(call);
      return fail ? { ok: false, problems: [{ code: "cli_failed", detail: "teamwire metrics exited 6: boom" }] } : ok({ metrics: [], errors: [] });
    }) as unknown as Runner;
    const app = createApp({ security: createSecurity({ token, port }), poller: createPoller({ run, intervalMs: 5000 }), run, home });
    const get = () => app.handle(new Request(`http://127.0.0.1:${port}/api/teams/alpha/metrics`, { headers: { host: `127.0.0.1:${port}`, cookie: `tw_ui_7878=${token}` } }));
    expect((await get()).status).toBe(502);
    fail = false;
    expect((await get()).status).toBe(200);
    expect(calls).toHaveLength(2);
  });
});

describe("decisions routes", () => {
  test("a team's decisions run the CLI for a known team only", async () => {
    const { get, calls } = setup();
    const res = await get("/api/teams/alpha/decisions");
    expect(res.status).toBe(200);
    expect(await res.json()).toMatchObject({ teams: [{ team: "alpha" }] });
    expect(calls).toEqual([{ cmd: "decisionsTeam", team: "alpha" }]);
    calls.length = 0;
    expect((await get("/api/teams/nope/decisions")).status).toBe(404);
    expect(calls).toHaveLength(0);
  });
  test("grep and ref searches pass their text to the runner as given", async () => {
    const { get, calls } = setup();
    expect((await get(`/api/decisions?grep=${encodeURIComponent("rounding & more")}`)).status).toBe(200);
    expect((await get(`/api/decisions?ref=${encodeURIComponent("github:#3772")}`)).status).toBe(200);
    expect(calls).toEqual([
      { cmd: "decisionsGrep", text: "rounding & more" },
      { cmd: "decisionsRef", ref: "github:#3772" },
    ]);
  });
  test("400 for neither or both of grep and ref, and for text the CLI form refuses", async () => {
    const { get } = setup();
    for (const q of ["", "?grep=a&ref=b:c", "?grep=", `?grep=${"x".repeat(201)}`, "?grep=-x", "?grep=a%0Ab", "?ref=nocolon", "?ref=Linear:1", "?ref=linear:a%20b", "?other=1"]) {
      const res = await get(`/api/decisions${q}`);
      expect(res.status).toBe(400);
      expect(await res.json()).toMatchObject({ code: "bad_request" });
    }
  });
  test("502 when the CLI fails", async () => {
    const run = (async () => ({ ok: false, problems: [{ code: "cli_failed", detail: "teamwire decisions exited 1: boom" }] })) as unknown as Runner;
    const app = createApp({ security: createSecurity({ token, port }), poller: createPoller({ run, intervalMs: 5000 }), run, home });
    const res = await app.handle(new Request(`http://127.0.0.1:${port}/api/decisions?grep=x`, { headers: { host: `127.0.0.1:${port}`, cookie: `tw_ui_7878=${token}` } }));
    expect(res.status).toBe(502);
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
