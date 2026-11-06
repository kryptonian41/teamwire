import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync, chmodSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, relative } from "node:path";
import { ACTION_TIMEOUT_MS, buildArgs, CLOSE_TIMEOUT_MS, createRunner, DEFAULT_TIMEOUT_MS, loadUserKey, STATUS_TIMEOUT_MS, timeoutFor, type CliCall, type Runner } from "./cli.ts";

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
  flood) echo $$ > ${dir}/flood.pid; exec yes ;;
  grandchild) sleep 100 & sleep 8 ;;
  env) echo "{\\"teams\\": [], \\"leak\\": \\"\${TEAMWIRE_TOKEN}\${TEAMWIRE_ROLE}\${TEAMWIRE_TEAM}\${TEAMWIRE_TOKEN_FILE}\\"}" > ${dir}/env.out; echo '{"teams":[]}'; exit 0 ;;
  args) echo "$@" > ${dir}/args.out; echo '{"teams":[]}'; exit 0 ;;
  action) echo "out line"; echo "lead copy not delivered: m007; run teamwire resend" >&2; exit \${FAKE_EXIT:-0} ;;
  keyenv) echo "[\${TEAMWIRE_USER_KEY}]" > ${dir}/key.out; exit 0 ;;
esac
case "$*" in
  *"decisions"*) cat ${root}decisions.json ;;
  *"status"*"--role"*) echo '{"team":null,"title":null,"state":null,"lead":null,"members":[],"messages":[],"stuck_deliveries":[],"undelivered_results":[],"errors":[]}' ;;
  *"status"*) cat ${root}status.json ;;
  *"teams"*) cat ${root}teams.json ;;
  *"events"*) cat ${root}events.json ;;
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
    [{ cmd: "events", team: "alpha" }, ["events", "alpha", "--json"]],
    [{ cmd: "events", team: "alpha", after: 0 }, ["events", "alpha", "--after", "0", "--json"]],
    [{ cmd: "events", team: "alpha", after: 412 }, ["events", "alpha", "--after", "412", "--json"]],
    [{ cmd: "eventsArchive", team: "alpha", archive: "alpha-2026-10-07" }, ["events", "alpha", "--archive", "alpha-2026-10-07", "--json"]],
    [{ cmd: "eventsArchive", team: "alpha", archive: "alpha-2026-10-07-153012" }, ["events", "alpha", "--archive", "alpha-2026-10-07-153012", "--json"]],
    [{ cmd: "eventsArchive", team: "alpha", archive: "alpha-2026-10-07-153012-2" }, ["events", "alpha", "--archive", "alpha-2026-10-07-153012-2", "--json"]],
    [{ cmd: "metrics" }, ["metrics", "--json"]],
    [{ cmd: "metricsTeam", team: "alpha" }, ["metrics", "alpha", "--json"]],
    [{ cmd: "decisionsTeam", team: "alpha" }, ["decisions", "alpha", "--json"]],
    [{ cmd: "decisionsGrep", text: "rounding rule" }, ["decisions", "--grep", "rounding rule", "--json"]],
    [{ cmd: "decisionsRef", ref: "linear:GKA-580" }, ["decisions", "--ref=linear:GKA-580", "--json"]],
    [{ cmd: "decisionsRef", ref: "github:#3772" }, ["decisions", "--ref=github:#3772", "--json"]],
    [{ cmd: "version" }, ["version"]],
    [{ cmd: "send", team: "alpha", to: "impl", type: "note", text: "hi" }, ["send", "alpha", "--from", "user", "--to", "impl", "--type", "note", "--text=hi"]],
    [{ cmd: "send", team: "alpha", to: "lead", type: "reprompt", text: "go" }, ["send", "alpha", "--from", "user", "--to", "lead", "--type", "reprompt", "--text=go"]],
    [{ cmd: "closeMember", team: "alpha", role: "impl", stop: false, unverified: false, dryRun: true }, ["close", "alpha", "--role", "impl", "--dry-run"]],
    [{ cmd: "closeMember", team: "alpha", role: "impl", stop: true, unverified: true, dryRun: true }, ["close", "alpha", "--role", "impl", "--stop", "--unverified", "--dry-run"]],
    [{ cmd: "closeMember", team: "alpha", role: "impl", stop: false, unverified: true, dryRun: false }, ["close", "alpha", "--role", "impl", "--unverified"]],
    [{ cmd: "closeMember", team: "alpha", role: "impl", stop: true, unverified: false, dryRun: false }, ["close", "alpha", "--role", "impl", "--stop"]],
    [{ cmd: "closeTeam", team: "alpha", dryRun: true }, ["close", "alpha", "--dry-run"]],
    [{ cmd: "closeTeam", team: "alpha", dryRun: false }, ["close", "alpha"]],
    [{ cmd: "userKey" }, ["user-key"]],
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

