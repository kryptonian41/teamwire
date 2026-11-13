import { describe, expect, test } from "bun:test";
import status from "../../fixtures/status.json";
import teams from "../../fixtures/teams.json";
import log from "../../fixtures/log.json";
import metrics from "../../fixtures/metrics.json";
import events from "../../fixtures/events.json";
import decisions from "../../fixtures/decisions.json";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createRunner, loadUserKey } from "../server/cli.ts";
import { DecisionsView, EventsView, LogView, MetricsView, StatusAll, TeamsList } from "./teamwire.ts";

describe("fixtures match the teamwire schemas", () => {
  test("status", () => expect(StatusAll.safeParse(status).success).toBe(true));
  test("teams", () => expect(TeamsList.safeParse(teams).success).toBe(true));
  test("log", () => expect(LogView.safeParse(log).success).toBe(true));
  test("metrics", () => expect(MetricsView.safeParse(metrics).success).toBe(true));
  test("events", () => expect(EventsView.safeParse(events).success).toBe(true));
  test("decisions", () => expect(DecisionsView.safeParse(decisions).success).toBe(true));
});

describe("decisions output", () => {
  test("keeps each member's blocks and the team refs, and drops the paths it does not need", () => {
    const team = DecisionsView.parse(decisions).teams[0]!;
    expect(team.team).toBe("payments");
    expect(team.refs.map((r) => `${r.provider}:${r.id}`)).toEqual(["linear:GKA-580", "github:#3772"]);
    expect(team.members.map((m) => [m.role, m.blocks.length, m.report === null])).toEqual([
      ["api", 1, false],
      ["ledger", 2, false],
      ["web", 0, true],
      ["audit", 0, false],
    ]);
    expect(team.members[0]!.blocks[0]).toEqual({ heading: "## Decisions I made", line: 41, text: expect.stringContaining("422") });
    expect("path" in team).toBe(false);
    expect("ulid" in team).toBe(false);
  });
  test("a team from an archive, with no refs key, still parses", () => {
    const old = structuredClone(decisions);
    delete (old.teams[0] as { refs?: unknown }).refs;
    old.teams[0]!.live = false;
    expect(DecisionsView.parse(old).teams[0]!.refs).toEqual([]);
  });
  test("a block with a text line number fails the parse", () => {
    const bad = structuredClone(decisions);
    (bad.teams[0]!.members[0]!.blocks[0] as { line: unknown }).line = "41";
    expect(DecisionsView.safeParse(bad).success).toBe(false);
  });
});

