// One list of what needs a person's look across all teams, built from the
// `status --json` data the page already holds. Pure, so each signal is a test.

import { countText } from "./memberEvents.ts";
import { spanText } from "./status.ts";
import { formatAge, pastHandoff, type TeamModel } from "./teams.ts";

/** In the order a team's rows are listed: what has already failed first. */
export const ATTENTION_KINDS = ["undelivered", "stuck", "question", "deadline", "handoff", "nowatch"] as const;
export type AttentionKind = (typeof ATTENTION_KINDS)[number];

export interface AttentionItem {
  kind: AttentionKind;
  /** The team's name, or null for a team that has none (no page to link to). */
  team: string | null;
  teamTitle: string;
  /** Null when the row is about the whole team. */
  member: string | null;
  text: string;
}

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

/**
 * Rows for every team, in the order the teams are given. A question to the lead counts at any age,
 * unlike the overview card's flag, which waits 10 minutes. A finished member past its handoff point
 * is listed too, because the card flags its team for it.
 */
export function attentionItems(teams: readonly TeamModel[]): AttentionItem[] {
  return teams.flatMap((t) => {
    const s = t.status;
    const row = (kind: AttentionKind, member: string | null, text: string): AttentionItem => ({ kind, team: t.name, teamTitle: t.title, member, text });
    const rows: AttentionItem[] = [];
    for (const r of s.undelivered_results) rows.push(row("undelivered", r.role, `result ${r.n} is ${r.report_state}`));
    for (const d of s.stuck_deliveries) rows.push(row("stuck", d.to, `message ${d.display_id} is ${d.state.replace("_", " ")}, waiting ${spanText(d.age_s)}`));
    for (const q of s.unanswered_questions) {
      if (q.to.includes("lead")) rows.push(row("question", q.from, `asked the lead question ${q.display_id}, no answer for ${spanText(q.age_s)}`));
    }
    for (const m of t.members) {
      const age = formatAge(m.age_s, m.deadline_s);
      // teamwire prints `late` for a live member past its deadline, before it checks for `working`.
      if (m.state === "late") rows.push(row("deadline", m.role, `past its deadline: ${age.label}`));
      if (m.context && pastHandoff(m.context)) {
        const text =
          m.category === "done"
            ? "past its handoff point: hand it off before you reprompt it"
            : `context ${countText(m.context.tokens)} tokens (${m.context.pct}%), past its handoff point at ${countText(m.context.handoff_at)}`;
        rows.push(row("handoff", m.role, text));
      }
    }
    const working = t.members.filter((m) => m.state === "working" || m.state === "late").length;
    if (s.state === "open" && working > 0 && s.watch.state !== "running") {
      rows.push(row("nowatch", null, `${plural(working, "member")} ${working === 1 ? "is" : "are"} working and ${s.watch.state === "stale" ? "the watch is stale" : "no watch is running"}`));
    }
    return rows.sort((a, b) => ATTENTION_KINDS.indexOf(a.kind) - ATTENTION_KINDS.indexOf(b.kind));
  });
}
