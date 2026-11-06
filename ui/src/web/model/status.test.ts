import { expect, test } from "bun:test";
import type { FileRef } from "../../shared/api.ts";
import { StatusAll, TeamsList } from "../../shared/teamwire.ts";
import status from "../../../fixtures/status.json";
import teams from "../../../fixtures/teams.json";
import { acceptsLine, afkChip, afkLine, afkLinks, answersLink, deliveryText, ownsLine, questionRows, refText, spanText, startedByLine, unansweredLines, unreadLines, watchChip, watchLine } from "./status.ts";

test("watch line for each state", () => {
  expect(watchLine({ state: "running", pid: 40211, heartbeat_age_s: 4 })).toBe("Watch running · pid 40211 · heartbeat 4 s ago");
  expect(watchLine({ state: "stale", pid: 40211, heartbeat_age_s: 720 })).toBe("Watch stale · last heartbeat 12 min ago");
  expect(watchLine({ state: "none", pid: null, heartbeat_age_s: null })).toBe("No watch running");
  expect(watchLine({ state: "running", pid: null, heartbeat_age_s: null })).toBe("Watch running");
});

test("spans read as seconds, minutes, hours", () => {
  expect([spanText(4), spanText(720), spanText(4320), spanText(3600)]).toEqual(["4 s", "12 min", "1 h 12 min", "1 h"]);
});

test("AFK line while running and when spent", () => {
  const run = { budget_ends_at: "2026-10-21T18:18:00Z", seconds_left: 4320, spent: false, decisions: 3 };
  expect(afkLine(run, "UTC")).toBe("AFK run · 1 h 12 min left · ends 18:18 · 3 decisions");
  expect(afkLine({ ...run, seconds_left: 0, spent: true }, "UTC")).toBe("AFK budget spent at 18:18 · 3 decisions");
  expect(afkLine({ ...run, decisions: 1 }, "UTC")).toContain("1 decision");
  expect(afkChip(run)).toBe("AFK 1 h 12 min left");
  expect(afkChip({ ...run, spent: true })).toBe("AFK budget spent");
});

test("unread and unanswered lists", () => {
  expect(unreadLines({ unread_deliveries: [{ display_id: "m002", to: "web", age_s: 720 }] })).toEqual([
    "m002 to web: handed over 12 min ago, not read yet",
  ]);
  expect(
    unansweredLines({
      unanswered_questions: [
        { display_id: "m006", from: "web", to: ["lead"], age_s: 180 },
        { display_id: "m007", from: "api", to: ["db@billing"], age_s: 45 },
      ],
    }),
  ).toEqual([
    "m006 question from web to lead: asked 3 min ago, no answer yet",
    "m007 question from api to db@billing: asked 45 s ago, no answer yet",
  ]);
});

test("started-by, owns and accepts lines", () => {
  expect(startedByLine({ started_by: "0.0.0+abc123", older_build: true })).toBe("Started by teamwire 0.0.0+abc123 (older than this teamwire)");
  expect(startedByLine({ started_by: "0.4.0", older_build: false })).toBe("Started by teamwire 0.4.0");
  expect(startedByLine({ started_by: null, older_build: false })).toBeNull();
  expect(ownsLine({ owns: ["src/refund/", "src/ledger/types.ts"] })).toBe("May change: src/refund/, src/ledger/types.ts");
  expect(ownsLine({ owns: [] })).toBe("May change: nothing (read-only)");
  expect(acceptsLine({ accepts_from_members: ["finding", "question"], can_be_typed_to: true })).toBe(
    "Accepts from members: finding, question · Lead can type to it: yes",
  );
  expect(acceptsLine({ accepts_from_members: [], can_be_typed_to: false })).toBe("Accepts from members: nothing · Lead can type to it: no");
});

test("delivery chip text adds reason and tries", () => {
  expect(deliveryText({ to: "api", state: "undelivered", attempts: 3, reason: "not_at_input_prompt" })).toBe("api undelivered · not_at_input_prompt · 3 tries");
  expect(deliveryText({ to: "api", state: "acked", attempts: 1, reason: null })).toBe("api acked");
  expect(deliveryText({ to: "api", state: "acked", attempts: 1 })).toBe("api acked");
  expect(deliveryText({ to: "emails", state: "acked", attempts: 2, reason: null })).toBe("emails acked · 2 tries");
});

test("answers link targets only a message in the table", () => {
  const ids = new Set(["m006"]);
  expect(answersLink({ re: "m006" }, ids)).toEqual({ text: "answers m006", target: "m006" });
  expect(answersLink({ re: "m001" }, ids)).toEqual({ text: "answers m001", target: null });
  expect(answersLink({ re: null }, ids)).toBeNull();
  expect(answersLink({}, ids)).toBeNull();
});

test("AFK links list only present files, in order", () => {
  const f = (kind: FileRef["kind"], name: string): FileRef => ({ kind, name, bytes: 1, modifiedAt: "" });
  expect(afkLinks([f("afk", "task"), f("afk", "decisions"), f("brief", "decisions")])).toEqual([
    { name: "decisions", label: "Decisions" },
    { name: "task", label: "Task" },
  ]);
  expect(afkLinks([])).toEqual([]);
});

test("only a missing watch gets a card chip", () => {
  expect(watchChip({ state: "none", pid: null, heartbeat_age_s: null })).toBe("no watch");
  expect(watchChip({ state: "running", pid: 1, heartbeat_age_s: 1 })).toBeNull();
  expect(watchChip({ state: "stale", pid: 1, heartbeat_age_s: 900 })).toBeNull();
});

test("a ref is plain text, with any id", () => {
  expect(refText({ provider: "linear", id: "GKA-580" })).toBe("linear:GKA-580");
  expect(refText({ provider: "github", id: "#3772" })).toBe("github:#3772");
});

test("the fixtures hold refs for the docs team in status and in the roster", () => {
  const texts = (refs: { provider: string; id: string }[]) => refs.map(refText);
  expect(texts(StatusAll.parse(status)[1]!.refs)).toEqual(["linear:GKA-580", "github:#3772"]);
  expect(texts(TeamsList.parse(teams).teams[1]!.refs)).toEqual(["linear:GKA-580", "github:#3772"]);
});

test("question rows carry the id, the parties, the age and the excerpt the status messages hold", () => {
  const messages = [{ display_id: "m006", time: "2026-10-13T07:12:09.120Z", type: "question" as const, from: "web", to: ["lead"], excerpt: "Partial refunds as a separate row?" }];
  expect(
    questionRows({
      messages,
      unanswered_questions: [
        { display_id: "m006", from: "web", to: ["lead"], age_s: 180 },
        { display_id: "m007", from: "api", to: ["db@billing", "lead@uiplus"], age_s: 45 },
      ],
    }),
  ).toEqual([
    { id: "m006", from: "web", to: "lead", recipients: ["lead"], age: "3 min", text: "Partial refunds as a separate row?", toLead: true },
    { id: "m007", from: "api", to: "db@billing, lead@uiplus", recipients: ["db@billing", "lead@uiplus"], age: "45 s", text: null, toLead: false },
  ]);
  // The payments fixture's one question, to the lead, carries its excerpt.
  const rows = questionRows(StatusAll.parse(status)[0]!);
  expect(rows.map((r) => [r.id, r.toLead, r.text !== null])).toEqual([["m006", true, true]]);
});
