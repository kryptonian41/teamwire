import { expect, test } from "bun:test";
import { StatusAll, TeamsList, MetricsView, type Liveness, type StatusView } from "../../shared/teamwire.ts";
import status from "../../../fixtures/status.json";
import teams from "../../../fixtures/teams.json";
import metrics from "../../../fixtures/metrics.json";
import { contextLabel, countByCategory, formatAge, formatDuration, hasProblem, joinAll, joinTeam, leadCategory, livenessInfo, pastHandoff, problemReasons, refreshKey, sortTeams, summarizeMetrics } from "./teams.ts";

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
  expect(sorted.map((t) => t.key)).toEqual(["lost", "payments", "docs", "nightly"]);
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
  const flagged = (patch: Partial<typeof base>) => {
    const reasons = problemReasons(joinTeam({ ...base, ...patch }, T));
    // The overview splits teams with hasProblem, so it must agree with the flags.
    expect(hasProblem(joinTeam({ ...base, ...patch }, T))).toBe(reasons.length > 0);
    return reasons;
  };
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

test("the refresh key ignores a quiet poll and changes with the team", () => {
  const s = S[0]!;
  expect(refreshKey({ ...s })).toBe(refreshKey(s));
  expect(refreshKey({ ...s, members: s.members.map((m, i) => (i === 0 ? { ...m, age_s: m.age_s + 5 } : m)) })).toBe(refreshKey(s));
  expect(refreshKey({ ...s, members: s.members.map((m, i) => (i === 0 ? { ...m, state: "idle" } : m)) })).not.toBe(refreshKey(s));
  expect(refreshKey({ ...s, members: s.members.map((m, i) => (i === 0 ? { ...m, generation: m.generation + 1 } : m)) })).not.toBe(refreshKey(s));
  expect(refreshKey({ ...s, messages: s.messages.slice(1) })).not.toBe(refreshKey(s));
  expect(refreshKey({ ...s, unread_deliveries: [] })).not.toBe(refreshKey(s));
  expect(refreshKey({ ...s, stuck_deliveries: [] })).not.toBe(refreshKey(s));
});

test("the refresh key changes when a message arrives, whatever order the list is in", () => {
  const s = S[0]!;
  const [a, b, c] = s.messages;
  const oldestFirst = (n: number) => ({ ...s, messages: [a!, b!, c!, s.messages[3]!].slice(0, n) });
  expect(refreshKey(oldestFirst(4))).not.toBe(refreshKey(oldestFirst(3)));
  expect(refreshKey({ ...s, messages: [...s.messages].reverse() })).not.toBe(refreshKey(s));
});

test("durations round to whole seconds", () => {
  expect(formatDuration(10.666666666666666)).toBe("11s");
  expect(formatDuration(59.6)).toBe("1m");
});

test("the context label is the percentage, and nothing when unknown", () => {
  expect(contextLabel({ tokens: 720091, window: 1000000, pct: 72, handoff_at: 400000 })).toBe("72%");
  expect(contextLabel(null)).toBeNull();
  expect(contextLabel(undefined)).toBeNull();
});

test("a member is past its handoff at or beyond the handoff token count", () => {
  const ctx = (tokens: number, handoff_at = 400_000) => ({ tokens, window: 1_000_000, pct: Math.floor(tokens / 10_000), handoff_at });
  expect(pastHandoff(ctx(399_999))).toBe(false);
  expect(pastHandoff(ctx(400_000))).toBe(true);
  expect(pastHandoff(ctx(720_091))).toBe(true);
  // No reading (null, or an older teamwire without the key) is not past anything.
  expect(pastHandoff(null)).toBe(false);
  expect(pastHandoff(undefined)).toBe(false);
});

test("the overview card counts the members past their handoff", () => {
  const member = S[0]!.members[0]!;
  const ctx = (tokens: number) => ({ tokens, window: 1_000_000, pct: 40, handoff_at: 400_000 });
  const team = (...tokens: (number | null)[]) =>
    joinTeam(
      { ...S[0]!, members: tokens.map((t, i) => ({ ...member, role: `m${i}`, context: t === null ? null : ctx(t) })), stuck_deliveries: [], undelivered_results: [], unread_deliveries: [], unanswered_questions: [] },
      T,
    );
  expect(problemReasons(team(100, null))).toEqual([]);
  expect(problemReasons(team(400_000, 500_000, 100, null))).toEqual(["2 past handoff"]);
  expect(hasProblem(team(400_000))).toBe(true);
});
