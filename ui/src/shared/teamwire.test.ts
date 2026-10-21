import { describe, expect, test } from "bun:test";
import status from "../../fixtures/status.json";
import teams from "../../fixtures/teams.json";
import log from "../../fixtures/log.json";
import metrics from "../../fixtures/metrics.json";
import events from "../../fixtures/events.json";
import { createRunner } from "../server/cli.ts";
import { EventsView, LogView, MetricsView, StatusAll, TeamsList } from "./teamwire.ts";

describe("fixtures match the teamwire schemas", () => {
  test("status", () => expect(StatusAll.safeParse(status).success).toBe(true));
  test("teams", () => expect(TeamsList.safeParse(teams).success).toBe(true));
  test("log", () => expect(LogView.safeParse(log).success).toBe(true));
  test("metrics", () => expect(MetricsView.safeParse(metrics).success).toBe(true));
  test("events", () => expect(EventsView.safeParse(events).success).toBe(true));
});

describe("log and events additions", () => {
  test("log keeps re, commit and a delivery reason", () => {
    const parsed = LogView.parse(log);
    const byId = new Map(parsed.messages.map((m) => [m.display_id, m]));
    expect(byId.get("m008")!.re).toBe("m006");
    expect(byId.get("m004")!.commit).toBe("4f1c2ab");
    expect(byId.get("m005")!.deliveries[0]!.reason).toBe("not_at_input_prompt");
  });
  test("log from an older teamwire without the new keys still parses", () => {
    const old = { messages: [{ ...log.messages[0]!, re: undefined, commit: undefined, deliveries: [{ to: "ledger", state: "acked", attempts: 1 }] }] };
    expect(LogView.safeParse(old).success).toBe(true);
  });
  test("an unknown event type fails the parse", () => {
    const changed = structuredClone(events);
    changed.events[0]!.type = "member.teleported";
    expect(EventsView.safeParse(changed).success).toBe(false);
  });
});

describe("context output", () => {
  const context = { tokens: 720091, window: 1000000, pct: 72, handoff_at: 400000 };
  const withContext = (value: unknown) =>
    StatusAll.parse([{ ...status[0]!, members: [{ ...status[0]!.members[0]!, context: value }] }])[0]!
      .members[0]!.context;
  test("a member accepts a context object and keeps it", () => {
    expect(withContext(context)).toEqual(context);
  });
  test("a member accepts a null context and an older output without the key", () => {
    expect(withContext(null)).toBeNull();
    expect(StatusAll.parse(status)[0]!.members[0]!.context).toBeUndefined();
  });
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
  test("teams accepts the budget end as a string, null, or no key", () => {
    const team = teams.teams[0]!;
    const parse = (extra: object) => TeamsList.parse({ teams: [{ ...team, ...extra }] }).teams[0]!;
    expect(parse({ afk_budget_ends_at: "2026-10-20T08:00:00.000Z" }).afk_budget_ends_at).toBe(
      "2026-10-20T08:00:00.000Z",
    );
    expect(parse({ afk_budget_ends_at: null }).afk_budget_ends_at).toBeNull();
    expect(parse({}).afk_budget_ends_at).toBeUndefined();
    expect(TeamsList.safeParse({ teams: [{ ...team, afk_budget_ends_at: 5 }] }).success).toBe(false);
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
// member's check never depends on which teams happen to be running. It runs
// through the server's own runner: identity variables removed, time limits.
describe.if(process.env.TW_UI_LIVE === "1")("real teamwire output", () => {
  const run = createRunner({ bin: "teamwire" });
  test("status --json", async () => expect((await run({ cmd: "status" })).ok).toBe(true));
  test("teams --json", async () => expect((await run({ cmd: "teams" })).ok).toBe(true));
  test("metrics --json", async () => expect((await run({ cmd: "metrics" })).ok).toBe(true));
  test("log --json", async () => {
    const list = await run({ cmd: "teams" });
    const team = list.ok ? list.data.teams[0]?.name : undefined;
    if (team === undefined) return; // No team to read a log from.
    expect((await run({ cmd: "log", team })).ok).toBe(true);
  });
});
