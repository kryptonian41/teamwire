// Plain-language labels, filters and merging for the event timeline. Pure, so
// each worked example in the brief is a test. `data` keys differ per event
// type and older rows carry fewer of them, so every read is checked and a row
// with a missing key shows its type and member with no details line.

import type { EventRow } from "../../shared/teamwire.ts";
import { refText, spanText } from "./status.ts";

export const FILTERS = ["problems", "members", "messages", "verify", "watch", "all"] as const;
export type Filter = (typeof FILTERS)[number];

export const FILTER_LABELS: Record<Filter, string> = {
  problems: "Problems",
  members: "Members",
  messages: "Messages",
  verify: "Verify",
  watch: "Watch",
  all: "All",
};

export interface TimelineLine {
  seq: number;
  time: string;
  /** The party the row is about, from `whoOf`. */
  role: string | null;
  /** Display id of the message the row is about, such as `m014`. */
  message: string | null;
  label: string;
  details: string | null;
  problem: boolean;
  routine: boolean;
  row: EventRow;
}

type Data = EventRow["data"];

const str = (d: Data, k: string): string | null => (typeof d[k] === "string" && d[k] !== "" ? (d[k] as string) : null);
const num = (d: Data, k: string): number | null => (typeof d[k] === "number" && Number.isFinite(d[k]) ? (d[k] as number) : null);
const strs = (d: Data, k: string): string[] => {
  const v = d[k];
  return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
};
/** The `{provider, id}` objects in `d[k]`; anything else in the list is left out. */
const refs = (d: Data, k: string): string[] => {
  const v = d[k];
  if (!Array.isArray(v)) return [];
  return v.flatMap((r: unknown) => {
    const o = typeof r === "object" && r !== null ? (r as Data) : {};
    const provider = str(o, "provider");
    const id = str(o, "id");
    return provider !== null && id !== null ? [refText({ provider, id })] : [];
  });
};
const join = (parts: (string | null)[]): string | null => {
  const kept = parts.filter((p): p is string => p !== null);
  return kept.length === 0 ? null : kept.join(" · ");
};
const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;
const secs = (d: Data): string | null => {
  const s = num(d, "seconds");
  return s === null ? null : spanText(s);
};

/** The first line of `s`, cut to `max` characters. */
export function oneLine(s: string, max = 140): string {
  const first = (s.split("\n")[0] ?? "").trim();
  return first.length > max ? `${first.slice(0, max - 1)}…` : first;
}

export const PROBLEM_TYPES: ReadonlySet<string> = new Set([
  "member.failed", "member.refused", "member.held", "member.completion_refused", "message.refused",
  "delivery.undelivered", "delivery.unconfirmed", "result.undelivered", "result.unconfirmed",
  "close.refused", "store.error",
]);

export const ROUTINE_TYPES: ReadonlySet<string> = new Set([
  "delivery.handed_off", "delivery.acked", "result.delivering", "result.delivered", "member.creating", "close.step",
]);

/** `watch.line` words that report progress, not a problem. `WATCHING` may carry a member count after it. */
const calm = (word: string): boolean => /^(WATCHING\b.*|TEAM-COMPLETE|COMPLETED)$/.test(word);

export function isProblem(row: EventRow): boolean {
  if (row.type === "watch.line") return strs(row.data, "events").some((w) => !calm(w));
  // A message held for a member that asked for a handoff waits on purpose (ADR 0043 decision 11).
  if (row.type === "delivery.undelivered" && str(row.data, "code") === "held_for_handoff") return false;
  if (PROBLEM_TYPES.has(row.type)) return true;
  if (row.type === "verify.recorded") {
    const v = str(row.data, "verdict");
    return v !== null && v !== "PASS";
  }
  return row.type === "member.removed" && row.data["unverified"] === true;
}

export const isRoutine = (row: EventRow): boolean => ROUTINE_TYPES.has(row.type);

