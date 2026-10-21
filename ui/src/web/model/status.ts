// Plain-language lines for the status data the page receives. Pure, so each
// worked example in the brief is a test.

import { AFK_FILE_NAMES, type AfkFileName, type FileRef } from "../../shared/api.ts";
import type { AfkView, LogDelivery, LogMessage, MemberView, RosterMember, StatusView } from "../../shared/teamwire.ts";

/** Seconds as `4 s`, `12 min` or `1 h 12 min`. */
export function spanText(seconds: number): string {
  const s = Math.round(seconds);
  if (s < 60) return `${s} s`;
  const min = Math.floor(s / 60);
  if (min < 60) return `${min} min`;
  const h = Math.floor(min / 60);
  return min % 60 === 0 ? `${h} h` : `${h} h ${min % 60} min`;
}

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

export function watchLine(w: StatusView["watch"]): string {
  if (w.state === "none") return "No watch running";
  const beat = w.heartbeat_age_s === null ? null : spanText(w.heartbeat_age_s);
  if (w.state === "stale") return `Watch stale · ${beat === null ? "no heartbeat recorded" : `last heartbeat ${beat} ago`}`;
  return ["Watch running", w.pid === null ? null : `pid ${w.pid}`, beat === null ? null : `heartbeat ${beat} ago`]
    .filter((p) => p !== null)
    .join(" · ");
}

/** `HH:MM` of an RFC 3339 time. `timeZone` is for tests; the page uses the viewer's zone. */
export function clockText(iso: string, timeZone?: string): string {
  return new Date(iso).toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit", hour12: false, timeZone });
}

export function afkLine(afk: AfkView, timeZone?: string): string {
  const ends = clockText(afk.budget_ends_at, timeZone);
  const decisions = plural(afk.decisions, "decision");
  return afk.spent ? `AFK budget spent at ${ends} · ${decisions}` : `AFK run · ${spanText(afk.seconds_left)} left · ends ${ends} · ${decisions}`;
}

/** The short chip on an Overview card. */
export function afkChip(afk: AfkView): string {
  return afk.spent ? "AFK budget spent" : `AFK ${spanText(afk.seconds_left)} left`;
}

export function unreadLines(s: Pick<StatusView, "unread_deliveries">): string[] {
  return s.unread_deliveries.map((d) => `${d.display_id} to ${d.to}: handed over ${spanText(d.age_s)} ago, not read yet`);
}

/** Every unanswered question, whoever it was sent to. */
export function unansweredLines(s: Pick<StatusView, "unanswered_questions">): string[] {
  return s.unanswered_questions.map(
    (q) => `${q.display_id} question from ${q.from} to ${q.to.join(", ")}: asked ${spanText(q.age_s)} ago, no answer yet`,
  );
}

export function startedByLine(m: Pick<MemberView, "started_by" | "older_build">): string | null {
  if (m.started_by === null) return null;
  return `Started by teamwire ${m.started_by}${m.older_build ? " (older than this teamwire)" : ""}`;
}

export function ownsLine(r: Pick<RosterMember, "owns">): string {
  return `May change: ${r.owns.length === 0 ? "nothing (read-only)" : r.owns.join(", ")}`;
}

export function acceptsLine(r: Pick<RosterMember, "accepts_from_members" | "can_be_typed_to">): string {
  const types = r.accepts_from_members.length === 0 ? "nothing" : r.accepts_from_members.join(", ");
  return `Accepts from members: ${types} · Lead can type to it: ${r.can_be_typed_to ? "yes" : "no"}`;
}

/** The text of a delivery chip: `api undelivered · not_at_input_prompt · 3 tries`. */
export function deliveryText(d: Pick<LogDelivery, "to" | "state" | "attempts" | "reason">): string {
  return [`${d.to} ${d.state}`, d.reason, d.attempts > 1 ? `${d.attempts} tries` : null].filter((p) => p != null).join(" · ");
}

/** `answers m006`, and the message to select, only when the table holds it. */
export function answersLink(m: Pick<LogMessage, "re">, ids: ReadonlySet<string>): { text: string; target: string | null } | null {
  if (!m.re) return null;
  return { text: `answers ${m.re}`, target: ids.has(m.re) ? m.re : null };
}

const AFK_LABELS: Record<AfkFileName, string> = { decisions: "Decisions", REPORT: "Report", task: "Task" };

/** Links for the AFK files present in a file list, in a fixed order. */
export function afkLinks(files: readonly FileRef[]): { name: AfkFileName; label: string }[] {
  const have = new Set(files.filter((f) => f.kind === "afk").map((f) => f.name));
  return AFK_FILE_NAMES.filter((n) => have.has(n)).map((name) => ({ name, label: AFK_LABELS[name] }));
}

/** Plain info on an Overview card, not a problem flag. */
export function watchChip(w: StatusView["watch"]): string | null {
  return w.state === "none" ? "no watch" : null;
}
