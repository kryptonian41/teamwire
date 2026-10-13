import { expect, test } from "bun:test";
import { StatusAll, TeamsList, MetricsView } from "../../shared/teamwire.ts";
import status from "../../../fixtures/status.json";
import teams from "../../../fixtures/teams.json";
import metrics from "../../../fixtures/metrics.json";
import { countByCategory, formatAge, joinAll, joinTeam, livenessInfo, sortTeams, summarizeMetrics } from "./teams.ts";

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
  const quiet = joinTeam({ ...S[0]!, members: [], stuck_deliveries: [], undelivered_results: [], team: "aaa", title: "Zed" }, T);
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

test("describes the lead's liveness", () => {
  expect(livenessInfo(null).label).toBe("no lead");
  expect(livenessInfo({ session_id: null, liveness: "dead" })).toEqual({ cls: "dead", label: "lead dead" });
  expect(livenessInfo({ session_id: null, liveness: { alive: "working" } }).cls).toBe("alive-working");
});
