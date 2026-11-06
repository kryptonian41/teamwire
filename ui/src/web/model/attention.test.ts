import { expect, test } from "bun:test";
import { StatusAll, TeamsList, type StatusView } from "../../shared/teamwire.ts";
import status from "../../../fixtures/status.json";
import teams from "../../../fixtures/teams.json";
import { attentionItems } from "./attention.ts";
import { joinAll, joinTeam } from "./teams.ts";

const S = StatusAll.parse(status);
const T = TeamsList.parse(teams).teams;
const member = S[0]!.members[0]!; // api, working, 1240 s of 3600 s
const quiet: StatusView = { ...S[0]!, team: "alpha", title: "Alpha", members: [], stuck_deliveries: [], undelivered_results: [], unread_deliveries: [], unanswered_questions: [], errors: [] };
const items = (patch: Partial<typeof quiet>) => attentionItems([joinTeam({ ...quiet, ...patch }, T)]);

test("a quiet team gives no rows", () => {
  expect(items({})).toEqual([]);
});

test("a question to the lead is listed at any age, one to another party is not", () => {
  const q = (to: string[], age_s: number) => ({ display_id: "m007", from: "api", to, age_s });
  expect(items({ unanswered_questions: [q(["lead"], 3)] })).toEqual([
    { kind: "question", team: "alpha", teamTitle: "Alpha", member: "api", text: "asked the lead question m007, no answer for 3 s" },
  ]);
  expect(items({ unanswered_questions: [q(["lead", "web"], 4000)] })[0]!.text).toBe("asked the lead question m007, no answer for 1 h 6 min");
  expect(items({ unanswered_questions: [q(["web"], 9000)] })).toEqual([]);
});

test("a late member is listed as past its deadline, a member inside it or in another state is not", () => {
  // `teamwire status` prints `late` for a live member past its deadline, never `working` (crates/teamwire-watch/src/status.rs).
  const w = (state: string, age_s: number, deadline_s = 3600) => ({ ...member, state, age_s, deadline_s });
  expect(items({ members: [w("late", 3900)] }).filter((i) => i.kind !== "nowatch")).toEqual([
    { kind: "deadline", team: "alpha", teamTitle: "Alpha", member: "api", text: "past its deadline: 1h5m / 1h" },
  ]);
  expect(items({ members: [w("working", 3600)] }).filter((i) => i.kind === "deadline")).toEqual([]);
  expect(items({ members: [w("quiet", 9000)] })).toEqual([]);
  expect(items({ members: [w("done(done)", 9000)] })).toEqual([]);
});

test("the fixture's own late member, web, gets a deadline row", () => {
  const rows = attentionItems(joinAll(S, T)).filter((r) => r.kind === "deadline");
  expect(rows).toEqual([{ kind: "deadline", team: "payments", teamTitle: "Refund flow rewrite", member: "web", text: "past its deadline: 1h5m / 1h" }]);
});

test("a member past its handoff point is listed, with its own text once it has finished", () => {
  const ctx = { tokens: 450_000, window: 1_000_000, pct: 45, handoff_at: 400_000 };
  expect(items({ members: [{ ...member, context: ctx }] })).toEqual([
    { kind: "handoff", team: "alpha", teamTitle: "Alpha", member: "api", text: "context 450k tokens (45%), past its handoff point at 400k" },
  ]);
  expect(items({ members: [{ ...member, context: { ...ctx, tokens: 399_999 } }] })).toEqual([]);
  expect(items({ members: [{ ...member, state: "done(done)", context: ctx }] })).toEqual([
    { kind: "handoff", team: "alpha", teamTitle: "Alpha", member: "api", text: "past its handoff point: hand it off before you reprompt it" },
  ]);
});

test("a stuck delivery and an undelivered result are listed against the recipient and the member", () => {
  expect(items({ stuck_deliveries: [{ display_id: "m005", to: "api", state: "undelivered", age_s: 520 }] })).toEqual([
    { kind: "stuck", team: "alpha", teamTitle: "Alpha", member: "api", text: "message m005 is undelivered, waiting 8 min" },
  ]);
  expect(items({ undelivered_results: [{ role: "fraud", n: 2, report_state: "undelivered", path: "/x" }] })).toEqual([
    { kind: "undelivered", team: "alpha", teamTitle: "Alpha", member: "fraud", text: "result 2 is undelivered" },
  ]);
});

test("running members, working or late, with no running watch give one row for the team", () => {
  const none = { state: "none" as const, pid: null, heartbeat_age_s: null };
  const stale = { state: "stale" as const, pid: 9, heartbeat_age_s: 900 };
  const running = { state: "running" as const, pid: 9, heartbeat_age_s: 1 };
  expect(items({ members: [member, { ...member, role: "web" }], watch: none })).toEqual([
    { kind: "nowatch", team: "alpha", teamTitle: "Alpha", member: null, text: "2 members are working and no watch is running" },
  ]);
  expect(items({ members: [{ ...member, state: "late", age_s: 9000 }], watch: none }).find((i) => i.kind === "nowatch")?.text).toBe("1 member is working and no watch is running");
  expect(items({ members: [member], watch: stale })[0]!.text).toBe("1 member is working and the watch is stale");
  expect(items({ members: [member], watch: running })).toEqual([]);
  expect(items({ members: [{ ...member, state: "quiet" }], watch: none })).toEqual([]);
  expect(items({ state: "closed", members: [member], watch: none })).toEqual([]);
});

test("rows keep the team order, and kinds run from the worst down inside a team", () => {
  const both = (key: string) => joinTeam({ ...quiet, team: key, title: key, members: [{ ...member, context: { tokens: 5, window: 9, pct: 1, handoff_at: 5 } }], stuck_deliveries: [{ display_id: "m1", to: "api", state: "pending", age_s: 1 }] }, T);
  const rows = attentionItems([both("b"), both("a")]);
  expect(rows.map((r) => `${r.team}:${r.kind}`)).toEqual(["b:stuck", "b:handoff", "a:stuck", "a:handoff"]);
});

test("the fixtures give rows for the stuck delivery and the undelivered result", () => {
  const rows = attentionItems(joinAll(S, T));
  expect(rows.map((r) => r.kind)).toContain("stuck");
  // The finished ledger member is past its handoff point, and its team is flagged for it, so it is listed.
  expect(rows.find((r) => r.member === "ledger")?.kind).toBe("handoff");
});
