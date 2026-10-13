import { expect, test } from "bun:test";
import type { ArchiveEntry } from "../../shared/api.ts";
import archives from "../../../fixtures/archives.json";
import { tabsFor } from "./files.ts";

const files = (archives as ArchiveEntry[])[0]!.files;

test("one tab per file kind that exists for the role, in display order", () => {
  expect(tabsFor(files, "impl")).toEqual([
    { kind: "brief", label: "Brief" },
    { kind: "outbox", label: "Result" },
    { kind: "report", label: "Report" },
    { kind: "verify", label: "Check log" },
  ]);
  expect(tabsFor(files, "review").map((t) => t.kind)).toEqual(["brief", "outbox"]);
});

test("comms files and other roles never make a tab", () => {
  expect(tabsFor(files, "m001")).toEqual([]);
  expect(tabsFor(files, "nobody")).toEqual([]);
});
