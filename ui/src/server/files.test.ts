import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { FILE_KINDS, FILE_KIND_NAMES, MAX_FILE_BYTES } from "../shared/api.ts";
import { archiveDir, listArchives, listFiles, parseArchiveName, readKindFile, teamDir } from "./files.ts";

let home: string;
let team: string;

beforeAll(() => {
  home = mkdtempSync(join(tmpdir(), "twui-files-"));
  team = join(home, "teams", "alpha");
  for (const kind of FILE_KIND_NAMES) {
    const spec = FILE_KINDS[kind];
    mkdirSync(join(team, spec.dir), { recursive: true });
    const name = spec.name === "display_id" ? "m001" : "impl";
    writeFileSync(join(team, spec.dir, name + spec.ext), `${kind} text`);
  }
  writeFileSync(join(team, "team.db"), "");
  mkdirSync(join(team, "members", "impl"), { recursive: true });
  writeFileSync(join(team, "members", "impl", "token"), "SECRET");
  writeFileSync(join(home, "outside.md"), "outside");
  symlinkSync(join(home, "outside.md"), join(team, "briefs", "evil.md"));
  symlinkSync(join(team, "members", "impl", "token"), join(team, "reports", "tok.md"));
  writeFileSync(join(team, "reports", "big.md"), "x".repeat(MAX_FILE_BYTES + 10));

  const arch = join(home, "archive");
  for (const d of ["ads-2026-10-07", "ads-2026-10-07-153012", "zed-2026-10-09", "bad-name", "Upper-2026-10-07", "ads-2026-10-07-1530"]) {
    mkdirSync(join(arch, d, "briefs"), { recursive: true });
  }
  writeFileSync(join(arch, "ads-2026-10-07-153012", "briefs", "impl.md"), "hi");
  writeFileSync(join(arch, "ads-2026-10-07-153012", "SHA256SUMS"), "");
  utimesSync(join(arch, "ads-2026-10-07-153012", "SHA256SUMS"), new Date("2026-10-07T15:30:12Z"), new Date("2026-10-07T15:30:12Z"));
  writeFileSync(join(arch, "stray-file-2026-10-07"), "");
});
afterAll(() => rmSync(home, { recursive: true, force: true }));

describe("readKindFile", () => {
  test("every kind resolves", async () => {
    for (const kind of FILE_KIND_NAMES) {
      const name = FILE_KINDS[kind].name === "display_id" ? "m001" : "impl";
      const r = await readKindFile(team, kind, name);
      expect(r.ok && r.file.text).toBe(`${kind} text`);
    }
  });

  test("refuses traversal and absolute names", async () => {
    for (const name of ["../team", "../../outside", "/etc/passwd", "..", "a/b", "impl.md", "Impl", ""]) {
      const r = await readKindFile(team, "brief", name);
      expect(r.ok).toBe(false);
    }
    expect((await readKindFile(team, "comms", "impl")).ok).toBe(false);
  });

  test("rejects an unknown kind, including prototype keys", async () => {
    for (const kind of ["members", "toString", "__proto__", "constructor"]) {
      const r = await readKindFile(team, kind, "impl");
      expect(r.ok).toBe(false);
      if (!r.ok) expect(r.problem.code).toBe("bad_request");
    }
  });

  test("refuses a symlink that leaves the team directory", async () => {
    const r = await readKindFile(team, "brief", "evil");
    expect(r).toMatchObject({ ok: false, problem: { code: "not_found" } });
  });

  test("refuses a symlink into members/", async () => {
    const r = await readKindFile(team, "report", "tok");
    expect(r.ok).toBe(false);
    expect(JSON.stringify(await listFiles(team))).not.toContain("tok");
  });

  test("a missing file is not_found", async () => {
    expect(await readKindFile(team, "brief", "nobody")).toMatchObject({ ok: false, problem: { code: "not_found" } });
  });

  test("truncates a file over the limit", async () => {
    const r = await readKindFile(team, "report", "big");
    expect(r.ok && r.file.truncated).toBe(true);
    expect(r.ok && r.file.text.length).toBe(MAX_FILE_BYTES);
    expect(r.ok && r.file.bytes).toBe(MAX_FILE_BYTES + 10);
  });

  test("a Markdown kind carries its parsed tree and a log does not", async () => {
    const md = await readKindFile(team, "brief", "impl");
    expect(md.ok && md.file.doc).toEqual([{ t: "paragraph", c: [{ t: "text", v: "brief text" }] }]);
    const log = await readKindFile(team, "verify", "impl");
    expect(log.ok && log.file.doc).toBeNull();
  });

  test("a small file is not truncated", async () => {
    const r = await readKindFile(team, "brief", "impl");
    expect(r.ok && r.file.truncated).toBe(false);
  });
});

describe("listFiles / teamDir", () => {
  test("lists existing files of every kind and nothing from members/", async () => {
    const files = await listFiles(team);
    expect(new Set(files.map((f) => f.kind))).toEqual(new Set(FILE_KIND_NAMES));
    expect(files.some((f) => f.name === "evil" || f.name === "tok")).toBe(false);
  });

  test("teamDir needs a valid name and team.db", async () => {
    expect(await teamDir(home, "alpha")).toBe(team);
    expect(await teamDir(home, "nope")).toBeNull();
    expect(await teamDir(home, "../teams/alpha")).toBeNull();
    expect(await teamDir(home, "Alpha")).toBeNull();
  });
});

describe("archives", () => {
  test("parseArchiveName", () => {
    expect(parseArchiveName("ads-2026-10-07")).toEqual({ team: "ads", date: "2026-10-07", time: "" });
    expect(parseArchiveName("ads_2-2026-10-07-153012")).toEqual({ team: "ads_2", date: "2026-10-07", time: "153012" });
    for (const bad of ["bad-name", "Upper-2026-10-07", "ads-2026-10-07-1530", "-2026-10-07", "ads-2026-10-07/x", "1ads-2026-10-07"]) {
      expect(parseArchiveName(bad)).toBeNull();
    }
  });

  test("lists valid archives newest first and skips bad ones", async () => {
    const list = await listArchives(home);
    expect(list.map((a) => a.dir)).toEqual(["zed-2026-10-09", "ads-2026-10-07-153012", "ads-2026-10-07"]);
    const sealed = list[1]!;
    expect(sealed.closedAt).toBe("2026-10-07T15:30:12.000Z");
    expect(sealed.files).toEqual([expect.objectContaining({ kind: "brief", name: "impl", bytes: 2 })]);
    expect(list[2]!.closedAt).toBeNull();
  });

  test("archiveDir refuses traversal and unknown names", async () => {
    expect(await archiveDir(home, "ads-2026-10-07")).toBe(join(home, "archive", "ads-2026-10-07"));
    expect(await archiveDir(home, "../teams")).toBeNull();
    expect(await archiveDir(home, "stray-file-2026-10-07")).toBeNull();
    expect(await archiveDir(home, "nope-2026-01-01")).toBeNull();
  });

  test("a missing archive directory lists nothing", async () => {
    expect(await listArchives(join(home, "teams"))).toEqual([]);
  });
});
