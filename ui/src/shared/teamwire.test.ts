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

describe("ticket refs on a team", () => {
  const linear = { provider: "linear", id: "GKA-580" };
  const github = { provider: "github", id: "#3772" };
  test("a payload with refs keeps them, in order", () => {
    const withRefs = { teams: [{ ...teams.teams[0]!, refs: [linear, github] }] };
    expect(TeamsList.parse(withRefs).teams[0]!.refs).toEqual([linear, github]);
    const view = StatusAll.parse([{ ...status[0]!, refs: [linear] }]);
    expect(view[0]!.refs).toEqual([linear]);
  });
  test("a payload without refs reads as an empty list", () => {
    expect("refs" in teams.teams[0]!).toBe(false);
    expect(TeamsList.parse(teams).teams[0]!.refs).toEqual([]);
    expect("refs" in status[0]!).toBe(false);
    expect(StatusAll.parse(status)[0]!.refs).toEqual([]);
  });
  test("a bare string ref, or a ref without a provider, fails the parse", () => {
    for (const bad of ["GKA-580", { id: "GKA-580" }, { provider: "linear" }]) {
      const payload = { teams: [{ ...teams.teams[0]!, refs: [bad] }] };
      expect(TeamsList.safeParse(payload).success).toBe(false);
    }
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
