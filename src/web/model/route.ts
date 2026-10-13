// Hash routes. Names from the URL are checked against the same patterns the
// server uses, so a crafted hash cannot reach another API path.

import { NAME_RE } from "../../shared/teamwire.ts";

export type Route =
  | { page: "overview" }
  | { page: "team"; team: string; member: string | null }
  | { page: "history" }
  | { page: "archive"; dir: string }
  | { page: "notfound"; hash: string };

const ARCHIVE_RE = /^[a-z][a-z0-9_]*-\d{4}-\d{2}-\d{2}(-\d{6})?$/;

export function parseRoute(hash: string): Route {
  const path = hash.replace(/^#/, "");
  if (path === "" || path === "/") return { page: "overview" };
  const parts = path.split("/").filter((p, i) => i > 0 || p !== "");
  const [head, a, b, c] = parts;
  if (head === "history" && a === undefined) return { page: "history" };
  if (head === "history" && a !== undefined && parts.length === 2 && ARCHIVE_RE.test(a)) {
    return { page: "archive", dir: a };
  }
  if (head === "team" && a !== undefined && NAME_RE.test(a)) {
    if (parts.length === 2) return { page: "team", team: a, member: null };
    if (parts.length === 4 && b === "member" && c !== undefined && NAME_RE.test(c)) {
      return { page: "team", team: a, member: c };
    }
  }
  return { page: "notfound", hash };
}

export const href = {
  overview: () => "#/",
  team: (team: string) => `#/team/${team}`,
  member: (team: string, role: string) => `#/team/${team}/member/${role}`,
  history: () => "#/history",
  archive: (dir: string) => `#/history/${dir}`,
};
