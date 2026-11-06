// What the event log says about one member: model, latest verify, token use
// and why it was removed. Pure functions over the rows the timeline holds.

import type { StateCategory } from "../../shared/state.ts";
import type { EventRow } from "../../shared/teamwire.ts";

const str = (d: EventRow["data"], k: string): string | null => (typeof d[k] === "string" && d[k] !== "" ? (d[k] as string) : null);
const num = (d: EventRow["data"], k: string): number | null => (typeof d[k] === "number" && Number.isFinite(d[k]) ? (d[k] as number) : null);
const strs = (d: EventRow["data"], k: string): string[] => (Array.isArray(d[k]) ? (d[k] as unknown[]).filter((x): x is string => typeof x === "string") : []);

/** The member's own rows, in the order given. */
export const rowsForRole = (rows: readonly EventRow[], role: string): EventRow[] => rows.filter((r) => r.role === role);

/** The rows are oldest first, so the last match is the newest. */
const newest = (rows: readonly EventRow[], role: string, type: EventRow["type"]): EventRow | undefined =>
  rows.findLast((r) => r.role === role && r.type === type);

/** `sonnet · medium` from the member's latest `member.started`, or null. */
export function modelText(rows: readonly EventRow[], role: string): string | null {
  const r = newest(rows, role, "member.started");
  if (!r) return null;
  const parts = [str(r.data, "model"), str(r.data, "effort")].filter((p): p is string => p !== null);
  return parts.length === 0 ? null : parts.join(" · ");
}

export interface Waiver {
  seq: number;
  time: string;
  flags: string[];
  headSha: string | null;
}

export interface VerifyRun {
  seq: number;
  time: string;
  verdict: string | null;
  exit: number | null;
  seconds: number | null;
  /** The full commit the run checked; waivers keep the short form. */
  headSha: string | null;
  flags: string[];
  /** Waivers recorded after this run and before the member's next run. */
  waivers: Waiver[];
  /** `teamwire verify` gives PASS only when every flag is waived, so a PASS run's flags are all waived. */
  flagsWaived: boolean;
}

/** The member's verify runs, newest first, each with the waivers recorded for it. */
export function verifyHistory(rows: readonly EventRow[], role: string): VerifyRun[] {
  const runs: VerifyRun[] = [];
  for (const r of rows) {
    if (r.role !== role) continue;
    if (r.type === "verify.recorded") {
      runs.push({
        seq: r.seq,
        time: r.time,
        verdict: str(r.data, "verdict"),
        exit: num(r.data, "exit"),
        seconds: num(r.data, "seconds"),
        headSha: str(r.data, "head_sha"),
        flags: strs(r.data, "flags"),
        waivers: [],
        flagsWaived: str(r.data, "verdict") === "PASS" && strs(r.data, "flags").length > 0,
      });
    } else if (r.type === "verify.waived") {
      runs.at(-1)?.waivers.push({ seq: r.seq, time: r.time, flags: strs(r.data, "flags"), headSha: str(r.data, "head_sha")?.slice(0, 7) ?? null });
    }
  }
  return runs.reverse();
}

const VERDICT_CATEGORY: Record<string, StateCategory> = { PASS: "done", FLAGGED: "attention", FAIL: "failed" };
export const verdictCategory = (verdict: string | null): StateCategory => (verdict === null ? "idle" : (VERDICT_CATEGORY[verdict] ?? "attention"));

/** The chip text and colour: `PASS`, `FLAGGED · 2`, `FAIL`, or `—` when the member never ran verify. */
export function verifyChip(rows: readonly EventRow[], role: string): { text: string; category: StateCategory } {
  const run = verifyHistory(rows, role)[0];
  if (!run?.verdict) return { text: "—", category: "idle" };
  return {
    text: run.flags.length === 0 ? run.verdict : `${run.verdict} · ${run.flags.length}${run.flagsWaived ? " waived" : ""}`,
    category: verdictCategory(run.verdict),
  };
}

export interface Handoff {
  seq: number;
  time: string;
  /** The generation the member has after the handoff. */
  generation: number | null;
  oldSession: string | null;
  /** Null when the harness picks its own id. */
  newSession: string | null;
}

/** The member's handoffs, newest first. */
export function handoffHistory(rows: readonly EventRow[], role: string): Handoff[] {
  return rows
    .filter((r) => r.role === role && r.type === "member.handoff")
    .map((r) => ({ seq: r.seq, time: r.time, generation: num(r.data, "generation"), oldSession: str(r.data, "old_session"), newSession: str(r.data, "new_session") }))
    .reverse();
}

export interface Usage {
  input: number;
  output: number;
  cacheRead: number;
  cacheCreation: number;
}

const zero = (): Usage => ({ input: 0, output: 0, cacheRead: 0, cacheCreation: 0 });
export const usageTotal = (u: Usage): number => u.input + u.output + u.cacheRead + u.cacheCreation;

