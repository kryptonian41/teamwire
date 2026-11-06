// The member drawer's record: one shape for a live member, a removed member,
// an archived member and the lead. Each fact holds a value or the reason it is
// missing, so the drawer lays out the same sections for every subject.

import type { FileKind, FileRef } from "../../shared/api.ts";
import type { StateCategory } from "../../shared/state.ts";
import type { EventRow, LeadView } from "../../shared/teamwire.ts";
import { canMessage, canReprompt } from "./actions.ts";
import { tabsFor } from "./files.ts";
import {
  createdKind,
  handoffHistory,
  highestGeneration,
  leadRows,
  leadSessions,
  modelText,
  removal,
  removalText,
  rolesIn,
  rowsForRole,
  tokensByModel,
  verifyHistory,
  type Handoff,
  type LeadSession,
  type Usage,
  type VerifyRun,
} from "./memberEvents.ts";
import { afkLinks, startedByLine } from "./status.ts";
import { formatAge, livenessInfo, type AgeView, type TeamModel } from "./teams.ts";

export const WHY = {
  removed: "not available: removed",
  archived: "not available: archived",
  lead: "not applicable to the lead",
  noRoster: "not in the roster",
  leadTokens: "teamwire records no token use for the lead",
  leadFiles: "the lead has no files here",
} as const;

export type Fact<T> = { ok: true; value: T } | { ok: false; why: string };
const has = <T>(value: T): Fact<T> => ({ ok: true, value });
const not = <T>(why: string): Fact<T> => ({ ok: false, why });

export type Subject = "live" | "removed" | "archived" | "lead";

export interface RosterFacts {
  title: string;
  owns: string[];
  acceptsFromMembers: string[];
  canBeTypedTo: boolean;
}

/** One file the drawer can show: `name` is the role, or an AFK file name for the lead. */
export interface FileTab {
  kind: FileKind;
  name: string;
  label: string;
}

export interface Detail {
  role: string;
  subject: Subject;
  state: { word: string; category: StateCategory };
  age: Fact<AgeView>;
  roster: Fact<RosterFacts>;
  kind: Fact<string>;
  generation: Fact<number>;
  branch: Fact<string | null>;
  model: Fact<string | null>;
  build: string | null;
  /** Why the member was removed; null when it never was, and for the lead. */
  removal: string | null;
  /** The lead's start and adoptions; null for a member. */
  sessions: LeadSession[] | null;
  /** The lead's current session id; null for a member. */
  session: Fact<string | null> | null;
  handoffs: Fact<Handoff[]>;
  verify: Fact<VerifyRun[]>;
  tokens: Fact<[string, Usage][]>;
  events: EventRow[];
  files: Fact<FileTab[]>;
  /** True when the drawer may load the member's resume command. */
  resume: Fact<true>;
  /** Which action buttons to show; null shows none. */
  actions: { reprompt: boolean; note: boolean; close: boolean } | null;
}

export type DetailSource =
  | { page: "team"; team: TeamModel; rows: readonly EventRow[]; files: readonly FileRef[]; actions: boolean }
  | { page: "archive"; rows: readonly EventRow[]; files: readonly FileRef[] };

/** The roles the source knows besides `lead`: live and removed members on the team page, created roles in an archive. */
export function knownRoles(src: DetailSource): Set<string> {
  if (src.page === "archive") return new Set(rolesIn(src.rows));
  return new Set([...src.team.members.map((m) => m.role), ...src.team.removed.map((m) => m.role)]);
}

const memberFiles = (files: readonly FileRef[], role: string): FileTab[] => tabsFor(files, role).map((t) => ({ ...t, name: role }));

/** The event-log facts every member shares; `why` replaces the ones a closed source no longer has. */
function fromLog(rows: readonly EventRow[], role: string) {
  return {
    model: has(modelText(rows, role)),
    handoffs: has(handoffHistory(rows, role)),
    verify: has(verifyHistory(rows, role)),
    tokens: has([...tokensByModel(rows, role)]),
    events: rowsForRole(rows, role),
    sessions: null,
    session: null,
  };
}

/** The drawer's record for `role`, or null when no source knows it. */
export function buildDetail(role: string, src: DetailSource): Detail | null {
  if (role === "lead") return leadDetail(src);
  const { rows, files } = src;
  if (src.page === "archive") {
    if (!knownRoles(src).has(role)) return null;
    return {
      role,
      subject: "archived",
      state: { word: "archived", category: "idle" },
      age: not(WHY.archived),
      roster: not(WHY.archived),
      kind: has(createdKind(rows, role) ?? "unknown"),
      generation: has(highestGeneration(rows, role)),
      branch: not(WHY.archived),
      build: null,
      removal: removal(rows, role) ? removalText(rows, role) : null,
      ...fromLog(rows, role),
      files: has(memberFiles(files, role)),
      resume: not(WHY.archived),
      actions: null,
    };
  }
  const { team } = src;
  const entry = team.roster?.members.find((m) => m.role === role) ?? null;
  const roster: Fact<RosterFacts> = entry
    ? has({ title: entry.title, owns: entry.owns, acceptsFromMembers: entry.accepts_from_members, canBeTypedTo: entry.can_be_typed_to })
    : not(WHY.noRoster);
  const live = team.members.find((m) => m.role === role);
  if (live) {
    return {
      role,
      subject: "live",
      state: { word: live.state, category: live.category },
      age: has(formatAge(live.age_s, live.deadline_s)),
      roster,
      kind: has(live.kind ?? "unknown"),
      generation: has(live.generation),
      branch: has(live.branch),
      build: startedByLine(live),
      removal: null,
      ...fromLog(rows, role),
      files: has(memberFiles(files, role)),
      resume: has(true),
      actions: src.actions
        ? { reprompt: canReprompt(live.kind, entry?.lifecycle ?? null), note: canMessage(live.kind), close: true }
        : null,
    };
  }
  const gone = team.removed.find((m) => m.role === role);
  if (!gone) return null;
  return {
    role,
    subject: "removed",
    state: { word: "removed", category: "idle" },
    age: not(WHY.removed),
    roster,
    kind: has(gone.kind),
    generation: has(gone.generation),
    branch: not(WHY.removed),
    build: null,
    removal: removalText(rows, role),
    ...fromLog(rows, role),
    // Closing a member keeps its files.
    files: has(memberFiles(files, role)),
    resume: not(WHY.removed),
    actions: null,
  };
}

function leadDetail(src: DetailSource): Detail {
  const archived = src.page === "archive";
  const lead: LeadView | null = src.page === "team" ? src.team.status.lead : null;
  const live = livenessInfo(lead);
  const afk: FileTab[] = afkLinks(src.files).map((l) => ({ kind: "afk", name: l.name, label: l.label }));
  return {
    role: "lead",
    subject: "lead",
    state: archived ? { word: "archived", category: "idle" } : { word: live.label, category: live.category },
    age: not(WHY.lead),
    roster: not(WHY.lead),
    kind: not(WHY.lead),
    generation: not(WHY.lead),
    branch: not(WHY.lead),
    model: not(WHY.lead),
    build: null,
    removal: null,
    sessions: leadSessions(src.rows),
    session: archived ? not(WHY.archived) : has(lead?.session_id ?? null),
    handoffs: not(WHY.lead),
    verify: not(WHY.lead),
    tokens: not(WHY.leadTokens),
    events: leadRows(src.rows),
    files: afk.length > 0 ? has(afk) : not(WHY.leadFiles),
    resume: not(WHY.lead),
    actions: null,
  };
}
