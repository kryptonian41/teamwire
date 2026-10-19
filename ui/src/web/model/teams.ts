// Pure helpers over the snapshot. Components stay thin by calling these.

import type { Metric, MemberKind, MemberView, RosterMember, StatusView, TeamEntry } from "../../shared/teamwire.ts";
import { CATEGORY_ORDER, classifyState, type StateCategory } from "../../shared/state.ts";

export interface JoinedMember extends MemberView {
  /** From the roster. Null when the roster has no entry for this role. */
  kind: MemberKind | null;
  category: StateCategory;
}

export interface TeamModel {
  /** Route key. Falls back to the title or an index when teamwire gave no name. */
  key: string;
  name: string | null;
  title: string;
  status: StatusView;
  roster: TeamEntry | null;
  members: JoinedMember[];
  /** Roster members with lifecycle `removed`. `status` never lists them. */
  removed: RosterMember[];
}

/** Joins each `status` member with its roster entry by team name and role. */
export function joinTeam(status: StatusView, teams: readonly TeamEntry[], index = 0): TeamModel {
  const roster = status.team === null ? null : (teams.find((t) => t.name === status.team) ?? null);
  const kinds = new Map(roster?.members.map((m) => [m.role, m.kind] as const));
  const members = status.members.map((m) => ({
    ...m,
    kind: kinds.get(m.role) ?? null,
    category: classifyState(m.state),
  }));
  return {
    key: status.team ?? `unnamed-${index}`,
    name: status.team,
    title: status.title ?? roster?.title ?? status.team ?? "(unnamed team)",
    status,
    roster,
    members,
    removed: roster?.members.filter((m) => m.lifecycle === "removed") ?? [],
  };
}

export function joinAll(status: readonly StatusView[], teams: readonly TeamEntry[]): TeamModel[] {
  return status.map((s, i) => joinTeam(s, teams, i));
}

export function countByCategory(members: readonly { category: StateCategory }[]): Record<StateCategory, number> {
  const counts = Object.fromEntries(CATEGORY_ORDER.map((c) => [c, 0])) as Record<StateCategory, number>;
  for (const m of members) counts[m.category] += 1;
  return counts;
}

const QUESTION_WAIT_S = 600;

/** The signals of D18 as card flags. Lead, watch and AFK count only while the team is open. */
export function problemReasons(t: TeamModel): string[] {
  const s = t.status;
  const out: string[] = [];
  if (s.state === "open") {
    const lead = leadCategory(s.lead);
    if (lead === "failed" || lead === "attention") out.push(livenessInfo(s.lead).label);
    if (s.watch.state === "stale") out.push("watch stale");
    if (s.afk?.spent) out.push("AFK spent");
  }
  if (s.unread_deliveries.length > 0) out.push(`${s.unread_deliveries.length} unread`);
  const waiting = s.unanswered_questions.filter((q) => q.to.includes("lead") && q.age_s >= QUESTION_WAIT_S);
  if (waiting.length > 0) out.push(`${waiting.length} question waiting`);
  return out;
}

/** A team has a problem when teamwire reported errors, or something needs the lead. */
export function hasProblem(t: TeamModel): boolean {
  const s = t.status;
  return (
    s.team === null ||
    s.errors.length > 0 ||
    s.stuck_deliveries.length > 0 ||
    s.undelivered_results.length > 0 ||
    problemReasons(t).length > 0 ||
    t.members.some((m) => m.category === "failed" || m.category === "attention" || m.category === "unknown")
  );
}

/** Problems first, then by title, then by key so the order never flickers. */
export function sortTeams(teams: readonly TeamModel[]): TeamModel[] {
  return [...teams].sort((a, b) => {
    const pa = hasProblem(a) ? 0 : 1;
    const pb = hasProblem(b) ? 0 : 1;
    if (pa !== pb) return pa - pb;
    return a.title.localeCompare(b.title) || a.key.localeCompare(b.key);
  });
}

function duration(s: number): string {
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return m === 0 ? `${h}h` : `${h}h${m}m`;
}

export interface AgeView {
  /** For example `20m / 60m`. */
  label: string;
  overdue: boolean;
  /** 0 to 1, for a bar. A zero deadline counts as full. */
  ratio: number;
}

export function formatAge(age_s: number, deadline_s: number): AgeView {
  return {
    label: `${duration(age_s)} / ${duration(deadline_s)}`,
    overdue: age_s > deadline_s,
    ratio: deadline_s <= 0 ? 1 : Math.min(1, age_s / deadline_s),
  };
}

export interface TeamMetrics {
  /** Completions by their `status` label. */
  completions: Record<string, number>;
  messagesSent: number;
  leadWakeups: number;
  verifyRuns: number;
  /** Means for histograms: name -> { mean, count }. */
  durations: Record<string, { mean: number; count: number }>;
}

function emptyMetrics(): TeamMetrics {
  return { completions: {}, messagesSent: 0, leadWakeups: 0, verifyRuns: 0, durations: {} };
}

/**
 * Groups metrics by their `team` label. A value without `.count` is the mean
 * for a histogram and the total for a counter; `<name>.count` is how many
 * observations the mean covers.
 */
export function summarizeMetrics(metrics: readonly Metric[]): Map<string, TeamMetrics> {
  const out = new Map<string, TeamMetrics>();
  const histograms = new Map<string, { mean: number; count: number }>();
  for (const m of metrics) {
    const team = m.labels.find(([k]) => k === "team")?.[1];
    if (team === undefined) continue;
    const sum = out.get(team) ?? emptyMetrics();
    out.set(team, sum);
    const label = (k: string) => m.labels.find(([lk]) => lk === k)?.[1];
    switch (m.name) {
      case "teamwire.completions": {
        const status = label("status") ?? "unknown";
        sum.completions[status] = (sum.completions[status] ?? 0) + m.value;
        break;
      }
      case "teamwire.messages.sent":
        sum.messagesSent += m.value;
        break;
      case "teamwire.lead.wakeups":
        sum.leadWakeups += m.value;
        break;
      case "teamwire.verify.runs":
        sum.verifyRuns += m.value;
        break;
      default: {
        if (m.name.endsWith(".count")) {
          const base = m.name.slice(0, -".count".length);
          const h = histograms.get(`${team}|${base}`);
          if (h) h.count = m.value;
          else {
            const entry = { mean: 0, count: m.value };
            histograms.set(`${team}|${base}`, entry);
            sum.durations[base] = entry;
          }
        } else if (m.unit === "s" || m.name.includes("attempts")) {
          const h = histograms.get(`${team}|${m.name}`);
          if (h) h.mean = m.value;
          else {
            const entry = { mean: m.value, count: 0 };
            histograms.set(`${team}|${m.name}`, entry);
            sum.durations[m.name] = entry;
          }
        }
      }
    }
  }
  return out;
}

/** The one place that maps the lead's liveness to a colour category. */
export function leadCategory(lead: StatusView["lead"]): StateCategory {
  if (lead === null) return "unknown";
  const l = lead.liveness;
  if (l === "dead") return "failed";
  if (l === "unknown") return "unknown";
  switch (l.alive) {
    case "working":
      return "active";
    case "idle":
      return "idle";
    case "blocked":
      return "attention";
    case "gone":
      return "failed";
    case "unknown":
      return "unknown";
  }
}

/** The lead's liveness as a category and a label. */
export function livenessInfo(lead: StatusView["lead"]): { category: StateCategory; label: string } {
  if (lead === null) return { category: leadCategory(lead), label: "no lead" };
  const l = lead.liveness;
  return { category: leadCategory(lead), label: `lead ${typeof l === "string" ? l : l.alive}` };
}