describe("buildArgs for the write forms", () => {
  const send = (over: Partial<Extract<CliCall, { cmd: "send" }>>): CliCall => ({ cmd: "send", team: "alpha", to: "impl", type: "note", text: "hi", ...over });
  test("a text that starts with a dash or looks like a flag stays in its one --text= item", () => {
    for (const text of ["- stop and commit", "--stop", "--from lead", "a\nb", "x".repeat(4000), "é".repeat(4000)]) {
      const args = buildArgs(send({ text }));
      expect(args.at(-1)).toBe(`--text=${text}`);
      expect(args).toHaveLength(9);
    }
  });
  test("a text is 1 to 4000 characters with no NUL", () => {
    for (const text of ["", "x".repeat(4001), "a\0b"]) expect(() => buildArgs(send({ text }))).toThrow();
  });
  test("a type other than reprompt or note is refused", () => {
    for (const type of ["answer", "question", "finding", "notice", "--note"]) expect(() => buildArgs(send({ type: type as "note" }))).toThrow();
  });
  test("team names follow teamwire's team pattern", () => {
    for (const team of ["a".repeat(13), "a_b", "Alpha", "1a", "-x", "", "a-b", "../x"]) {
      expect(() => buildArgs(send({ team }))).toThrow();
      expect(() => buildArgs({ cmd: "closeTeam", team, dryRun: true })).toThrow();
      expect(() => buildArgs({ cmd: "closeMember", team, role: "impl", stop: false, unverified: false, dryRun: true })).toThrow();
    }
    expect(buildArgs({ cmd: "closeTeam", team: "a".repeat(12), dryRun: true })[1]).toBe("a".repeat(12));
  });
  test("role names follow teamwire's role pattern", () => {
    for (const role of ["a".repeat(17), "Impl", "1a", "-x", "", "a-b", "api@other", "--stop"]) {
      expect(() => buildArgs(send({ to: role }))).toThrow();
      expect(() => buildArgs({ cmd: "closeMember", team: "alpha", role, stop: false, unverified: false, dryRun: true })).toThrow();
    }
    expect(buildArgs(send({ to: "a_b".padEnd(16, "c") }))[5]).toBe("a_bccccccccccccc");
  });
  test("a flag is added only for a real true", () => {
    const truthy = "yes" as unknown as boolean;
    expect(buildArgs({ cmd: "closeMember", team: "alpha", role: "impl", stop: truthy, unverified: truthy, dryRun: truthy })).toEqual(["close", "alpha", "--role", "impl"]);
  });
});

