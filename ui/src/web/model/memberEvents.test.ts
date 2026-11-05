import { expect, test } from "bun:test";
import eventsFixture from "../../../fixtures/events.json";
import { EventsView, type EventRow } from "../../shared/teamwire.ts";
import { countText, handoffHistory, modelText, removal, removalText, rolesIn, rowsForRole, tokensByModel, tokensText, verifyChip, verifyHistory } from "./memberEvents.ts";

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
  // Each run keeps its exit code and the whole commit hash; a run without one has null.
  expect(h.map((r) => [r.exit, r.headSha])).toEqual([
    [0, "4f1c2ab9d8e7f6a5b4c3d2e1f00112233445566a"],
    [0, "0a1b2c3d4e5f60718293a4b5c6d7e8f901234567"],
  ]);
  expect(verifyHistory(rows, "api")[0]).toMatchObject({ verdict: "FAIL", exit: 1, headSha: "9d8e7f6a5b4c3d2e1f00112233445566778899aa" });
  expect(verifyHistory([ev("verify.recorded", "x", { verdict: "PASS" })], "x")[0]).toMatchObject({ exit: null, headSha: null });
  expect(verifyHistory(rows, "web")).toEqual([]);
  // A waiver before any run is dropped, not attached to a later one.
  const early = [ev("verify.waived", "x", { flags: ["f"] }, 1), ev("verify.recorded", "x", { verdict: "PASS" }, 2)];
  expect(verifyHistory(early, "x")[0]!.waivers).toEqual([]);
});

test("tokens: output and total over every model, a dash without usage", () => {
  // The fixture's two ledger rows share one session, so only the newer row counts.
  expect(tokensText(rows, "ledger")).toBe("22k out · 1.9M total");
  expect(tokensText(rows, "api")).toBe("—");
  expect(tokensText([ev("result.delivering", "x", { seconds: 1 })], "x")).toBe("—");
  const per = tokensByModel(rows, "ledger");
  expect(per.get("claude-sonnet-5-5")).toEqual({ input: 198, output: 21877, cacheRead: 1804211, cacheCreation: 90122 });
  const two = ev("result.delivering", "x", { usage: { a: { output: 5 }, b: { input: 2, output: "bad" } } });
  expect(tokensText([two], "x")).toBe("5 out · 7 total");
});

test("tokens: rows are running totals, so the newest row of each session counts and sessions add up", () => {
  // Real shape: three rows of one session that grow (planner, archive fixall-2026-10-20), then a second session.
  const row = (seq: number, session: string, output: number, model = "m") =>
    ev("result.delivering", "x", { session, usage: { [model]: { input: 1, output, cache_read: 10, cache_creation: 0 } } }, seq);
  const series = [row(1, "s1", 100), row(2, "s1", 150), row(3, "s1", 160), row(4, "s2", 40)];
  expect(tokensByModel(series, "x").get("m")).toEqual({ input: 2, output: 200, cacheRead: 20, cacheCreation: 0 });
  expect(tokensText(series, "x")).toBe("200 out · 222 total");
  // A row with no session id stands alone.
  const bare = ev("result.delivering", "x", { usage: { m: { output: 5 } } }, 5);
  expect(tokensByModel([...series, bare], "x").get("m")!.output).toBe(205);
});

test("verify: a PASS run's flags are waived; the FLAGGED run before it keeps the waivers", () => {
  const flags = ["outside-owns:a", "weakened:b"];
  const real = [
    ev("verify.recorded", "x", { verdict: "FLAGGED", flags, exit: 0, seconds: 5 }, 1),
    ev("verify.waived", "x", { flags: [flags[0]], head_sha: "aaaaaaa1" }, 2),
    ev("verify.waived", "x", { flags: [flags[1]], head_sha: "aaaaaaa1" }, 3),
    ev("verify.recorded", "x", { verdict: "PASS", flags, exit: 0, seconds: 5 }, 4),
  ];
  expect(verifyChip(real, "x")).toEqual({ text: "PASS · 2 waived", category: "done" });
  const [pass, flagged] = verifyHistory(real, "x");
  expect(pass).toMatchObject({ verdict: "PASS", flagsWaived: true, waivers: [] });
  expect(flagged).toMatchObject({ verdict: "FLAGGED", flagsWaived: false });
  expect(flagged!.waivers.length).toBe(2);
  // A PASS run with no flags and a FLAGGED run keep their plain chips.
  expect(verifyChip([ev("verify.recorded", "x", { verdict: "PASS", flags: [] })], "x").text).toBe("PASS");
  expect(verifyChip(real.slice(0, 1), "x").text).toBe("FLAGGED · 2");
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

test("handoff history lists the member's handoffs, newest first, with both sessions", () => {
  expect(handoffHistory(rows, "api")).toEqual([
    { seq: 55, time: "2026-10-13T07:17:30.000Z", generation: 2, oldSession: "11111111-1111-1111-1111-111111111111", newSession: "22222222-2222-2222-2222-222222222222" },
  ]);
  expect(handoffHistory(rows, "web")).toEqual([]);
  const more = [...rows, ev("member.handoff", "api", { generation: 3, old_session: "bbb", new_session: null }, 90)];
  expect(handoffHistory(more, "api").map((h) => [h.generation, h.oldSession, h.newSession])).toEqual([
    [3, "bbb", null],
    [2, "11111111-1111-1111-1111-111111111111", "22222222-2222-2222-2222-222222222222"],
  ]);
  // An older row with no keys still lists, with nulls.
  expect(handoffHistory([ev("member.handoff", "api")], "api")[0]).toMatchObject({ generation: null, oldSession: null, newSession: null });
});
