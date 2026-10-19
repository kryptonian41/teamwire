import { describe, expect, test } from "bun:test";
import status from "../../fixtures/status.json";
import teams from "../../fixtures/teams.json";
import log from "../../fixtures/log.json";
import metrics from "../../fixtures/metrics.json";
import { LogView, MetricsView, StatusAll, TeamsList } from "./teamwire.ts";

describe("fixtures match the teamwire schemas", () => {
  test("status", () => expect(StatusAll.safeParse(status).success).toBe(true));
  test("teams", () => expect(TeamsList.safeParse(teams).success).toBe(true));
  test("log", () => expect(LogView.safeParse(log).success).toBe(true));
  test("metrics", () => expect(MetricsView.safeParse(metrics).success).toBe(true));
});

describe("AFK output", () => {
  const afk = {
    budget_ends_at: "2026-10-20T08:00:00.000Z",
    seconds_left: 3600,
    spent: false,
    decisions: 2,
  };
  test("status accepts an afk object and keeps it", () => {
    const parsed = StatusAll.parse([{ ...status[0]!, afk }]);
    expect(parsed[0]!.afk).toEqual(afk);
  });
  test("status accepts a null afk and an older output without the key", () => {
    expect(StatusAll.parse([{ ...status[0]!, afk: null }])[0]!.afk).toBeNull();
    expect(StatusAll.parse([status[0]!])[0]!.afk).toBeUndefined();
  });
  test("a spent budget with a text seconds_left fails the parse", () => {
    const bad = { ...status[0]!, afk: { ...afk, seconds_left: "0" } };
    expect(StatusAll.safeParse([bad]).success).toBe(false);
  });
});

describe("schemas refuse a changed shape", () => {
  test("a new delivery state fails the parse", () => {
    const changed = structuredClone(log);
    changed.messages[0]!.deliveries[0]!.state = "teleported";
    expect(LogView.safeParse(changed).success).toBe(false);
  });
  test("a new key is accepted and dropped", () => {
    const added = { teams: [{ ...teams.teams[0]!, colour: "red" }] };
    const parsed = TeamsList.parse(added);
    expect("colour" in parsed.teams[0]!).toBe(false);
  });
});

// Reads the real teamwire on this machine. Opt in with TW_UI_LIVE=1, so a
// member's check never depends on which teams happen to be running.
describe.if(process.env.TW_UI_LIVE === "1")("real teamwire output", () => {
  const run = (args: string[]) => {
    const out = Bun.spawnSync(["teamwire", ...args], { stdout: "pipe", stderr: "pipe" });
    return JSON.parse(out.stdout.toString());
  };
  test("status --json", () => void StatusAll.parse(run(["status", "--json"])));
  test("teams --json", () => void TeamsList.parse(run(["teams", "--json"])));
  test("metrics --json", () => void MetricsView.parse(run(["metrics", "--json"])));
});
