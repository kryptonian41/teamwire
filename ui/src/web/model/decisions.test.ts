import { expect, test } from "bun:test";
import decisions from "../../../fixtures/decisions.json";
import { DecisionsView } from "../../shared/teamwire.ts";
import { decisionColumns, queryFromText, queryLabel } from "./decisions.ts";

const team = DecisionsView.parse(decisions).teams[0]!;

test("each member gets its own column, with a note when it has no list to show", () => {
  const cols = decisionColumns(team);
  expect(cols.map((c) => [c.role, c.note, c.blocks.length])).toEqual([
    ["api", null, 1],
    ["ledger", null, 2],
    ["web", "no report", 0],
    ["audit", "no Decisions I made block", 0],
  ]);
  expect(cols[0]!.blocks[0]).toContain("422");
});

test("search text is trimmed, and an empty or dash-led text makes no query", () => {
  expect(queryFromText("  rounding ")).toEqual({ grep: "rounding" });
  expect(queryFromText("")).toBeNull();
  expect(queryFromText("   ")).toBeNull();
  // The server refuses a leading dash, so the page says so before it asks.
  expect(queryFromText("-x")).toBeNull();
  expect(queryFromText("x".repeat(201))).toBeNull();
  expect(queryFromText("x".repeat(200))).toEqual({ grep: "x".repeat(200) });
});

test("a query is named in words", () => {
  expect(queryLabel({ grep: "rounding" })).toBe("text “rounding”");
  expect(queryLabel({ ref: "linear:GKA-580" })).toBe("ref linear:GKA-580");
});