describe("log and events additions", () => {
  test("log keeps re, commit and a delivery reason", () => {
    const parsed = LogView.parse(log);
    const byId = new Map(parsed.messages.map((m) => [m.display_id, m]));
    expect(byId.get("m008")!.re).toBe("m006");
    expect(byId.get("m004")!.commit).toBe("4f1c2ab");
    expect(byId.get("m005")!.deliveries[0]!.reason).toBe("not_at_input_prompt");
  });
  test("a message from the user parses, with the lead's copy as one more recipient and delivery", () => {
    const m = LogView.parse(log).messages.find((x) => x.from === "user")!;
    expect(m.to).toEqual(["ledger", "lead"]);
    expect(m.deliveries.map((d) => d.to)).toEqual(["ledger", "lead"]);
  });
  test("log from an older teamwire without the new keys still parses", () => {
    const old = { messages: [{ ...log.messages[0]!, re: undefined, commit: undefined, deliveries: [{ to: "ledger", state: "acked", attempts: 1 }] }] };
    expect(LogView.safeParse(old).success).toBe(true);
  });
  test("team.refs_added and member.handoff rows parse and keep their data", () => {
    const parsed = EventsView.parse(events);
    const added = parsed.events.find((e) => e.type === "team.refs_added")!;
    expect(added.data["added"]).toEqual([
      { provider: "linear", id: "GKA-581" },
      { provider: "github", id: "#3772" },
    ]);
    const handoff = parsed.events.find((e) => e.type === "member.handoff")!;
    expect(handoff.role).toBe("api");
    expect(handoff.data["generation"]).toBe(2);
  });
  test("a member.woken event parses", () => {
    // teamwire ADR 0043: a message that wakes a finished member starts a generation.
    const changed = structuredClone(events);
    changed.events[0]!.type = "member.woken" as (typeof changed.events)[number]["type"];
    changed.events[0]!.role = "impl";
    changed.events[0]!.data = { generation: 3, message: "m012" };
    const parsed = EventsView.safeParse(changed);
    expect(parsed.success).toBe(true);
    expect(String(parsed.data?.events[0]?.type)).toBe("member.woken");
    expect(parsed.data?.events[0]?.data["generation"]).toBe(3);
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

describe("current tool output", () => {
  test("current_tool parses and is kept", () => {
    const tool = { name: "Bash", started_at: "2026-11-13T07:00:09.5Z", seconds: 902 };
    const member = { ...status[0]!.members[0]!, current_tool: tool };
    const parsed = StatusAll.parse([{ ...status[0]!, members: [member] }]);
    expect(parsed[0]!.members[0]!.current_tool).toEqual(tool);
  });
  test("a null current_tool and an older output without the key still parse", () => {
    const member = { ...status[0]!.members[0]!, current_tool: null };
    expect(StatusAll.parse([{ ...status[0]!, members: [member] }])[0]!.members[0]!.current_tool).toBeNull();
    expect(StatusAll.parse(status)[0]!.members[0]!.current_tool).toBeUndefined();
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

describe("held for handoff", () => {
  test("held_deliveries parse and are kept", () => {
    const held = [{ display_id: "m004", to: "impl" }];
    const parsed = StatusAll.parse([{ ...status[0]!, held_deliveries: held }]);
    expect(parsed[0]!.held_deliveries).toEqual(held);
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
// member's check never depends on which teams happen to be running. It runs
// through the server's own runner: identity variables removed, time limits.
describe.if(process.env.TW_UI_LIVE === "1")("real teamwire output", () => {
  const run = createRunner({ bin: "teamwire" });
  test("status --json", async () => expect((await run({ cmd: "status" })).ok).toBe(true));
  test("teams --json", async () => expect((await run({ cmd: "teams" })).ok).toBe(true));
  test("metrics --json", async () => expect((await run({ cmd: "metrics" })).ok).toBe(true));
  test("decisions --json, for a team, a search text and a ref", async () => {
    const list = await run({ cmd: "teams" });
    const team = list.ok ? list.data.teams[0]?.name : undefined;
    if (team === undefined) return; // No team to read.
    expect((await run({ cmd: "decisionsTeam", team })).ok).toBe(true);
    expect((await run({ cmd: "metricsTeam", team })).ok).toBe(true);
    expect((await run({ cmd: "decisionsGrep", text: "decisions" })).ok).toBe(true);
    expect((await run({ cmd: "decisionsRef", ref: "linear:ZZZ-0" })).ok).toBe(true);
  });
  // The write forms run only in a throwaway home with no team, so nothing is sent or closed.
  // Exit 3 would mean teamwire refused the arguments the dashboard builds.
  test("user-key, and the send and dry-run close forms parse, in an empty throwaway home", async () => {
    const home = mkdtempSync(join(tmpdir(), "twui-live-"));
    try {
      const key = await loadUserKey(createRunner({ bin: "teamwire", home }));
      expect(key.ok).toBe(true);
      const write = createRunner({ bin: "teamwire", home, userKey: key.ok ? key.key : undefined });
      const send = await write({ cmd: "send", team: "nosuch", to: "lead", type: "note", text: "- stop and commit" });
      expect(send.ok && send.data.exit).toBe(1);
      expect(send.ok && send.data.stderr).toContain("no database for team nosuch");
      for (const call of [
        { cmd: "closeTeam", team: "nosuch", dryRun: true },
        { cmd: "closeMember", team: "nosuch", role: "impl", stop: true, unverified: true, dryRun: true },
      ] as const) {
        const res = await write(call);
        expect(res.ok && res.data.exit).not.toBe(3);
        expect(res.ok && res.data.exit).not.toBe(0);
      }
    } finally {
      rmSync(home, { recursive: true, force: true });
    }
  });
  test("log --json", async () => {
    const list = await run({ cmd: "teams" });
    const team = list.ok ? list.data.teams[0]?.name : undefined;
    if (team === undefined) return; // No team to read a log from.
    expect((await run({ cmd: "log", team })).ok).toBe(true);
  });
});
