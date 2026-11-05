// Shapes the output of `teamwire decisions` for the Decisions section. Pure.

import type { DecisionQuery } from "../../shared/api.ts";
import type { DecisionTeam } from "../../shared/teamwire.ts";

export interface DecisionColumn {
  role: string;
  title: string;
  kind: string;
  /** Why the column is empty; null when it holds blocks. */
  note: string | null;
  /** The text of each "Decisions I made" block, in file order. */
  blocks: string[];
}

/** One column per member, in the CLI's order, so two members' answers to one question sit side by side. */
export function decisionColumns(team: DecisionTeam): DecisionColumn[] {
  return team.members.map((m) => ({
    role: m.role,
    title: m.title,
    kind: m.kind,
    note: m.report === null ? "no report" : m.blocks.length === 0 ? "no Decisions I made block" : null,
    blocks: m.blocks.map((b) => b.text),
  }));
}

/** The longest search text the server accepts. */
export const GREP_MAX = 200;

/** A search query from what the user typed, or null when the server would refuse it. */
export function queryFromText(input: string): DecisionQuery | null {
  const text = input.trim();
  if (text === "" || text.length > GREP_MAX || text.startsWith("-")) return null;
  return { grep: text };
}

export const queryLabel = (q: DecisionQuery): string => ("grep" in q ? `text “${q.grep}”` : `ref ${q.ref}`);
