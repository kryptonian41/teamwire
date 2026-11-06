import { expect, test } from "bun:test";
import type { ArchiveEntry, FileRef } from "../../shared/api.ts";
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

test("the handoff document gets a tab after the report, only when it exists", () => {
  const f = (kind: FileRef["kind"], name: string): FileRef => ({ kind, name, bytes: 1, modifiedAt: "2026-10-07T12:00:00.000Z" });
  expect(tabsFor([f("handoff", "impl"), f("report", "impl"), f("brief", "impl")], "impl")).toEqual([
    { kind: "brief", label: "Brief" },
    { kind: "report", label: "Report" },
    { kind: "handoff", label: "Handoff" },
  ]);
  expect(tabsFor([f("report", "impl")], "impl").some((t) => t.kind === "handoff")).toBe(false);
  expect(tabsFor([f("handoff", "other")], "impl")).toEqual([]);
});