export function matchesFilter(row: EventRow, filter: Filter): boolean {
  switch (filter) {
    case "all":
      return true;
    case "problems":
      return isProblem(row);
    case "members":
      return row.type.startsWith("member.");
    case "messages":
      return /^(message|delivery|result)\./.test(row.type);
    case "verify":
      return row.type.startsWith("verify.");
    case "watch":
      return row.type.startsWith("watch.");
  }
}

/** The label and details of one row. `resultN` is the member's latest completion number at that row. */
export function describe(row: EventRow, resultN: number | null = null): { label: string | null; details: string | null } {
  const d = row.data;
  const who = row.role;
  const m = row.message;
  const to = str(d, "to") ?? who;
  const result = resultN === null ? "result" : `result #${resultN}`;
  switch (row.type) {
    case "team.created":
      return { label: "team created", details: str(d, "title") };
    case "team.closing":
      return { label: "team closing", details: null };
    case "team.closed":
      return { label: "team closed", details: str(d, "outcome") };
    case "team.lead_adopted":
      return { label: "lead adopted by a new session", details: null };
    case "team.refs_added": {
      const added = refs(d, "added");
      return { label: added.length > 0 ? "team ticket refs added" : null, details: added.join(", ") };
    }
    case "member.creating":
      return { label: who && `${who} being created`, details: join([str(d, "kind"), str(d, "mode")]) };
    case "member.started": {
      const steps = d["steps"];
      const ms =
        typeof steps === "object" && steps !== null
          ? Object.values(steps).reduce<number>((sum, v) => sum + (typeof v === "number" ? v : 0), 0)
          : 0;
      return {
        label: who && `${who} started`,
        details: join([str(d, "model"), str(d, "effort"), ms > 0 ? `took ${(ms / 1000).toFixed(1)} s` : null]),
      };
    }
    case "member.failed":
      return { label: who && `${who} failed to start`, details: oneLine(str(d, "reason") ?? "") || null };
    case "member.refused":
      return { label: who && `${who} refused at launch`, details: oneLine(str(d, "reason") ?? "") || null };
    case "member.replaced":
      return { label: who && `${who} replaced`, details: str(d, "previous_state") === null ? null : `previous run was ${str(d, "previous_state")}` };
    case "member.removed":
      return {
        label: who && (d["unverified"] === true ? `${who} removed without a passing verify` : `${who} removed`),
        details: strs(d, "unverified_reasons")[0] ?? null,
      };
    case "member.reprompted": {
      const g = num(d, "generation");
      return { label: who && `${who} reprompted`, details: g === null ? null : `generation ${g}` };
    }
    // A message other than a reprompt woke a finished member: a new
    // generation that teamwire tracks, not a reprompt (teamwire ADR 0043).
    case "member.woken": {
      const g = num(d, "generation");
      return { label: who && `${who} woken`, details: g === null ? null : `generation ${g}` };
    }
    case "member.handoff": {
      const g = num(d, "generation");
      const was = str(d, "old_session")?.slice(0, 8) ?? null;
      const now = str(d, "new_session")?.slice(0, 8) ?? null;
      return {
        label: who && `${who} got a fresh session`,
        details: join([g === null ? null : `generation ${g}`, was === null && now === null ? null : `${was ?? "no session"} → ${now ?? "chosen by the harness"}`]),
      };
    }
    case "member.completed": {
      const n = num(d, "n");
      return { label: who && `${who} completed result${n === null ? "" : ` #${n}`}`, details: str(d, "status") };
    }
    case "member.completion_refused":
      return { label: who && `${who}'s completion refused`, details: oneLine(str(d, "reason") ?? "") || null };
    case "member.held": {
      const g = d["gaps"];
      const gaps = typeof g === "object" && g !== null ? (g as Data) : {};
      const commits = strs(gaps, "commits_without_line");
      const parts = [
        commits.length === 0 ? null : `${commits.length === 1 ? "commit" : "commits"} ${commits.join(", ")} ${commits.length === 1 ? "has" : "have"} no progress line in the report`,
        gaps["decisions_missing"] === true ? "decisions file is missing" : null,
        gaps["report_missing"] === true ? "report is missing" : null,
      ].filter((p): p is string => p !== null);
      const unread = strs(d, "unread");
      if (unread.length > 0) parts.push(`${unread.length} unread: ${unread.join(", ")}`);
      return { label: who && `${who} could not stop yet`, details: parts.length === 0 ? null : parts.join("; ") };
    }
    case "message.sent": {
      const sender = who ?? str(d, "sender_kind");
      const rec = strs(d, "recipients");
      return {
        label: sender && m && `${sender} sent ${m}`,
        details: join([str(d, "type"), rec.length === 0 ? null : `to ${rec.join(", ")}`]),
      };
    }
    case "message.refused": {
      const sender = str(d, "sender") ?? who;
      const rec = strs(d, "to");
      const type = str(d, "type");
      return {
        label: sender && type && rec.length > 0 ? `${sender}'s ${type} to ${rec.join(", ")} was refused` : null,
        details: str(d, "reason") ?? str(d, "code"),
      };
    }
    case "delivery.handed_off":
      return { label: m && to && `${m} handed to ${to}`, details: secs(d) };
    case "delivery.acked":
      return { label: m && to && `${m} acked by ${to}`, details: secs(d) === null ? null : `after ${secs(d)}` };
    case "delivery.unconfirmed":
      return { label: m && to && `${m} not confirmed by ${to}`, details: str(d, "code") };
    case "delivery.undelivered": {
      const a = num(d, "attempts");
      return {
        label: m && to && `${m} not delivered to ${to}`,
        details: [str(d, "code"), a === null ? null : `after ${a === 1 ? "1 try" : `${a} tries`}`].filter((p) => p !== null).join(" ") || null,
      };
    }
    case "delivery.reminded": {
      const s = num(d, "seconds");
      return { label: m && to && `${to} reminded to read ${m}`, details: s === null ? null : `unread for ${spanText(s)}` };
    }
    case "delivery.resent": {
      const a = num(d, "attempt");
      return { label: m && to && `${m} sent again to ${to}`, details: a === null ? null : `attempt ${a}` };
    }
    case "delivery.reentered":
      return { label: m && to && `${m} re-entered for ${to}`, details: str(d, "place") };
    case "delivery.recipient_stopped":
      return { label: m && to && `${to} stopped while ${m} was being delivered`, details: null };
    case "result.delivering":
      return { label: who && `${who}'s ${result} being delivered`, details: secs(d) };
    case "result.delivered":
      return { label: who && `${who}'s ${result} delivered`, details: secs(d) };
    case "result.undelivered":
      return { label: who && `${who}'s ${result} not delivered`, details: str(d, "code") };
    case "result.unconfirmed":
      return { label: who && `${who}'s ${result} not confirmed`, details: str(d, "code") };
    case "watch.started":
      return { label: "watch started", details: join([str(d, "host"), num(d, "pid") === null ? null : `pid ${num(d, "pid")}`]) };
    case "watch.line": {
      const ev = strs(d, "events");
      return { label: ev.length > 0 ? `watch told the lead: ${ev.join(", ")}` : null, details: null };
    }
    case "watch.stopped": {
      const p = num(d, "polls");
      return { label: "watch stopped", details: join([str(d, "reason"), p === null ? null : plural(p, "poll")]) };
    }
    case "verify.recorded": {
      const verdict = str(d, "verdict");
      const flags = strs(d, "flags");
      const exit = num(d, "exit");
      const s = num(d, "seconds");
      return {
        label: who && verdict && `${who} verify ${verdict}`,
        details: join([flags.length > 0 ? flags.join(", ") : null, exit === null ? null : `exit ${exit}`, s === null ? null : spanText(s)]),
      };
    }
    case "verify.waived": {
      const flags = strs(d, "flags");
      const sha = str(d, "head_sha");
      return {
        label: who && flags.length > 0 ? `lead waived ${plural(flags.length, "flag")} on ${who}` : null,
        details: flags.length === 0 ? null : `${flags.join(", ")}${sha === null ? "" : ` at ${sha.slice(0, 7)}`}`,
      };
    }
    case "close.step":
      return { label: "close step", details: join([str(d, "step"), str(d, "outcome")]) };
    case "close.refused": {
      const r = str(d, "reason");
      return { label: "close refused", details: r === null ? null : oneLine(r) };
    }
    case "store.error":
      return { label: "store error", details: join([str(d, "op"), str(d, "code")]) };
  }
}

/** The label when a key it needs is missing: the type, then the member. */
const fallbackLabel = (row: EventRow): string => (row.role ? `${row.type} · ${row.role}` : row.type);

/**
 * One line per row, same order as `rows`. A result row names the member's
 * latest completion number, which only an earlier `member.completed` row
 * carries.
 */
/** The party a row is about. The lead's own `message.sent` rows carry no role, so they name `lead` from `sender_kind`. */
export const whoOf = (row: EventRow): string | null =>
  row.role ?? (row.type === "message.sent" && row.data["sender_kind"] === "lead" ? "lead" : null);

export function describeRows(rows: readonly EventRow[]): TimelineLine[] {
  const latest = new Map<string, number>();
  return rows.map((row) => {
    if (row.type === "member.completed" && row.role !== null) {
      const n = row.data["n"];
      if (typeof n === "number") latest.set(row.role, n);
    }
    const { label, details } = describe(row, row.type.startsWith("result.") && row.role !== null ? (latest.get(row.role) ?? null) : null);
    return {
      seq: row.seq,
      time: row.time,
      role: whoOf(row),
      message: row.message,
      label: label || fallbackLabel(row),
      details: label ? details || null : null,
      problem: isProblem(row),
      routine: isRoutine(row),
      row,
    };
  });
}

/** Lines to show, newest first. Routine rows stay out unless `showRoutine`. */
export function visibleLines(lines: readonly TimelineLine[], filter: Filter, showRoutine: boolean): TimelineLine[] {
  return lines.filter((l) => (showRoutine || !l.routine) && matchesFilter(l.row, filter)).sort((a, b) => b.seq - a.seq);
}

/** Rows from a later fetch join the rows already held; a seq seen twice keeps one copy. Oldest first. */
export function mergeRows(have: readonly EventRow[], more: readonly EventRow[]): EventRow[] {
  const bySeq = new Map<number, EventRow>();
  for (const r of [...have, ...more]) bySeq.set(r.seq, r);
  return [...bySeq.values()].sort((a, b) => a.seq - b.seq);
}

/** True when the server's newest seq is below the newest row held: the store was replaced, so the held rows no longer belong to it. */
export const storeRestarted = (rows: readonly EventRow[], serverLastSeq: number): boolean => {
  const held = lastSeq(rows);
  return held !== undefined && serverLastSeq < held;
};

/** The highest seq held, or undefined when no row is held (ask for the full timeline). */
export const lastSeq = (rows: readonly EventRow[]): number | undefined => rows.reduce<number | undefined>((hi, r) => (hi === undefined || r.seq > hi ? r.seq : hi), undefined);

/** `just now`, `3 min ago`, `2 h 5 min ago`, `8 days ago`; a time in the future reads as `just now`. */
export function agoText(iso: string, now: number): string {
  const s = Math.round((now - new Date(iso).getTime()) / 1000);
  if (s < 5) return "just now";
  return s >= 172_800 ? `${Math.floor(s / 86_400)} days ago` : `${spanText(s)} ago`;
}
