import { expect, test } from "bun:test";
import eventsFixture from "../../../fixtures/events.json";
import { EventsView, EventType, type EventRow } from "../../shared/teamwire.ts";
import { agoText, describeRows, isProblem, isRoutine, lastSeq, matchesFilter, mergeRows, oneLine, storeRestarted, visibleLines, type Filter } from "./timeline.ts";

const rows = EventsView.parse(eventsFixture).events;
const lines = describeRows(rows);
const line = (seq: number) => {
  const l = lines.find((x) => x.seq === seq);
  if (!l) throw new Error(`no row ${seq}`);
  return l;
};
const ev = (type: EventRow["type"], role: string | null = null, data: EventRow["data"] = {}, message: string | null = null, seq = 1): EventRow => ({
  seq, time: "2026-10-13T06:30:00.000Z", type, role, message, data,
});

test("each example in the label table", () => {
  const cases: [number, string, string | null][] = [
    [6, "ledger started", "sonnet · medium · took 7.8 s"],
    [35, "ledger could not stop yet", "commit 4f1c2ab has no progress line in the report"],
    [34, "audit's finding to review was refused", "recipient_not_allowed"],
    [44, "m005 not delivered to api", "not_at_input_prompt after 3 tries"],
    [33, "web reminded to read m002", "unread for 2 min"],
    [40, "ledger verify FLAGGED", "outside-owns:src/shared/money.ts · exit 0 · 11 s"],
    [41, "lead waived 1 flag on ledger", "outside-owns:src/shared/money.ts at 4f1c2ab"],
    [47, "fraud's result #1 not delivered", "session_blocked"],
    [48, "watch told the lead: UNDELIVERED, SILENT", null],
    [15, "oldapi removed without a passing verify", "oldapi has no passing verify at HEAD 1a2b3c4: it never ran"],
    [54, "team ticket refs added", "linear:GKA-581, github:#3772"],
    [55, "api got a fresh session", "generation 2 · 11111111 → 22222222"],
    [53, "close refused", "web is busy: its current turn is lost if its place closes. api's verify at HEAD 9d8e7f6 is FAIL, not PASS"],
  ];
  for (const [seq, label, details] of cases) {
    expect([line(seq).label, line(seq).details]).toEqual([label, details]);
  }
});

test("a result row names the completion before it", () => {
  expect(line(38).label).toBe("ledger's result #2 being delivered");
  expect(line(21).label).toBe("ledger's result #1 being delivered");
});

test("every fixture row has a label that is not its raw type", () => {
  for (const l of lines) expect(l.label).not.toBe(l.row.type);
});

test("a row with an empty data object still shows its type and member, with no details", () => {
  for (const type of EventType.options) {
    const [l] = describeRows([ev(type, "web")]);
    expect(l!.label.length).toBeGreaterThan(0);
    expect(l!.row.type).toBe(type);
    // Rows that need no key for their label may carry a details line only when data has keys.
    expect(l!.details).toBeNull();
  }
  const [l] = describeRows([ev("message.refused", "audit")]);
  expect(l!.label).toBe("message.refused · audit");
});

test("a wrongly typed key is ignored, not thrown on", () => {
  const [l] = describeRows([ev("member.held", "ledger", { gaps: "nope", unread: 3 })]);
  expect(l!.label).toBe("ledger could not stop yet");
  expect(l!.details).toBeNull();
});

test("problem and routine rules", () => {
  const problems = rows.filter(isProblem).map((r) => r.seq);
  expect(problems).toEqual([15, 34, 35, 40, 42, 44, 47, 48, 51, 53]); // 48 is a watch line with alerts
  expect(isProblem(ev("verify.recorded", "a", { verdict: "PASS" }))).toBe(false);
  expect(isProblem(ev("verify.recorded", "a"))).toBe(false);
  expect(isProblem(ev("member.removed", "a", { unverified: false }))).toBe(false);
  expect(isProblem(ev("watch.line", null, { events: ["WATCHING"] }))).toBe(false);
  for (const t of ["delivery.handed_off", "delivery.acked", "result.delivering", "result.delivered", "member.creating", "close.step"] as const) {
    expect(isRoutine(ev(t))).toBe(true);
  }
  expect(isRoutine(ev("member.started"))).toBe(false);
  expect(rows.filter((r) => isProblem(r) && isRoutine(r))).toEqual([]);
});

