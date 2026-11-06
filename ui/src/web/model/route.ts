// Hash routes. Names from the URL are checked against the same patterns the
// server uses, so a crafted hash cannot reach another API path.

import { NAME_RE } from "../../shared/teamwire.ts";

/** The team page's tabs, in the order they are shown. A hash that names none means Overview. */
export const TABS = ["overview", "messages", "decisions", "tokens", "timeline"] as const;
export type Tab = (typeof TABS)[number];
const isTab = (p: string | undefined): p is Tab => p !== undefined && (TABS as readonly string[]).includes(p);

export type Route =
  | { page: "overview" }
  | { page: "team"; team: string; tab: Tab; member: string | null }
  | { page: "history" }
  | { page: "archive"; dir: string; member: string | null }
  | { page: "notfound"; hash: string };

const ARCHIVE_RE = /^[a-z][a-z0-9_]*-\d{4}-\d{2}-\d{2}(-\d{6}(-\d+)?)?$/;

export function parseRoute(hash: string): Route {
  const path = hash.replace(/^#/, "");
  if (path === "" || path === "/") return { page: "overview" };
  const parts = path.split("/").filter((p, i) => i > 0 || p !== "");
  const [head, a, ...rest] = parts;
  if (head === "history" && a === undefined) return { page: "history" };
  if (head === "history" && a !== undefined && ARCHIVE_RE.test(a)) {
    // `#/history/<dir>[/member/<role>]`
    if (rest.length === 0) return { page: "archive", dir: a, member: null };
    if (rest.length === 2 && rest[0] === "member" && NAME_RE.test(rest[1]!)) return { page: "archive", dir: a, member: rest[1]! };
  }
  if (head === "team" && a !== undefined && NAME_RE.test(a)) {
    // `#/team/<team>[/<tab>][/member/<role>]`
    const named = rest[0];
    const tab: Tab = isTab(named) ? named : "overview";
    if (isTab(named)) rest.shift();
    if (rest.length === 0) return { page: "team", team: a, tab, member: null };
    if (rest.length === 2 && rest[0] === "member" && NAME_RE.test(rest[1]!)) {
      return { page: "team", team: a, tab, member: rest[1]! };
    }
  }
  return { page: "notfound", hash };
}

export const href = {
  overview: () => "#/",
  team: (team: string, tab: Tab = "overview") => (tab === "overview" ? `#/team/${team}` : `#/team/${team}/${tab}`),
  member: (team: string, role: string, tab: Tab = "overview") => `${href.team(team, tab)}/member/${role}`,
  history: () => "#/history",
  archive: (dir: string) => `#/history/${dir}`,
  archiveMember: (dir: string, role: string) => `#/history/${dir}/member/${role}`,
};

/** The page a role link opens a drawer on, and the roles that page knows besides `lead`. */
export type RolePlace =
  | { page: "team"; team: string; tab: Tab; roles: ReadonlySet<string> }
  | { page: "archive"; dir: string; roles: ReadonlySet<string> };

/**
 * The drawer link for a role, or null to leave it as text: `user`, a party of
 * another team (`api@billing`, `lead@billing`) and a role the page does not know.
 * `lead` is a reserved role in teamwire, so it never names a member.
 */
export function roleHref(role: string, at: RolePlace): string | null {
  if (role !== "lead" && (role === "user" || !NAME_RE.test(role) || !at.roles.has(role))) return null;
  return at.page === "team" ? href.member(at.team, role, at.tab) : href.archiveMember(at.dir, role);
}