describe("loadUserKey", () => {
  const key = "0123456789abcdef".repeat(4);
  const keyRunner = (res: unknown): Runner => (async () => res) as unknown as Runner;
  const keyFile = (text: string, name = `user-${Math.random()}.key`) => {
    const p = join(dir, name);
    writeFileSync(p, text);
    return p;
  };
  test("reads the path line, then the file, and trims ASCII white space", async () => {
    expect(await loadUserKey(keyRunner({ ok: true, data: keyFile(`${key}\n`), problems: [] }))).toEqual({ ok: true, key });
    expect(await loadUserKey(keyRunner({ ok: true, data: keyFile(` \t${key}\r\n\n`), problems: [] }))).toEqual({ ok: true, key });
  });
  const fails = async (res: unknown) => {
    const out = await loadUserKey(keyRunner(res));
    expect(out.ok).toBe(false);
    expect(JSON.stringify(out)).not.toContain(key);
    expect(JSON.stringify(out)).not.toContain("secretish");
    return out;
  };
  test("a failed user-key is the CLI's reason", async () => {
    const out = await fails({ ok: false, problems: [{ code: "cli_failed", detail: "teamwire user-key exited 6: disk full" }] });
    expect(out).toEqual({ ok: false, detail: "teamwire user-key exited 6: disk full" });
  });
  test("stdout that is not one absolute path is refused", async () => {
    // Both files exist and hold a good key, so only the path check refuses them.
    await fails({ ok: true, data: relative(process.cwd(), keyFile(`${key}\n`)), problems: [] });
    await fails({ ok: true, data: keyFile(`${key}\n`, "two\nlines.key"), problems: [] });
    await fails({ ok: true, data: "", problems: [] });
  });
  test("a missing file is refused", () => fails({ ok: true, data: join(dir, "nope.key"), problems: [] }));
  test("a file that is not 64 lowercase hex characters is refused, and its text is not repeated", async () => {
    for (const text of ["", "secretish", key.toUpperCase(), `${key}0`, `${key}\n${key}`]) {
      await fails({ ok: true, data: keyFile(text), problems: [] });
    }
  });
});

describe("buildArgs for decisions and team metrics", () => {
  test("refuses a bad team name", () => {
    for (const team of ["../x", "a-b", "Alpha", "", "-x"]) {
      expect(() => buildArgs({ cmd: "decisionsTeam", team })).toThrow();
      expect(() => buildArgs({ cmd: "metricsTeam", team })).toThrow();
    }
  });
  test("a grep text is 1 to 200 characters, never starts with a dash, has no control character", () => {
    expect(buildArgs({ cmd: "decisionsGrep", text: "x".repeat(200) })).toEqual(["decisions", "--grep", "x".repeat(200), "--json"]);
    // Text that looks like another option stays one array item after --grep.
    expect(buildArgs({ cmd: "decisionsGrep", text: "a --json" })).toEqual(["decisions", "--grep", "a --json", "--json"]);
    for (const text of ["", "x".repeat(201), "-x", "--json", "-", "a\nb", "a\0b", "a\u007fb"]) {
      expect(() => buildArgs({ cmd: "decisionsGrep", text })).toThrow();
    }
  });
  test("a ref is provider:id, with the plan's provider and id rules, in one --ref= item", () => {
    expect(buildArgs({ cmd: "decisionsRef", ref: "a:b" })).toEqual(["decisions", "--ref=a:b", "--json"]);
    expect(buildArgs({ cmd: "decisionsRef", ref: `p23456789012345:${"i".repeat(200)}` }).at(1)).toBe(`--ref=p23456789012345:${"i".repeat(200)}`);
    // An id that starts with a dash cannot read as a flag: it sits after the equals sign.
    expect(buildArgs({ cmd: "decisionsRef", ref: "linear:-5" })).toEqual(["decisions", "--ref=linear:-5", "--json"]);
    expect(buildArgs({ cmd: "decisionsRef", ref: "gh:a:b" }).at(1)).toBe("--ref=gh:a:b");
    const bad = [
      "",
      "GKA-580", // A bare id is not a form the dashboard builds.
      "Linear:GKA-1",
      "1linear:GKA-1",
      "_x:1",
      "p234567890123456x:1", // Provider of 17 characters.
      "linear:",
      "linear:a b",
      "linear:a\tb",
      "linear:a\nb",
      "linear:a\0b",
      "linear:a\u00a0b",
      `linear:${"i".repeat(201)}`,
      "-linear:1",
      "--ref=x:1",
      ":1",
    ];
    for (const ref of bad) expect(() => buildArgs({ cmd: "decisionsRef", ref })).toThrow();
  });
});

