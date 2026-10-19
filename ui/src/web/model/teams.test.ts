import { expect, test } from "bun:test";
import { StatusAll, TeamsList, MetricsView, type Liveness, type StatusView } from "../../shared/teamwire.ts";
import status from "../../../fixtures/status.json";
import teams from "../../../fixtures/teams.json";
import metrics from "../../../fixtures/metrics.json";
import { countByCategory, formatAge, hasProblem, joinAll, joinTeam, leadCategory, livenessInfo, problemReasons, sortTeams, summarizeMetrics } from "./teams.ts";

const S = StatusAll.parse(status);
const T = TeamsList.parse(teams).teams;
const M = MetricsView.parse(metrics).metrics;

test("joins kind from the roster by team and role", () => {
  const payments = joinTeam(S[0]!, T);
  expect(payments.members.find((m) => m.role === "api")?.kind).toBe("implementer");
  expect(payments.members.find((m) => m.role === "audit")?.kind).toBe("investigator");
  expect(payments.members.find((m) => m.role === "review")?.category).toBe("attention");
});

test("removed roster members are listed apart and absent from status", () => {
  const payments = joinTeam(S[0]!, T);
  expect(payments.removed.map((m) => m.role)).toEqual(["oldapi"]);
  expect(payments.members.some((m) => m.role === "oldapi")).toBe(false);
});

test("a member missing from the roster has a null kind", () => {
  const t = joinTeam(S[0]!, []);
  expect(t.members.every((m) => m.kind === null)).toBe(true);
});

test("counts members per category, keeping unknown words apart", () => {
  const docs = joinTeam(S[1]!, T);
  const c = countByCategory(docs.members);
  expect(c.unknown).toBe(1);
  expect(c.failed).toBe(2);
  const payments = countByCategory(joinTeam(S[0]!, T).members);
  expect(payments).toEqual({ failed: 0, attention: 4, unknown: 0, active: 1, idle: 1, done: 1 });
});

test("sorts problem teams first, then by title", () => {
  const sorted = sortTeams(joinAll(S, T));
  expect(sorted.map((t) => t.key)).toEqual(["lost", "payments", "docs"]);
  const quiet = joinTeam({ ...S[0]!, members: [], stuck_deliveries: [], undelivered_results: [], unread_deliveries: [], unanswered_questions: [], team: "aaa", title: "Zed" }, T);
  const other = joinTeam({ ...quiet.status, team: "bbb", title: "Alpha" }, T);
  const bad = joinTeam({ ...quiet.status, team: "ccc", title: "Zzz", errors: ["x"] }, T);
  expect(sortTeams([quiet, other, bad]).map((t) => t.title)).toEqual(["Zzz", "Alpha", "Zed"]);
});

test("formats age against deadline with an overdue flag", () => {
  expect(formatAge(1200, 3600)).toEqual({ label: "20m / 1h", overdue: false, ratio: 1 / 3 });
  expect(formatAge(3900, 3600).overdue).toBe(true);
  expect(formatAge(3900, 3600).ratio).toBe(1);
  expect(formatAge(45, 1800).label).toBe("45s / 30m");
  expect(formatAge(5, 0).ratio).toBe(1);
});

test("groups metrics by team label", () => {
  const sum = summarizeMetrics(M);
  const ads = sum.get("ads")!;
  expect(ads.completions).toEqual({ done: 3, failed: 1 });
  expect(ads.messagesSent).toBe(5);
  expect(ads.verifyRuns).toBe(2);
  const pay = sum.get("payments")!;
  expect(pay.leadWakeups).toBe(4);
  expect(pay.durations["teamwire.member.run.duration"]).toEqual({ mean: 1820.5, count: 1 });
});

test("maps the lead's liveness to one category", () => {
  const lead = (liveness: Liveness) => ({ session_id: null, liveness });
  expect(leadCategory(null)).toBe("unknown");
  expect(leadCategory(lead("dead"))).toBe("failed");
  expect(leadCategory(lead("unknown"))).toBe("unknown");
  expect(leadCategory(lead({ alive: "working" }))).toBe("active");
  expect(leadCategory(lead({ alive: "idle" }))).toBe("idle");
  expect(leadCategory(lead({ alive: "blocked" }))).toBe("attention");
  expect(leadCategory(lead({ alive: "gone" }))).toBe("failed");
  expect(livenessInfo(lead({ alive: "working" }))).toEqual({ category: "active", label: "lead working" });
  expect(livenessInfo(null).label).toBe("no lead");
});

test("each signal alone flags a team, and a quiet team has none", () => {
  const base: StatusView = { ...S[0]!, members: [], stuck_deliveries: [], undelivered_results: [], unread_deliveries: [], unanswered_questions: [] };
  const flagged = (patch: Partial<typeof base>) => problemReasons(joinTeam({ ...base, ...patch }, T));
  expect(hasProblem(joinTeam(base, T))).toBe(false);
  expect(flagged({})).toEqual([]);
  expect(flagged({ lead: { session_id: null, liveness: "dead" } })).toEqual(["lead dead"]);
  expect(flagged({ watch: { state: "stale", pid: 1, heartbeat_age_s: 900 } })).toEqual(["watch stale"]);
  expect(flagged({ unread_deliveries: [{ display_id: "m1", to: "api", age_s: 1 }] })).toEqual(["1 unread"]);
  expect(flagged({ unanswered_questions: [{ display_id: "m1", from: "api", to: ["lead"], age_s: 600 }] })).toEqual(["1 question waiting"]);
  expect(flagged({ unanswered_questions: [{ display_id: "m1", from: "api", to: ["lead"], age_s: 599 }] })).toEqual([]);
  expect(flagged({ unanswered_questions: [{ display_id: "m1", from: "api", to: ["web"], age_s: 9000 }] })).toEqual([]);
  expect(flagged({ afk: { budget_ends_at: "x", seconds_left: 0, spent: true, decisions: 0 } })).toEqual(["AFK spent"]);
  // A closed team keeps no lead, watch or budget to watch.
  expect(flagged({ state: "closed", lead: null, watch: { state: "none", pid: null, heartbeat_age_s: null } })).toEqual([]);
  expect(flagged({ lead: { session_id: null, liveness: "unknown" }, watch: { state: "none", pid: null, heartbeat_age_s: null } })).toEqual([]);
});

test("label sets of one duration combine into a count-weighted average", () => {
  const row = (kind: string, name: string, unit: string, value: number) => ({
    name,
    unit,
    labels: [["team", "ads"], ["kind", kind]] as [string, string][],
    value,
  });
  const d = "teamwire.member.run.duration";
  const sum = summarizeMetrics([
    row("implementer", d, "s", 100),
    row("implementer", `${d}.count`, "{observation}", 1),
    row("reviewer", d, "s", 400),
    row("reviewer", `${d}.count`, "{observation}", 3),
  ]);
  expect(sum.get("ads")!.durations[d]).toEqual({ mean: 325, count: 4 });
});
