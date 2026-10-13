import type { ArchiveEntry, FileRef } from "../../shared/api.ts";

const when = (a: ArchiveEntry) => a.closedAt ?? a.date;

/** Archives grouped by team. Teams in name order, archives newest first. */
export function groupArchives(archives: readonly ArchiveEntry[]): { team: string; archives: ArchiveEntry[] }[] {
  const byTeam = new Map<string, ArchiveEntry[]>();
  for (const a of archives) byTeam.set(a.team, [...(byTeam.get(a.team) ?? []), a]);
  return [...byTeam.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([team, list]) => ({
      team,
      // The directory name breaks ties, since its time suffix grows with the close time.
      archives: [...list].sort((x, y) => when(y).localeCompare(when(x)) || y.dir.localeCompare(x.dir)),
    }));
}

/** Files by role. Message files (`comms`) form one group named by `MESSAGES`. */
export const MESSAGES = "(messages)";

export function groupFilesByRole(files: readonly FileRef[]): { role: string; files: FileRef[] }[] {
  const groups = new Map<string, FileRef[]>();
  for (const f of files) {
    const key = f.kind === "comms" ? MESSAGES : f.name;
    groups.set(key, [...(groups.get(key) ?? []), f]);
  }
  return [...groups.entries()]
    .sort(([a], [b]) => (a === MESSAGES ? 1 : b === MESSAGES ? -1 : a.localeCompare(b)))
    .map(([role, list]) => ({ role, files: list }));
}
