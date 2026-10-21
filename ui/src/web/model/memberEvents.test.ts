import { expect, test } from "bun:test";
import eventsFixture from "../../../fixtures/events.json";
import { EventsView, type EventRow } from "../../shared/teamwire.ts";
import { countText, modelText, removal, removalText, rolesIn, rowsForRole, tokensByModel, tokensText, verifyChip, verifyHistory } from "./memberEvents.ts";

const rows = EventsView.parse(eventsFixture).events;
const ev = (type: EventRow["type"], role: string, data: EventRow["data"] = {}, seq = 1): EventRow => ({
  seq, time: "2026-10-13T06:30:00.000Z", type, role, message: null, data,
});

test("model and effort come from the latest member.started", () => {
  expect(modelText(rows, "ledger")).toBe("sonnet · medium");
  expect(modelText(rows, "audit")).toBe("sonnet · high");
  expect(modelText(rows, "review")).toBe("opus · xhigh");
  expect(modelText(rows, "nobody")).toBeNull();
  const later = [...rows, ev("member.started", "ledger", { model: "opus" }, 99)];
  expect(modelText(later, "ledger")).toBe("opus");
  expect(modelText([ev("member.started", "x")], "x")).toBeNull();
});

test("verify chip: verdict and flag count, a dash when never run", () => {
  expect(verifyChip(rows, "ledger")).toEqual({ text: "FLAGGED · 1", category: "attention" });
  expect(verifyChip(rows, "api")).toEqual({ text: "FAIL", category: "failed" });
  expect(verifyChip(rows, "web")).toEqual({ text: "—", category: "idle" });
  const two = [ev("verify.recorded", "x", { verdict: "FLAGGED", flags: ["a", "b"] })];
  expect(verifyChip(two, "x").text).toBe("FLAGGED · 2");
  expect(verifyChip([ev("verify.recorded", "x", { verdict: "PASS", flags: [] })], "x")).toEqual({ text: "PASS", category: "done" });
  expect(verifyChip([ev("verify.recorded", "x")], "x").text).toBe("—");
});

test("verify history is newest first, with each run's waivers", () => {
  const h = verifyHistory(rows, "ledger");
  expect(h.map((r) => r.seq)).toEqual([40, 23]);
  expect(h[0]).toMatchObject({ verdict: "FLAGGED", exit: 0, seconds: 11, flags: ["outside-owns:src/shared/money.ts"] });
  expect(h[0]!.waivers).toEqual([
    { seq: 41, time: "2026-10-13T07:01:00.000Z", flags: ["outside-owns:src/shared/money.ts"], headSha: "4f1c2ab" },
  ]);
  expect(h[1]!.waivers).toEqual([]);
  expect(verifyHistory(rows, "web")).toEqual([]);
  // A waiver before any run is dropped, not attached to a later one.
  const early = [ev("verify.waived", "x", { flags: ["f"] }, 1), ev("verify.recorded", "x", { verdict: "PASS" }, 2)];
  expect(verifyHistory(early, "x")[0]!.waivers).toEqual([]);
});

test("tokens: output and total over every model, a dash without usage", () => {
  expect(tokensText(rows, "ledger")).toBe("70k out · 5.3M total");
  expect(tokensText(rows, "api")).toBe("—");
  expect(tokensText([ev("result.delivering", "x", { seconds: 1 })], "x")).toBe("—");
  const per = tokensByModel(rows, "ledger");
  expect(per.get("claude-sonnet-5-5")).toEqual({ input: 610, output: 70087, cacheRead: 4924611, cacheCreation: 291452 });
  const two = ev("result.delivering", "x", { usage: { a: { output: 5 }, b: { input: 2, output: "bad" } } });
  expect(tokensText([two], "x")).toBe("5 out · 7 total");
});

test("countText", () => {
  expect(countText(950)).toBe("950");
  expect(countText(1000)).toBe("1k");
  expect(countText(9500)).toBe("9.5k");
  expect(countText(70087)).toBe("70k");
  expect(countText(5_300_000)).toBe("5.3M");
  expect(countText(2_000_000)).toBe("2M");
});

test("removal reason", () => {
  expect(removal(rows, "oldapi")).toEqual({
    time: "2026-10-13T06:34:10.000Z",
    unverified: true,
    reasons: ["oldapi has no passing verify at HEAD 1a2b3c4: it never ran"],
  });
  expect(removal(rows, "ledger")).toBeNull();
  expect(removal([ev("member.removed", "x")], "x")).toMatchObject({ unverified: false, reasons: [] });
});

test("rowsForRole keeps only that member's rows in order", () => {
  expect(rowsForRole(rows, "fraud").map((r) => r.seq)).toEqual([13, 14, 43, 45, 46, 47]);
});

test("roles in order of first appearance, and the removal cell", () => {
  expect(rolesIn(rows)).toEqual(["oldapi", "ledger", "web", "emails", "audit", "fraud", "api", "review"]);
  expect(removalText(rows, "oldapi")).toBe("oldapi has no passing verify at HEAD 1a2b3c4: it never ran");
  expect(removalText(rows, "ledger")).toBe("—");
  expect(removalText([ev("member.removed", "x", { unverified: true })], "x")).toBe("removed without a passing verify");
  expect(removalText([ev("member.removed", "x")], "x")).toBe("removed");
});
