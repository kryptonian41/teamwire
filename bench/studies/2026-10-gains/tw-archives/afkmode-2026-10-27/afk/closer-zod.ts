import { StatusView, StatusAll, TeamsList, MetricsView } from "/private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/cache/closer/afkmode/clone/ui/src/shared/teamwire.ts";
import { readFileSync } from "node:fs";
const d = "/private/tmp/claude-501/bnruns/bn4tu2e1us/twhome/teams/afkmode/afk/closer-zero";
const r = (f: string) => JSON.parse(readFileSync(d + "/" + f, "utf8"));
const checks: [string, any, any][] = [
  ["status team", StatusView, r("02_status_team_json.out")],
  ["status all", StatusAll, r("04_status_all_json.out")],
  ["teams", TeamsList, r("06_teams_json.out")],
  ["metrics", MetricsView, r("09_metrics_all_json.out")],
];
let bad = 0;
for (const [name, schema, v] of checks) {
  const p = schema.safeParse(v);
  console.log(name, p.success ? "ok" : JSON.stringify(p.error.issues));
  if (!p.success) bad++;
  if (p.success && name === "status team") console.log("  afk kept:", JSON.stringify(p.data.afk));
  if (p.success && name === "teams") console.log("  key kept:", p.data.teams[0].afk_budget_ends_at);
}
process.exit(bad);