describe("timeoutFor", () => {
  test("a decisions search reads every team and archive, so it gets the long limit", () => {
    expect(STATUS_TIMEOUT_MS).toBe(30_000);
    expect(timeoutFor({ cmd: "decisionsGrep", text: "x" })).toBe(STATUS_TIMEOUT_MS);
    expect(timeoutFor({ cmd: "decisionsRef", ref: "linear:A-1" })).toBe(STATUS_TIMEOUT_MS);
    expect(timeoutFor({ cmd: "status" })).toBe(STATUS_TIMEOUT_MS);
  });
  test("send and every dry run get 60 s, a real close 15 minutes", () => {
    expect(ACTION_TIMEOUT_MS).toBe(60_000);
    expect(CLOSE_TIMEOUT_MS).toBe(15 * 60_000);
    expect(timeoutFor({ cmd: "send", team: "a", to: "lead", type: "note", text: "x" })).toBe(ACTION_TIMEOUT_MS);
    expect(timeoutFor({ cmd: "closeMember", team: "a", role: "b", stop: false, unverified: false, dryRun: true })).toBe(ACTION_TIMEOUT_MS);
    expect(timeoutFor({ cmd: "closeTeam", team: "a", dryRun: true })).toBe(ACTION_TIMEOUT_MS);
    expect(timeoutFor({ cmd: "closeMember", team: "a", role: "b", stop: false, unverified: false, dryRun: false })).toBe(CLOSE_TIMEOUT_MS);
    expect(timeoutFor({ cmd: "closeTeam", team: "a", dryRun: false })).toBe(CLOSE_TIMEOUT_MS);
    expect(timeoutFor({ cmd: "userKey" })).toBe(DEFAULT_TIMEOUT_MS);
  });
  test("the other forms keep the short limit", () => {
    for (const call of [{ cmd: "decisionsTeam", team: "a" }, { cmd: "metricsTeam", team: "a" }, { cmd: "metrics" }, { cmd: "teams" }] as const) {
      expect(timeoutFor(call)).toBe(DEFAULT_TIMEOUT_MS);
    }
  });
});