/**
 * Token use per model. A `result.delivering` row carries the session's
 * running totals, so only the newest row of each session counts and the
 * sessions add up. Empty when no row carries usage.
 */
export function tokensByModel(rows: readonly EventRow[], role: string): Map<string, Usage> {
  const newestPerSession = new Map<string, unknown>();
  for (const r of rows) {
    if (r.type !== "result.delivering" || r.role !== role || typeof r.data["usage"] !== "object" || r.data["usage"] === null) continue;
    // A row without a session id cannot be matched to another, so it stands alone.
    newestPerSession.set(str(r.data, "session") ?? `seq ${r.seq}`, r.data["usage"]);
  }
  const out = new Map<string, Usage>();
  for (const usage of newestPerSession.values()) {
    for (const [model, u] of Object.entries(usage as object)) {
      if (typeof u !== "object" || u === null) continue;
      const have = out.get(model) ?? zero();
      const d = u as EventRow["data"];
      have.input += num(d, "input") ?? 0;
      have.output += num(d, "output") ?? 0;
      have.cacheRead += num(d, "cache_read") ?? 0;
      have.cacheCreation += num(d, "cache_creation") ?? 0;
      out.set(model, have);
    }
  }
  return out;
}

/** `950`, `70k`, `5.3M`. */
export function countText(n: number): string {
  const one = (x: number, unit: string) => `${x.toFixed(1).replace(/\.0$/, "")}${unit}`;
  if (n >= 1e6) return one(n / 1e6, "M");
  if (n >= 1e4) return `${Math.round(n / 1e3)}k`;
  if (n >= 1e3) return one(n / 1e3, "k");
  return String(n);
}

/** `70k out · 5.3M total`, or `—` when no row carries usage. */
export function tokensText(rows: readonly EventRow[], role: string): string {
  const per = tokensByModel(rows, role);
  if (per.size === 0) return "—";
  let out = 0;
  let total = 0;
  for (const u of per.values()) {
    out += u.output;
    total += usageTotal(u);
  }
  return `${countText(out)} out · ${countText(total)} total`;
}

export interface Removal {
  time: string;
  unverified: boolean;
  reasons: string[];
}

/** Why the member was removed, from its latest `member.removed` row; null when it never was. */
export function removal(rows: readonly EventRow[], role: string): Removal | null {
  const r = newest(rows, role, "member.removed");
  return r ? { time: r.time, unverified: r.data["unverified"] === true, reasons: strs(r.data, "unverified_reasons") } : null;
}

/** The roles that were ever created, in the order their first row appears. */
export function rolesIn(rows: readonly EventRow[]): string[] {
  const roles = new Set<string>();
  for (const r of rows) if (r.role !== null && (r.type === "member.creating" || r.type === "member.started")) roles.add(r.role);
  return [...roles];
}

/** Why a member is gone, for a table cell: its first reason, else a plain sentence; `—` when never removed. */
export function removalText(rows: readonly EventRow[], role: string): string {
  const r = removal(rows, role);
  if (!r) return "—";
  return r.reasons[0] ?? (r.unverified ? "removed without a passing verify" : "removed");
}

/** The kind the member's first `member.creating` row recorded, or null. */
export function createdKind(rows: readonly EventRow[], role: string): string | null {
  const r = rows.find((x) => x.role === role && x.type === "member.creating");
  return r ? str(r.data, "kind") : null;
}

/** The highest `generation` any of the member's rows carries. A member starts at generation 1, so 1 when no row carries one. */
export function highestGeneration(rows: readonly EventRow[], role: string): number {
  let g = 1;
  for (const r of rows) if (r.role === role) g = Math.max(g, num(r.data, "generation") ?? 1);
  return g;
}

/** The rows that involve the lead: its own deliveries, messages it sent or got, and its sessions. */
export function leadRows(rows: readonly EventRow[]): EventRow[] {
  return rows.filter(
    (r) =>
      r.role === "lead" ||
      r.type === "team.created" ||
      r.type === "team.lead_adopted" ||
      (r.type === "message.sent" && (r.data["sender_kind"] === "lead" || strs(r.data, "recipients").includes("lead"))),
  );
}

export interface LeadSession {
  seq: number;
  time: string;
  event: "created" | "adopted";
  harness: string | null;
  host: string | null;
  oldSession: string | null;
  newSession: string | null;
  folder: string | null;
}

/** The team's start and every `teamwire lead --adopt`, oldest first. */
export function leadSessions(rows: readonly EventRow[]): LeadSession[] {
  return rows
    .filter((r) => r.type === "team.created" || r.type === "team.lead_adopted")
    .map((r) => ({
      seq: r.seq,
      time: r.time,
      event: r.type === "team.created" ? "created" : "adopted",
      harness: str(r.data, "harness"),
      host: str(r.data, "host"),
      oldSession: str(r.data, "old_session"),
      newSession: str(r.data, "new_session"),
      folder: str(r.data, "lead_cwd"),
    }));
}
