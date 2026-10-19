// Everything the server reads from disk. A path is built only from the
// FILE_KINDS table and a validated name, then its real path (symlinks
// followed) must stay inside the team's or archive's own directory.

import { readdir, realpath, stat } from "node:fs/promises";
import { join, relative, sep, isAbsolute } from "node:path";
import {
  FILE_KINDS,
  FILE_KIND_NAMES,
  MAX_FILE_BYTES,
  type ArchiveEntry,
  type FileKind,
  type FileRef,
  type FileText,
  type Problem,
} from "../shared/api.ts";
import { DISPLAY_ID_RE, NAME_RE } from "../shared/teamwire.ts";
import { parseMarkdownBounded } from "./markdown.ts";

export type FileResult = { ok: true; file: FileText } | { ok: false; problem: Problem };

export function isFileKind(value: string): value is FileKind {
  return Object.hasOwn(FILE_KINDS, value);
}

function nameOk(kind: FileKind, name: string): boolean {
  return (FILE_KINDS[kind].name === "display_id" ? DISPLAY_ID_RE : NAME_RE).test(name);
}

const notFound = (detail: string): { ok: false; problem: Problem } => ({
  ok: false,
  problem: { code: "not_found", detail },
});

/** True when `child` is `base` or inside it. Both must be real paths. */
function inside(base: string, child: string): boolean {
  const rel = relative(base, child);
  return rel !== "" && !rel.startsWith("..") && !isAbsolute(rel);
}

/**
 * Resolves one file to its real path, or null. The result is inside `base`
 * and never inside `base/members`, whatever symlinks say. A file with more
 * than one hard link is refused.
 */
async function resolveFile(base: string, kind: FileKind, name: string): Promise<string | null> {
  if (!nameOk(kind, name)) return null;
  const spec = FILE_KINDS[kind];
  let realBase: string;
  let real: string;
  try {
    realBase = await realpath(base);
    real = await realpath(join(base, spec.dir, name + spec.ext));
    // A hard link to a member's token has the same real path rules as a
    // plain file, so only its link count gives it away. An archive copies a
    // hard link as a separate file with one link; that gap is accepted.
    const info = await stat(real);
    if (info.isFile() && info.nlink > 1) return null;
  } catch {
    return null;
  }
  if (!inside(realBase, real)) return null;
  if (relative(realBase, real).split(sep)[0] === "members") return null;
  return real;
}

/** Reads one file from a team or archive directory. */
export async function readKindFile(base: string, kind: string, name: string): Promise<FileResult> {
  if (!isFileKind(kind)) return { ok: false, problem: { code: "bad_request", detail: `unknown file kind` } };
  if (!nameOk(kind, name)) return { ok: false, problem: { code: "bad_request", detail: `bad ${kind} name` } };
  const real = await resolveFile(base, kind, name);
  if (!real) return notFound(`no ${kind} file named ${name}`);
  try {
    const info = await stat(real);
    if (!info.isFile()) return notFound(`no ${kind} file named ${name}`);
    const bytes = await Bun.file(real).slice(0, MAX_FILE_BYTES).arrayBuffer();
    const text = new TextDecoder().decode(bytes);
    return {
      ok: true,
      file: {
        kind,
        name,
        bytes: info.size,
        modifiedAt: info.mtime.toISOString(),
        text,
        truncated: info.size > MAX_FILE_BYTES,
        doc: FILE_KINDS[kind].ext === ".md" ? await parseMarkdownBounded(text) : null,
      },
    };
  } catch {
    return notFound(`no ${kind} file named ${name}`);
  }
}

/** Lists the files that exist, across every kind. */
export async function listFiles(base: string): Promise<FileRef[]> {
  const out: FileRef[] = [];
  for (const kind of FILE_KIND_NAMES) {
    const spec = FILE_KINDS[kind];
    let entries: string[];
    try {
      entries = await readdir(join(base, spec.dir));
    } catch {
      continue;
    }
    for (const entry of entries.sort()) {
      if (!entry.endsWith(spec.ext)) continue;
      const name = entry.slice(0, -spec.ext.length);
      const real = await resolveFile(base, kind, name);
      if (!real) continue;
      try {
        const info = await stat(real);
        if (info.isFile()) out.push({ kind, name, bytes: info.size, modifiedAt: info.mtime.toISOString() });
      } catch {
        // The file vanished between listing and stat.
      }
    }
  }
  return out;
}

/** The team's directory when the name is valid and its `team.db` exists. */
export async function teamDir(home: string, team: string): Promise<string | null> {
  if (!NAME_RE.test(team)) return null;
  const dir = join(home, "teams", team);
  return (await Bun.file(join(dir, "team.db")).exists()) ? dir : null;
}

// `-<n>` is how close names a second archive made in the same second.
const ARCHIVE_RE = /^([a-z][a-z0-9_]*)-(\d{4}-\d{2}-\d{2})(?:-(\d{6})(?:-(\d+))?)?$/;

export function parseArchiveName(dir: string): { team: string; date: string; time: string } | null {
  const m = ARCHIVE_RE.exec(dir);
  if (!m) return null;
  return { team: m[1]!, date: m[2]!, time: m[3] ?? "" };
}

/** The archive's directory when its name parses and it is a real directory under `archive/`. */
export async function archiveDir(home: string, dir: string): Promise<string | null> {
  if (!parseArchiveName(dir)) return null;
  const path = join(home, "archive", dir);
  try {
    const [realBase, real] = await Promise.all([realpath(join(home, "archive")), realpath(path)]);
    if (!inside(realBase, real) || relative(realBase, real).includes(sep)) return null;
    return (await stat(real)).isDirectory() ? path : null;
  } catch {
    return null;
  }
}

/** Archives, newest first by the date and time in the directory name. */
export async function listArchives(home: string): Promise<ArchiveEntry[]> {
  let names: string[];
  try {
    names = await readdir(join(home, "archive"));
  } catch {
    return [];
  }
  const parsed = names.flatMap((dir) => {
    const p = parseArchiveName(dir);
    return p ? [{ dir, ...p }] : [];
  });
  const suffix = (dir: string) => Number(/-\d{6}-(\d+)$/.exec(dir)?.[1] ?? 0);
  parsed.sort((a, b) => {
    const ka = a.date + a.time;
    const kb = b.date + b.time;
    return ka < kb ? 1 : ka > kb ? -1 : suffix(b.dir) - suffix(a.dir) || a.dir.localeCompare(b.dir);
  });
  const out: ArchiveEntry[] = [];
  for (const p of parsed) {
    const path = await archiveDir(home, p.dir);
    if (!path) continue;
    let closedAt: string | null = null;
    try {
      closedAt = (await stat(join(path, "SHA256SUMS"))).mtime.toISOString();
    } catch {
      // No checksum file: the archive was never sealed.
    }
    out.push({ dir: p.dir, team: p.team, date: p.date, closedAt, files: await listFiles(path) });
  }
  return out;
}
