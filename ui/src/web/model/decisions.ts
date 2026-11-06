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
  /** All blocks joined: one list of items, each a list of inline parts. */
  items: Part[][];
  count: number;
  /** The empty state in words for the member list; null when the member has items. */
  empty: string | null;
}

/** One column per member, in the CLI's order, so two members' answers to one question sit side by side. */
export function decisionColumns(team: DecisionTeam): DecisionColumn[] {
  return team.members.map((m) => {
    const items = m.blocks.flatMap((b) => blockItems(b.text));
    return {
      role: m.role,
      title: m.title,
      kind: m.kind,
      note: m.report === null ? "no report" : m.blocks.length === 0 ? "no Decisions I made block" : null,
      blocks: m.blocks.map((b) => b.text),
      items,
      count: items.length,
      empty: m.report === null ? "no report yet" : m.blocks.length === 0 ? "no Decisions block" : items.length === 0 ? "empty Decisions block" : null,
    };
  });
}

export interface Part {
  kind: "text" | "bold" | "code";
  text: string;
}

const ITEM_START = /^\s*(?:[-*]|\d+\.)\s+(.*)$/;
// Leftmost match wins, so a `**` inside a code span stays code text. An unclosed marker matches nothing.
const INLINE = /\*\*(.+?)\*\*|`([^`]+)`/g;

function inlineParts(raw: string): Part[] {
  const parts: Part[] = [];
  let at = 0;
  for (const m of raw.matchAll(INLINE)) {
    if (m.index > at) parts.push({ kind: "text", text: raw.slice(at, m.index) });
    parts.push(m[1] !== undefined ? { kind: "bold", text: m[1] } : { kind: "code", text: m[2] ?? "" });
    at = m.index + m[0].length;
  }
  if (at < raw.length) parts.push({ kind: "text", text: raw.slice(at) });
  return parts;
}

/** The items of one block's text. Only plain strings come out, so the page never parses report text as HTML. */
export function blockItems(text: string): Part[][] {
  const raw: string[] = [];
  for (const line of text.split("\n")) {
    const t = line.trim();
    if (t === "") continue;
    const start = ITEM_START.exec(line);
    if (start) raw.push(start[1]!.trim());
    else if (raw.length === 0) raw.push(t);
    else raw[raw.length - 1] += ` ${t}`;
  }
  return raw.filter((r) => r !== "").map(inlineParts);
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
