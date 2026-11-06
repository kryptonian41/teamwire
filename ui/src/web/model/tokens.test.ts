import { expect, test } from "bun:test";
import metrics from "../../../fixtures/metrics.json";
import { MetricsView, type Metric } from "../../shared/teamwire.ts";
import { tokenTable } from "./tokens.ts";

const M = MetricsView.parse(metrics).metrics;
const tok = (labels: Record<string, string>, value: number): Metric => ({
  name: "teamwire.member.tokens",
  unit: "{token}",
  labels: Object.entries(labels),
  value,
});

test("totals per role and per model, each of the four types in its own column", () => {
  const t = tokenTable(M, "payments");
  expect(t.byRole.map((r) => [r.key, r.usage.input, r.usage.output, r.usage.cacheRead, r.usage.cacheCreation])).toEqual([
    ["api", 1200, 56000, 610000, 48000],
    ["ledger", 900, 31000, 420000, 30000],
    ["review", 300, 9000, 150000, 12000],
  ]);
  expect(t.byModel.map((r) => [r.key, r.usage.output])).toEqual([
    ["sonnet", 83000],
    ["opus", 9000],
    ["haiku", 4000],
  ]);
  // The grand total is the sum of every type of every row.
  expect(t.total).toEqual({ input: 2400, output: 96000, cacheRead: 1180000, cacheCreation: 90000 });
});

test("rows of other teams, other metrics, and unknown types are left out; label sets of one role add up", () => {
  const t = tokenTable(
    [
      tok({ team: "a", role: "x", model: "m1", type: "input", effort: "low" }, 5),
      tok({ team: "a", role: "x", model: "m1", type: "input", effort: "high" }, 7),
      tok({ team: "b", role: "x", model: "m1", type: "input" }, 100),
      tok({ team: "a", role: "x", model: "m1", type: "reasoning" }, 100),
      { name: "teamwire.completions", unit: "{completion}", labels: [["team", "a"]], value: 9 },
    ],
    "a",
  );
  expect(t.byRole).toEqual([{ key: "x", usage: { input: 12, output: 0, cacheRead: 0, cacheCreation: 0 } }]);
});

test("a team with no token rows gives empty tables", () => {
  const t = tokenTable(M, "docs");
  expect(t.byRole).toEqual([]);
  expect(t.byModel).toEqual([]);
});

test("a row without a role or model label is kept under a plain name", () => {
  const t = tokenTable([tok({ team: "a", type: "output" }, 3)], "a");
  expect(t.byRole[0]!.key).toBe("(no role)");
  expect(t.byModel[0]!.key).toBe("(no model)");
});
