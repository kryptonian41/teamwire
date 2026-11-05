// Token totals from `teamwire metrics <team>` (`teamwire.member.tokens`). Pure.

import type { Metric } from "../../shared/teamwire.ts";
import { usageTotal, type Usage } from "./memberEvents.ts";

export interface TokenRow {
  key: string;
  usage: Usage;
}

export interface TokenTable {
  byRole: TokenRow[];
  byModel: TokenRow[];
  total: Usage;
}

const TYPE_FIELD: Record<string, keyof Usage> = { input: "input", output: "output", cache_read: "cacheRead", cache_creation: "cacheCreation" };
const zero = (): Usage => ({ input: 0, output: 0, cacheRead: 0, cacheCreation: 0 });

/**
 * Sums the counter by role and by model, split by its `type` label. A row of
 * another team, or with a `type` outside the four known ones, is left out, so
 * a new type cannot be counted under the wrong column. Biggest total first.
 */
export function tokenTable(metrics: readonly Metric[], team: string): TokenTable {
  const roles = new Map<string, Usage>();
  const models = new Map<string, Usage>();
  const total = zero();
  for (const m of metrics) {
    if (m.name !== "teamwire.member.tokens") continue;
    const label = (k: string) => m.labels.find(([lk]) => lk === k)?.[1];
    const field = TYPE_FIELD[label("type") ?? ""];
    if (field === undefined || (label("team") ?? team) !== team) continue;
    const add = (map: Map<string, Usage>, key: string) => {
      const u = map.get(key) ?? zero();
      u[field] += m.value;
      map.set(key, u);
    };
    add(roles, label("role") ?? "(no role)");
    add(models, label("model") ?? "(no model)");
    total[field] += m.value;
  }
  const rows = (map: Map<string, Usage>): TokenRow[] =>
    [...map].map(([key, usage]) => ({ key, usage })).sort((a, b) => usageTotal(b.usage) - usageTotal(a.usage) || a.key.localeCompare(b.key));
  return { byRole: rows(roles), byModel: rows(models), total };
}
