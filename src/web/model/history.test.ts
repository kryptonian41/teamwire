import { expect, test } from "bun:test";
import type { ArchiveEntry } from "../../shared/api.ts";
import archives from "../../../fixtures/archives.json";
import { groupArchives, groupFilesByRole, MESSAGES } from "./history.ts";

const A = archives as ArchiveEntry[];

test("groups by team with the newest archive first", () => {
  const g = groupArchives([
    { dir: "ads-2026-10-01", team: "ads", date: "2026-10-01", closedAt: "2026-10-01T10:00:00.000Z", files: [] },
    { dir: "zed-2026-09-01", team: "zed", date: "2026-09-01", closedAt: null, files: [] },
    { dir: "ads-2026-10-07", team: "ads", date: "2026-10-07", closedAt: null, files: [] },
    { dir: "ads-2026-10-07-153012", team: "ads", date: "2026-10-07", closedAt: "2026-10-07T15:30:12.000Z", files: [] },
  ]);
  expect(g.map((x) => x.team)).toEqual(["ads", "zed"]);
  expect(g[0]!.archives.map((a) => a.dir)).toEqual(["ads-2026-10-07-153012", "ads-2026-10-07", "ads-2026-10-01"]);
});

test("the fixture archives form one group", () => {
  expect(groupArchives(A).map((g) => g.team)).toEqual(["ads"]);
});

test("files group by role with messages last", () => {
  const g = groupFilesByRole(A[0]!.files);
  expect(g.map((x) => x.role)).toEqual(["impl", "review", MESSAGES]);
  expect(g[0]!.files.map((f) => f.kind)).toEqual(["brief", "outbox", "report", "verify"]);
});
