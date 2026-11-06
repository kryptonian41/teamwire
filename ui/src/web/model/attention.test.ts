import { expect, test } from "bun:test";
import { StatusAll, TeamsList, type StatusView } from "../../shared/teamwire.ts";
import status from "../../../fixtures/status.json";
import teams from "../../../fixtures/teams.json";
import { ATTENTION_KINDS, attentionItems, rankAttention } from "./attention.ts";
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
    { kind: "question", team: "alpha", teamTitle: "Alpha", member: "api", text: "asked the lead question m007, no answer for 3 s", category: "attention", word: "Question" },
  ]);
  expect(items({ unanswered_questions: [q(["lead", "web"], 4000)] })[0]!.text).toBe("asked the lead question m007, no answer for 1 h 6 min");
  expect(items({ unanswered_questions: [q(["web"], 9000)] })).toEqual([]);
});

test("a late member is listed as past its deadline, a member inside it or in another state is not", () => {
  // `teamwire status` prints `late` for a live member past its deadline, never `working` (crates/teamwire-watch/src/status.rs).
  const w = (state: string, age_s: number, deadline_s = 3600) => ({ ...member, state, age_s, deadline_s });
  expect(items({ members: [w("late", 3900)] }).filter((i) => i.kind !== "nowatch")).toEqual([
    { kind: "deadline", team: "alpha", teamTitle: "Alpha", member: "api", text: "past its deadline: 1h5m / 1h", category: "attention", word: "Late" },
  ]);
  expect(items({ members: [w("working", 3600)] }).filter((i) => i.kind === "deadline")).toEqual([]);
  expect(items({ members: [w("quiet", 9000)] })).toEqual([]);
  expect(items({ members: [w("done(done)", 9000)] })).toEqual([]);
});

test("the fixture's own late member, web, gets a deadline row", () => {
  const rows = attentionItems(joinAll(S, T)).filter((r) => r.kind === "deadline");
  expect(rows).toEqual([{ kind: "deadline", team: "payments", teamTitle: "Refund flow rewrite", member: "web", text: "past its deadline: 1h5m / 1h", category: "attention", word: "Late" }]);
});

test("a member past its handoff point is listed, with its own text once it has finished", () => {
  const ctx = { tokens: 450_000, window: 1_000_000, pct: 45, handoff_at: 400_000 };
  expect(items({ members: [{ ...member, context: ctx }] })).toEqual([
    { kind: "handoff", team: "alpha", teamTitle: "Alpha", member: "api", text: "context 450k tokens (45%), past its handoff point at 400k", category: "attention", word: "Handoff" },
  ]);
  expect(items({ members: [{ ...member, context: { ...ctx, tokens: 399_999 } }] })).toEqual([]);
  expect(items({ members: [{ ...member, state: "done(done)", context: ctx }] })).toEqual([
    { kind: "handoff", team: "alpha", teamTitle: "Alpha", member: "api", text: "past its handoff point: hand it off before you reprompt it", category: "attention", word: "Handoff" },
  ]);
});

test("a stuck delivery and an undelivered result are listed against the recipient and the member", () => {
  expect(items({ stuck_deliveries: [{ display_id: "m005", to: "api", state: "undelivered", age_s: 520 }] })).toEqual([
    { kind: "stuck", team: "alpha", teamTitle: "Alpha", member: "api", text: "message m005 is undelivered, waiting 8 min", category: "failed", word: "Undelivered" },
  ]);
  expect(items({ undelivered_results: [{ role: "fraud", n: 2, report_state: "undelivered", path: "/x" }] })).toEqual([
    { kind: "undelivered", team: "alpha", teamTitle: "Alpha", member: "fraud", text: "result 2 is undelivered", category: "failed", word: "Undelivered" },
  ]);
});