test("a watch line is a problem only when one of its words is an alert", () => {
  const watch = (...events: string[]) => isProblem(ev("watch.line", null, { events }));
  for (const calm of [["WATCHING"], ["WATCHING 3"], ["TEAM-COMPLETE"], ["COMPLETED"], ["WATCHING 2", "TEAM-COMPLETE"]]) {
    expect(watch(...calm)).toBe(false);
  }
  for (const word of ["UNDELIVERED", "SILENT", "DIED", "LATE", "TEAM-STALLED", "UNREAD", "UNCONFIRMED", "DECLARED"]) {
    expect(watch(word)).toBe(true);
    expect(watch("WATCHING", word)).toBe(true);
  }
  expect(watch()).toBe(false);
});

test("the Messages filter covers message, delivery and result rows only", () => {
  for (const t of ["message.sent", "delivery.undelivered", "result.delivered", "result.undelivered"] as const) {
    expect(matchesFilter(ev(t), "messages")).toBe(true);
  }
  for (const t of ["member.started", "verify.recorded", "watch.line", "close.refused"] as const) {
    expect(matchesFilter(ev(t), "messages")).toBe(false);
  }
  expect(visibleLines(lines, "messages", true).map((l) => l.seq)).toContain(47);
});

test("a fallback label drops the details even when data has keys", () => {
  // No sender and no member: the label cannot be built, so the reason must not show on its own.
  const [l] = describeRows([ev("message.refused", null, { reason: "recipient_not_allowed", type: "finding", to: ["review"] })]);
  expect(l!.label).toBe("message.refused");
  expect(l!.details).toBeNull();
});

test("the timeline starts over when the server's newest seq is below the newest row held", () => {
  const held = rows.slice(0, 10);
  expect(storeRestarted(held, 3)).toBe(true);
  expect(storeRestarted(held, 9)).toBe(true);
  expect(storeRestarted(held, 10)).toBe(false);
  expect(storeRestarted(held, 53)).toBe(false);
  expect(storeRestarted([], 0)).toBe(false);
});

test("filters pick rows and routine rows hide unless asked", () => {
  const seqs = (f: Filter, routine: boolean) => visibleLines(lines, f, routine).map((l) => l.seq);
  expect(seqs("problems", false)).toEqual([53, 51, 48, 47, 44, 42, 40, 35, 34, 15]);
  expect(seqs("verify", false)).toEqual([42, 41, 40, 23]);
  expect(seqs("watch", false)).toEqual([48, 2]);
  expect(seqs("all", false).length).toBe(rows.filter((r) => !isRoutine(r)).length);
  expect(seqs("all", true).length).toBe(rows.length);
  expect(seqs("messages", false)).not.toContain(25);
  expect(seqs("messages", true)).toContain(25);
  expect(seqs("members", false)).toContain(35);
  // Newest first.
  expect(seqs("all", true)[0]).toBe(55);
});

test("merging keeps held rows, adds new ones once, in order", () => {
  const a = rows.slice(0, 10);
  const merged = mergeRows(a, [...rows.slice(8, 14)]);
  expect(merged.map((r) => r.seq)).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]);
  expect(mergeRows(merged, [])).toEqual(merged);
  expect(lastSeq(merged)).toBe(14);
  expect(lastSeq([])).toBeUndefined();
});

test("oneLine and agoText", () => {
  expect(oneLine("first\nsecond")).toBe("first");
  expect(oneLine("x".repeat(200), 10)).toBe("xxxxxxxxx…");
  const t = "2026-10-13T06:30:00.000Z";
  const at = new Date(t).getTime();
  expect(agoText(t, at + 2000)).toBe("just now");
  expect(agoText(t, at - 5000)).toBe("just now");
  expect(agoText(t, at + 180_000)).toBe("3 min ago");
  expect(agoText(t, at + 8 * 86_400_000)).toBe("8 days ago");
});

test("a message the lead sent names the lead in the who column; other rows keep their role", () => {
  // seq 24, 28 and 52 are message.sent rows from the lead, with role null.
  expect([24, 28, 52].map((s) => line(s).role)).toEqual(["lead", "lead", "lead"]);
  expect(line(36).role).toBe("ledger");
  expect(line(51).role).toBe("lead");
  // A user's message and a row about no party stay without a who.
  expect(describeRows([ev("message.sent", null, { sender_kind: "user", recipients: ["web"] })])[0]!.role).toBeNull();
  expect(line(2).role).toBeNull();
});
