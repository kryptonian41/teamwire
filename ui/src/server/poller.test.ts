import { describe, expect, test } from "bun:test";
import type { Runner } from "./cli.ts";
import { createPoller } from "./poller.ts";

const team = { name: "a", title: "A", state: "open", members: [] } as const;

function fakeRunner(script: { status: () => Promise<any>; teams: () => Promise<any> }): Runner {
  return ((call: { cmd: "status" | "teams" }) => script[call.cmd]()) as unknown as Runner;
}
const ok = (data: unknown) => Promise.resolve({ ok: true, data, problems: [] });
const bad = (detail: string) => Promise.resolve({ ok: false, problems: [{ code: "cli_failed", detail }] });

describe("poller", () => {
  test("builds a snapshot and notifies subscribers", async () => {
    let t = 1_000;
    const p = createPoller({
      run: fakeRunner({ status: () => ok([]), teams: () => ok({ teams: [team] }) }),
      intervalMs: 5000,
      now: () => (t += 10),
    });
    const seen: unknown[] = [];
    p.subscribe((s) => seen.push(s));
    const s = await p.poll();
    expect(s?.stale).toBe(false);
    expect(s?.teams).toHaveLength(1);
    expect(s?.durationMs).toBe(10);
    expect(seen).toHaveLength(1);
    expect(p.latest()).toBe(s);
  });

  test("the snapshot says whether actions are on, and off by default", async () => {
    const run = fakeRunner({ status: () => ok([]), teams: () => ok({ teams: [] }) });
    expect((await createPoller({ run, intervalMs: 5000 }).poll())?.actions).toBe(false);
    expect((await createPoller({ run, intervalMs: 5000, actions: true }).poll())?.actions).toBe(true);
  });

  test("skips a poll while another runs", async () => {
    let release!: () => void;
    let calls = 0;
    const p = createPoller({
      run: fakeRunner({
        status: () => {
          calls++;
          return new Promise((r) => (release = () => r({ ok: true, data: [], problems: [] })));
        },
        teams: () => ok({ teams: [] }),
      }),
      intervalMs: 1,
    });
    const first = p.poll();
    expect(await p.poll()).toBeNull();
    release();
    expect(await first).not.toBeNull();
    expect(calls).toBe(1);
  });

  test("a failed poll keeps old data, sets stale; a later good poll clears it", async () => {
    let fail = false;
    const p = createPoller({
      run: fakeRunner({
        status: () => (fail ? bad("status down") : ok([])),
        teams: () => (fail ? bad("teams down") : ok({ teams: [team] })),
      }),
      intervalMs: 1,
    });
    await p.poll();
    fail = true;
    const stale = await p.poll();
    expect(stale?.stale).toBe(true);
    expect(stale?.teams).toHaveLength(1);
    expect(stale?.problems.map((x) => x.detail)).toEqual(["status down", "teams down"]);
    fail = false;
    const good = await p.poll();
    expect(good?.stale).toBe(false);
    expect(good?.problems).toEqual([]);
  });

  test("first poll failing gives empty stale data", async () => {
    const p = createPoller({ run: fakeRunner({ status: () => bad("x"), teams: () => bad("y") }), intervalMs: 1 });
    const s = await p.poll();
    expect(s).toMatchObject({ stale: true, status: [], teams: [] });
  });

  test("unsubscribe stops notifications", async () => {
    const p = createPoller({ run: fakeRunner({ status: () => ok([]), teams: () => ok({ teams: [] }) }), intervalMs: 1 });
    let n = 0;
    const off = p.subscribe(() => n++);
    await p.poll();
    off();
    await p.poll();
    expect(n).toBe(1);
  });

  test("lastGoodAt: null on a first failure, kept on a later failure, moved by a good poll", async () => {
    let t = Date.parse("2026-10-13T10:00:00Z");
    let fail = true;
    const p = createPoller({
      run: fakeRunner({ status: () => (fail ? bad("x") : ok([])), teams: () => ok({ teams: [] }) }),
      intervalMs: 1,
      now: () => t,
    });
    expect(await p.poll()).toMatchObject({ stale: true, lastGoodAt: null });
    fail = false;
    t += 60_000;
    const good1 = await p.poll();
    expect(good1?.lastGoodAt).toBe("2026-10-13T10:01:00.000Z");
    fail = true;
    t += 60_000;
    expect(await p.poll()).toMatchObject({ stale: true, lastGoodAt: "2026-10-13T10:01:00.000Z" });
    fail = false;
    t += 60_000;
    expect((await p.poll())?.lastGoodAt).toBe("2026-10-13T10:03:00.000Z");
  });
});