describe("buildArgs for events", () => {
  test("refuses a bad team name", () => {
    for (const team of ["../x", "a-b", "Alpha", ""]) {
      expect(() => buildArgs({ cmd: "events", team })).toThrow();
      expect(() => buildArgs({ cmd: "eventsArchive", team, archive: "alpha-2026-10-07" })).toThrow();
    }
  });
  test("refuses an after that is negative, fractional or not a number", () => {
    for (const after of [-1, 1.5, Number.NaN, Infinity, Number.MAX_SAFE_INTEGER + 2, "3" as unknown as number]) {
      expect(() => buildArgs({ cmd: "events", team: "alpha", after })).toThrow();
    }
  });
  test("refuses an archive name that is a path or has the wrong shape", () => {
    const bad = [
      "../alpha-2026-10-07",
      "alpha-2026-10-07/x",
      "a/alpha-2026-10-07",
      "..",
      "alpha",
      "alpha-2026-10-07-1530",
      "alpha-2026-10-07-2",
      "alpha-2026-10-07-153012-",
      "alpha-2026-10-07-153012-x",
      "alpha-2026-10-07-153012-2-3",
      "alpha-2026-10-07-091500-02",
      "alpha-2026-10-07-091500-0",
      "beta-2026-10-07",
      "alpha-2026-10-07\n",
      "xalpha-2026-10-07",
      "",
    ];
    for (const archive of bad) expect(() => buildArgs({ cmd: "eventsArchive", team: "alpha", archive })).toThrow();
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
  }, 20_000);

  test("parses decisions output for the three forms, and team metrics", async () => {
    const r = make();
    for (const call of [
      { cmd: "decisionsTeam", team: "payments" },
      { cmd: "decisionsGrep", text: "status" },
      { cmd: "decisionsRef", ref: "linear:GKA-580" },
    ] as const) {
      const res = await r(call);
      expect(res.ok && res.data.teams[0]?.team).toBe("payments");
    }
    expect((await r({ cmd: "metricsTeam", team: "payments" })).ok).toBe(true);
  });

  test("parses events output for both call forms", async () => {
    const r = make();
    const live = await r({ cmd: "events", team: "payments", after: 3 });
    expect(live.ok && live.data.last_seq).toBe(55);
    const arch = await r({ cmd: "eventsArchive", team: "payments", archive: "payments-2026-10-07" });
    expect(arch.ok).toBe(true);
  });

  test("refuses a leading-zero archive suffix before starting a process", async () => {
    const run = createRunner({ bin: "/nonexistent/never" });
    for (const archive of ["alpha-2026-10-07-091500-02", "alpha-2026-10-07-091500-0"]) {
      const res = await run({ cmd: "eventsArchive", team: "alpha", archive });
      expect(res).toEqual({ ok: false, problems: [{ code: "bad_request", detail: "bad archive name" }] });
    }
    expect(buildArgs({ cmd: "eventsArchive", team: "alpha", archive: "alpha-2026-10-07-091500-2" })).toContain("alpha-2026-10-07-091500-2");
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

  test("events output that does not match EventsView is shape_changed", async () => {
    const res = await withMode("shape", () => make()({ cmd: "events", team: "alpha" }));
    expect(res.ok).toBe(false);
    expect(res.problems.at(-1)?.code).toBe("shape_changed");
    expect(res.problems.at(-1)?.detail).toContain("teamwire events");
  });

  test("a hang is killed and reported as cli_timeout", async () => {
    const t0 = Date.now();
    const res = await withMode("hang", () => make(200)({ cmd: "teams" }));
    expect(res.ok).toBe(false);
    expect(res.problems[0]?.code).toBe("cli_timeout");
    expect(Date.now() - t0).toBeLessThan(5000);
  });

  test("output past the cap is too_large and the child is killed", async () => {
    const res = await withMode("flood", () => make()({ cmd: "teams" }));
    expect(res.ok).toBe(false);
    expect(res.problems[0]?.code).toBe("too_large");
    const pid = Number((await Bun.file(join(dir, "flood.pid")).text()).trim());
    expect(() => process.kill(pid, 0)).toThrow();
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

  test("only send gets the user key, and the server's own TEAMWIRE_USER_KEY never passes", async () => {
    process.env.TEAMWIRE_USER_KEY = "from-the-server-env";
    const withKey = createRunner({ bin: fake, userKey: "k1" });
    const seen = async (call: CliCall, run = withKey) => {
      await withMode("keyenv", () => run(call));
      return (await Bun.file(join(dir, "key.out")).text()).trim();
    };
    try {
      expect(await seen({ cmd: "send", team: "alpha", to: "impl", type: "note", text: "hi" })).toBe("[k1]");
      expect(await seen({ cmd: "closeTeam", team: "alpha", dryRun: true })).toBe("[]");
      expect(await seen({ cmd: "closeMember", team: "alpha", role: "impl", stop: false, unverified: false, dryRun: true })).toBe("[]");
      expect(await seen({ cmd: "userKey" })).toBe("[]");
      expect(await seen({ cmd: "teams" })).toBe("[]");
      expect(await seen({ cmd: "send", team: "alpha", to: "impl", type: "note", text: "hi" }, make())).toBe("[]");
    } finally {
      delete process.env.TEAMWIRE_USER_KEY;
    }
  });

  test("a write form returns its exit code and both streams for any exit", async () => {
    for (const exit of [0, 1, 4, 5]) {
      process.env.FAKE_EXIT = String(exit);
      try {
        const res = await withMode("action", () => make()({ cmd: "closeTeam", team: "alpha", dryRun: true }));
        expect(res).toEqual({ ok: true, data: { exit, stdout: "out line", stderr: "lead copy not delivered: m007; run teamwire resend" }, problems: [] });
      } finally {
        delete process.env.FAKE_EXIT;
      }
    }
  });

  test("a write form past its limit is cli_timeout", async () => {
    const res = await withMode("hang", () => make(200)({ cmd: "send", team: "alpha", to: "impl", type: "note", text: "hi" }));
    expect(res.ok).toBe(false);
    expect(res.problems[0]?.code).toBe("cli_timeout");
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
