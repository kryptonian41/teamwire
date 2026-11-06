// One list of what needs a person's look across all teams, built from the
// `status --json` data the page already holds. Pure, so each signal is a test.

import type { StateCategory } from "../../shared/state.ts";
import { countText } from "./memberEvents.ts";
import { spanText } from "./status.ts";
import { formatAge, leadCategory, livenessInfo, pastHandoff, type TeamModel } from "./teams.ts";

/** Worst first. This order ranks a team's rows, and a team by its best-ranked row. */
export const ATTENTION_KINDS = ["error", "failed", "undelivered", "stuck", "lead", "question", "member", "deadline", "handoff", "afk", "nowatch"] as const;
export type AttentionKind = (typeof ATTENTION_KINDS)[number];

export interface AttentionItem {
  kind: AttentionKind;
  /** The team's name, or null for a team that has none (no page to link to). */
  team: string | null;
  teamTitle: string;
  /** Null when the row is about the whole team. */
  member: string | null;
  text: string;
  /** The mark's category gives its shape and colour. */
  category: StateCategory;
  /** The mark's label. */
  word: string;
}

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;
const capital = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

/**
 * Rows for every team, in the order the teams are given. A question to the lead counts at any age,
 * unlike the overview card's flag, which waits 10 minutes. A finished member past its handoff point
 * is listed too, because the card flags its team for it. A member in the `unknown` category gets no row.
 */
export function attentionItems(teams: readonly TeamModel[]): AttentionItem[] {
  return teams.flatMap((t) => {
    const s = t.status;
    const row = (kind: AttentionKind, member: string | null, text: string, category: StateCategory, word: string): AttentionItem => ({ kind, team: t.name, teamTitle: t.title, member, text, category, word });
    const rows: AttentionItem[] = [];
    for (const e of s.errors) rows.push(row("error", null, e, "failed", "Read error"));
    for (const m of t.members) {
      if (m.category !== "failed") continue;
      const text = m.state === "done(failed)" ? "finished as failed" : m.state === "failed to start" ? "never started" : `is ${m.state}`;
      rows.push(row("failed", m.role, text, "failed", m.state));
    }
    const undelivered = new Set(s.undelivered_results.map((r) => r.role));
    for (const r of s.undelivered_results) rows.push(row("undelivered", r.role, `result ${r.n} is ${r.report_state}`, "failed", "Undelivered"));
    for (const d of s.stuck_deliveries) {
      const state = d.state.replace("_", " ");
      rows.push(row("stuck", d.to, `message ${d.display_id} is ${state}, waiting ${spanText(d.age_s)}`, d.state === "undelivered" ? "failed" : "attention", capital(state)));
    }
    if (s.state === "open") {
      const lead = leadCategory(s.lead);
      if (lead === "failed" || lead === "attention") {
        const word = livenessInfo(s.lead).label.replace(/^lead /, "");
        rows.push(row("lead", null, `the lead is ${word}`, "attention", `Lead ${word}`));
      }
    }
    for (const q of s.unanswered_questions) {
      if (q.to.includes("lead")) rows.push(row("question", q.from, `asked the lead question ${q.display_id}, no answer for ${spanText(q.age_s)}`, "attention", "Question"));
    }
    for (const m of t.members) {
      // teamwire prints `late` for a live member past its deadline, before it checks for `working`.
      if (m.category === "attention" && m.state !== "late" && !undelivered.has(m.role)) rows.push(row("member", m.role, `is ${m.state}`, "attention", m.state));
      const age = formatAge(m.age_s, m.deadline_s);
      if (m.state === "late") rows.push(row("deadline", m.role, `past its deadline: ${age.label}`, "attention", "Late"));
      if (m.context && pastHandoff(m.context)) {
        const text =
          m.category === "done"
            ? "past its handoff point: hand it off before you reprompt it"
            : `context ${countText(m.context.tokens)} tokens (${m.context.pct}%), past its handoff point at ${countText(m.context.handoff_at)}`;
        rows.push(row("handoff", m.role, text, "attention", "Handoff"));
      }
    }
    if (s.state === "open" && s.afk?.spent) rows.push(row("afk", null, `AFK budget spent, ${plural(s.afk.decisions, "decision")} recorded`, "attention", "AFK spent"));
    const working = t.members.filter((m) => m.state === "working" || m.state === "late").length;
    if (s.state === "open" && working > 0 && s.watch.state !== "running") {
      const stale = s.watch.state === "stale";
      rows.push(row("nowatch", null, `${plural(working, "member")} ${working === 1 ? "is" : "are"} working and ${stale ? "the watch is stale" : "no watch is running"}`, "unknown", stale ? "Watch stale" : "No watch"));
    }
    return rows.sort((a, b) => ATTENTION_KINDS.indexOf(a.kind) - ATTENTION_KINDS.indexOf(b.kind));
  });
}

/** The key that groups a team's rows: its name, or its title for a team without one. */
export const teamKey = (i: Pick<AttentionItem, "team" | "teamTitle">): string => i.team ?? `\0${i.teamTitle}`;

/**
 * The queue's order: teams by their worst (best-ranked) row, teams with the same worst rank in
 * the order first given, and inside a team the table's order with rows of one kind as given.
 */
export function rankAttention(items: readonly AttentionItem[]): AttentionItem[] {
  const rank = (i: AttentionItem) => ATTENTION_KINDS.indexOf(i.kind);
  const worst = new Map<string, number>();
  const first = new Map<string, number>();
  items.forEach((i, n) => {
    const k = teamKey(i);
    if (!first.has(k)) first.set(k, n);
    worst.set(k, Math.min(worst.get(k) ?? Infinity, rank(i)));
  });
  return [...items].sort((a, b) => worst.get(teamKey(a))! - worst.get(teamKey(b))! || first.get(teamKey(a))! - first.get(teamKey(b))! || rank(a) - rank(b));
}
