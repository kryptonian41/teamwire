import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync, chmodSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { buildArgs, createRunner, type CliCall } from "./cli.ts";

const root = new URL("../../fixtures/", import.meta.url).pathname;
let dir: string;
let fake: string;

// The fake picks its behaviour from FAKE_MODE and prints fixtures for good runs.
beforeAll(() => {
  dir = mkdtempSync(join(tmpdir(), "twui-cli-"));
  fake = join(dir, "teamwire");
  writeFileSync(
    fake,
    `#!/bin/sh
case "$FAKE_MODE" in
  fail) echo "boom: store unreadable" >&2; echo "second line" >&2; exit 2 ;;
  partial) cat ${root}teams.json; echo "team x unreadable" >&2; exit 6 ;;
  badjson) echo "not json"; exit 0 ;;
  shape) echo '{"teams": 5}'; exit 0 ;;
  hang) exec sleep 30 ;;
  grandchild) sleep 100 & sleep 8 ;;
  env) echo "{\\"teams\\": [], \\"leak\\": \\"\${TEAMWIRE_TOKEN}\${TEAMWIRE_ROLE}\${TEAMWIRE_TEAM}\${TEAMWIRE_TOKEN_FILE}\\"}" > ${dir}/env.out; echo '{"teams":[]}'; exit 0 ;;
  args) echo "$@" > ${dir}/args.out; echo '{"teams":[]}'; exit 0 ;;
esac
case "$*" in
  *"status"*"--role"*) echo '{"team":null,"title":null,"state":null,"lead":null,"members":[],"messages":[],"stuck_deliveries":[],"undelivered_results":[],"errors":[]}' ;;
  *"status"*) cat ${root}status.json ;;
  *"teams"*) cat ${root}teams.json ;;
  *"log"*) cat ${root}log.json ;;
  *"metrics"*) cat ${root}metrics.json ;;
  *"version"*) echo "teamwire 0.0.0" ;;
esac
`,
  );
  chmodSync(fake, 0o755);
});
afterAll(() => rmSync(dir, { recursive: true, force: true }));

function withMode<T>(mode: string, fn: () => Promise<T>): Promise<T> {
  process.env.FAKE_MODE = mode;
  return fn().finally(() => {
    delete process.env.FAKE_MODE;
  });
}

describe("buildArgs", () => {
  const cases: [CliCall, string[]][] = [
    [{ cmd: "status" }, ["status", "--json"]],
    [{ cmd: "statusRole", team: "alpha", role: "impl" }, ["status", "alpha", "--role", "impl", "--json"]],
    [{ cmd: "teams" }, ["teams", "--json"]],
    [{ cmd: "log", team: "alpha" }, ["log", "alpha", "--json"]],
    [{ cmd: "metrics" }, ["metrics", "--json"]],
    [{ cmd: "version" }, ["version"]],
  ];
  for (const [call, args] of cases) {
    test(`${call.cmd} without home`, () => expect(buildArgs(call)).toEqual(args));
    test(`${call.cmd} with home`, () => expect(buildArgs(call, "/h")).toEqual(["--home", "/h", ...args]));
  }
  test("refuses bad names", () => {
    expect(() => buildArgs({ cmd: "log", team: "../x" })).toThrow();
    expect(() => buildArgs({ cmd: "statusRole", team: "ok", role: "-x" })).toThrow();
    expect(() => buildArgs({ cmd: "log", team: "a-b" })).toThrow();
  });
});

describe("runner", () => {
  const make = (timeoutMs?: number) => createRunner({ bin: fake }, timeoutMs ? { timeoutMs } : {});

  test("parses good output for every command", async () => {
    const r = make();
    for (const call of [{ cmd: "status" }, { cmd: "teams" }, { cmd: "log", team: "alpha" }, { cmd: "metrics" }] as const) {
      const res = await r(call);
      expect(res.ok).toBe(true);
    }
    const v = await r({ cmd: "version" });
    expect(v.ok && v.data).toBe("teamwire 0.0.0");
  });

  test("refuses a bad name before starting a process", async () => {
    const res = await createRunner({ bin: "/nonexistent/never" })({ cmd: "log", team: "../x" });
    expect(res).toEqual({ ok: false, problems: [{ code: "bad_request", detail: "bad team name" }] });
  });

  test("non-zero exit with no usable stdout", async () => {
    const res = await withMode("fail", () => make()({ cmd: "teams" }));
    expect(res.ok).toBe(false);
    expect(res.problems[0]?.code).toBe("cli_failed");
    expect(res.problems[0]?.detail).toContain("boom: store unreadable");
    expect(res.problems[0]?.detail).not.toContain("second line");
  });

  test("non-zero exit with parsable stdout is data plus a problem", async () => {
    const res = await withMode("partial", () => make()({ cmd: "teams" }));
    expect(res.ok).toBe(true);
    expect(res.problems[0]?.code).toBe("cli_failed");
    expect(res.problems[0]?.detail).toContain("team x unreadable");
  });

  test("invalid JSON is shape_changed", async () => {
    const res = await withMode("badjson", () => make()({ cmd: "teams" }));
    expect(res.ok).toBe(false);
    expect(res.problems[0]?.code).toBe("shape_changed");
  });

  test("changed shape is shape_changed", async () => {
    const res = await withMode("shape", () => make()({ cmd: "teams" }));
    expect(res.ok).toBe(false);
    expect(res.problems.at(-1)?.code).toBe("shape_changed");
  });

  test("a hang is killed and reported as cli_timeout", async () => {
    const t0 = Date.now();
    const res = await withMode("hang", () => make(200)({ cmd: "teams" }));
    expect(res.ok).toBe(false);
    expect(res.problems[0]?.code).toBe("cli_timeout");
    expect(Date.now() - t0).toBeLessThan(5000);
  });

  test("a grandchild that holds the pipes open does not stall the runner", async () => {
    const t0 = Date.now();
    const res = await withMode("grandchild", () => make(500)({ cmd: "teams" }));
    expect(res.problems[0]?.code).toBe("cli_timeout");
    // Time limit 0.5 s plus the 2 s read grace period; the grandchild sleeps 100 s.
    expect(Date.now() - t0).toBeLessThan(5000);
  });

  test("identity variables are removed from the environment", async () => {
    process.env.TEAMWIRE_TOKEN = "tok";
    process.env.TEAMWIRE_ROLE = "rol";
    process.env.TEAMWIRE_TEAM = "tea";
    process.env.TEAMWIRE_TOKEN_FILE = "/tok/file";
    try {
      await withMode("env", () => make()({ cmd: "teams" }));
    } finally {
      delete process.env.TEAMWIRE_TOKEN;
      delete process.env.TEAMWIRE_ROLE;
      delete process.env.TEAMWIRE_TEAM;
      delete process.env.TEAMWIRE_TOKEN_FILE;
    }
    const out = await Bun.file(join(dir, "env.out")).text();
    expect(out).toContain('"leak": ""');
  });

  test("passes --home first", async () => {
    await withMode("args", () => createRunner({ bin: fake, home: "/h" })({ cmd: "teams" }));
    expect((await Bun.file(join(dir, "args.out")).text()).trim()).toBe("--home /h teams --json");
  });

  test("a binary that cannot start is cli_failed", async () => {
    const res = await createRunner({ bin: join(dir, "missing") })({ cmd: "teams" });
    expect(res.ok).toBe(false);
    expect(res.problems[0]?.code).toBe("cli_failed");
  });
});