test("running members, working or late, with no running watch give one row for the team", () => {
  const none = { state: "none" as const, pid: null, heartbeat_age_s: null };
  const stale = { state: "stale" as const, pid: 9, heartbeat_age_s: 900 };
  const running = { state: "running" as const, pid: 9, heartbeat_age_s: 1 };
  expect(items({ members: [member, { ...member, role: "web" }], watch: none })).toEqual([
    { kind: "nowatch", team: "alpha", teamTitle: "Alpha", member: null, text: "2 members are working and no watch is running", category: "unknown", word: "No watch" },
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

const mark = (patch: Partial<typeof quiet>) => items(patch).map((i) => [i.kind, i.member, i.category, i.word, i.text]);

test("a read error is one failed row per error, about the whole team", () => {
  expect(mark({ errors: ["store busy past the timeout: database is locked", "x"] })).toEqual([
    ["error", null, "failed", "Read error", "store busy past the timeout: database is locked"],
    ["error", null, "failed", "Read error", "x"],
  ]);
});

test("a failed member is a failed row whose word is its state", () => {
  expect(mark({ members: [{ ...member, state: "done(failed)" }, { ...member, role: "x", state: "failed to start" }] })).toEqual([
    ["failed", "api", "failed", "done(failed)", "finished as failed"],
    ["failed", "x", "failed", "failed to start", "never started"],
  ]);
});

test("a stuck delivery's mark follows its state: failed when undelivered, attention when not", () => {
  const stuck = (state: "undelivered" | "unconfirmed" | "pending") => mark({ stuck_deliveries: [{ display_id: "m1", to: "lead", state, age_s: 5 }] })[0]!.slice(2, 4);
  expect(stuck("undelivered")).toEqual(["failed", "Undelivered"]);
  expect(stuck("unconfirmed")).toEqual(["attention", "Unconfirmed"]);
  expect(stuck("pending")).toEqual(["attention", "Pending"]);
});

test("a dead or blocked lead of an open team is an attention row; a working or idle lead is not", () => {
  const lead = (liveness: NonNullable<StatusView["lead"]>["liveness"]) => mark({ lead: { session_id: "s", liveness } });
  expect(lead({ alive: "blocked" })).toEqual([["lead", null, "attention", "Lead blocked", "the lead is blocked"]]);
  expect(lead("dead")).toEqual([["lead", null, "attention", "Lead dead", "the lead is dead"]]);
  expect(lead({ alive: "gone" })).toEqual([["lead", null, "attention", "Lead gone", "the lead is gone"]]);
  expect(lead({ alive: "working" })).toEqual([]);
  expect(lead({ alive: "idle" })).toEqual([]);
  expect(mark({ state: "closed", lead: { session_id: "s", liveness: "dead" } })).toEqual([]);
});

test("a member needing attention gets a row with its state word, unless a late, undelivered or unknown state already covers it", () => {
  const m = (role: string, state: string) => ({ ...member, role, state });
  expect(mark({ members: [m("a", "blocked"), m("b", "died?"), m("c", "done(blocked)"), m("d", "snoozing"), m("e", "unreported"), m("f", "prompted outside teamwire"), m("g", "unconfirmed")] })).toEqual([
    ["member", "a", "attention", "blocked", "reported that it is blocked"],
    ["member", "b", "attention", "died?", "shows no sign of life"],
    ["member", "c", "attention", "done(blocked)", "finished blocked"],
    ["member", "e", "attention", "unreported", "finished without a report"],
    ["member", "f", "attention", "prompted outside teamwire", "was prompted outside teamwire"],
    ["member", "g", "attention", "unconfirmed", "is in state unconfirmed"],
  ]);
  // A late member has its deadline row; a member with an undelivered result has that row.
  expect(mark({ members: [m("a", "late"), m("f", "undelivered")], undelivered_results: [{ role: "f", n: 1, report_state: "undelivered", path: "/x" }] }).map((r) => r[0])).toEqual(["undelivered", "deadline"]);
  expect(mark({ members: [m("f", "undelivered")] })).toEqual([["member", "f", "attention", "undelivered", "is in state undelivered"]]);
});

test("a spent AFK budget of an open team is an attention row", () => {
  const afk = { budget_ends_at: "2026-10-21T18:18:00Z", seconds_left: 0, spent: true, decisions: 3 };
  expect(mark({ afk })).toEqual([["afk", null, "attention", "AFK spent", "AFK budget spent, 3 decisions recorded"]]);
  expect(mark({ afk: { ...afk, spent: false, seconds_left: 10 } })).toEqual([]);
  expect(mark({ state: "closed", afk })).toEqual([]);
});

test("a stale watch on an open team gets a row whether or not anyone works, and says so in its word", () => {
  const stale = { state: "stale" as const, pid: 9, heartbeat_age_s: 900 };
  expect(mark({ members: [member], watch: stale })).toEqual([["nowatch", null, "unknown", "Watch stale", "1 member is working and the watch is stale"]]);
  expect(mark({ members: [{ ...member, state: "quiet" }], watch: stale })).toEqual([["nowatch", null, "unknown", "Watch stale", "the watch is stale, last heartbeat 15 min ago"]]);
  expect(mark({ members: [], watch: { ...stale, heartbeat_age_s: null } })).toEqual([["nowatch", null, "unknown", "Watch stale", "the watch is stale, no heartbeat recorded"]]);
  expect(mark({ state: "closed", members: [], watch: stale })).toEqual([]);
  // The docs fixture: a stale watch, nobody working.
  const docs = attentionItems(joinAll(S, T)).filter((r) => r.team === "docs" && r.kind === "nowatch");
  expect(docs.map((r) => [r.word, r.text])).toEqual([["Watch stale", "the watch is stale, last heartbeat 15 min ago"]]);
});

test("rows pushed out of table order are sorted back, inside a team and in the ranking", () => {
  // A member past its handoff point comes before a blocked member in the roster, but handoff ranks after member.
  const ctx = { tokens: 5, window: 9, pct: 1, handoff_at: 5 };
  const rows = items({ members: [{ ...member, context: ctx }, { ...member, role: "z", state: "blocked" }], errors: ["e"] });
  expect(rows.map((r) => `${r.kind}:${r.member}`)).toEqual(["error:null", "member:z", "handoff:api"]);
  // rankAttention on its own puts a team's rows back in the table's order too.
  const given = attentionItems([joinTeam({ ...quiet, members: [{ ...member, context: ctx }, { ...member, role: "z", state: "blocked" }] }, T)]);
  const shuffled = [given[1]!, given[0]!];
  expect(shuffled.map((r) => r.kind)).toEqual(["handoff", "member"]);
  expect(rankAttention(shuffled).map((r) => r.kind)).toEqual(["member", "handoff"]);
});

test("rows of one team run in the table's order", () => {
  const rows = items({
    errors: ["e"],
    members: [{ ...member, role: "z", state: "blocked" }, { ...member, state: "done(failed)" }, { ...member, role: "w", state: "late", age_s: 9000 }],
    undelivered_results: [{ role: "u", n: 1, report_state: "undelivered", path: "/x" }],
    stuck_deliveries: [{ display_id: "m1", to: "lead", state: "unconfirmed", age_s: 5 }],
    unanswered_questions: [{ display_id: "m1", from: "z", to: ["lead"], age_s: 5 }],
    lead: { session_id: "s", liveness: "dead" },
    afk: { budget_ends_at: "2026-10-21T18:18:00Z", seconds_left: 0, spent: true, decisions: 1 },
    watch: { state: "none", pid: null, heartbeat_age_s: null },
  });
  expect(rows.map((r) => r.kind)).toEqual(["error", "failed", "undelivered", "stuck", "lead", "question", "member", "deadline", "afk", "nowatch"]);
  expect(ATTENTION_KINDS).toEqual(["error", "failed", "undelivered", "stuck", "lead", "question", "member", "deadline", "handoff", "afk", "nowatch"]);
});

test("ranking orders teams by their worst row, keeps the input order on a tie, and keeps each team's rows together", () => {
  const t = (key: string, patch: Partial<typeof quiet>) => joinTeam({ ...quiet, team: key, title: key, ...patch }, T);
  const none = { state: "none" as const, pid: null, heartbeat_age_s: null };
  const ctx = { tokens: 5, window: 9, pct: 1, handoff_at: 5 };
  const teams = [
    t("b", { members: [member], watch: none }), // nowatch only
    t("c", { members: [{ ...member, context: ctx }], watch: none }), // handoff, nowatch
    t("a", { errors: ["e"], members: [{ ...member, context: ctx }], watch: none }), // error, handoff, nowatch
    t("d", { members: [{ ...member, role: "p", state: "blocked" }, { ...member, role: "q", state: "blocked" }], watch: none }), // member, member (blocked members are not working, so no watch row)
    t("e", { members: [{ ...member, context: ctx }], watch: none }), // handoff, nowatch: ties with c, stays after it
  ];
  const given = attentionItems(teams);
  expect(given.map((r) => r.team)).toEqual(["b", "c", "c", "a", "a", "a", "d", "d", "e", "e"]);
  const ranked = rankAttention(given);
  expect(ranked.map((r) => `${r.team}:${r.kind}${r.member ? ":" + r.member : ""}`)).toEqual([
    "a:error", "a:handoff:api", "a:nowatch",
    "d:member:p", "d:member:q",
    "c:handoff:api", "c:nowatch",
    "e:handoff:api", "e:nowatch",
    "b:nowatch",
  ]);
  expect(rankAttention([])).toEqual([]);
});
