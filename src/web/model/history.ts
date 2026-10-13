import type { ArchiveEntry, FileRef } from "../../shared/api.ts";

const when = (a: ArchiveEntry) => a.closedAt ?? a.date;

/** Archives grouped by team. Teams ordered by their newest archive, newest first; archives newest first. */
export function groupArchives(archives: readonly ArchiveEntry[]): { team: string; archives: ArchiveEntry[] }[] {
  const byTeam = new Map<string, ArchiveEntry[]>();
  for (const a of archives) byTeam.set(a.team, [...(byTeam.get(a.team) ?? []), a]);
  return [...byTeam.entries()]
    .map(([team, list]) => ({
      team,
      // The directory name breaks ties, since its time suffix grows with the close time.
      archives: [...list].sort((x, y) => when(y).localeCompare(when(x)) || y.dir.localeCompare(x.dir)),
    }))
    .sort((a, b) => when(b.archives[0]!).localeCompare(when(a.archives[0]!)) || a.team.localeCompare(b.team));
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
