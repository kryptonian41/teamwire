import { expect, test } from "bun:test";
import decisions from "../../../fixtures/decisions.json";
import { DecisionsView } from "../../shared/teamwire.ts";
import { blockItems, decisionColumns, type Part, queryFromText, queryLabel } from "./decisions.ts";

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

const plain = (text: string): Part => ({ kind: "text", text });
const bold = (text: string): Part => ({ kind: "bold", text });
const code = (text: string): Part => ({ kind: "code", text });

test("a dash list gives one item per line", () => {
  expect(blockItems("- one\n* two\n  - three")).toEqual([[plain("one")], [plain("two")], [plain("three")]]);
});

test("a numbered list keeps a wrapped line inside its item, and empty lines go", () => {
  const text = "1. First line\n   wraps here\n\n2. Second\n10. Tenth";
  expect(blockItems(text)).toEqual([[plain("First line wraps here")], [plain("Second")], [plain("Tenth")]]);
});

test("a line that starts no item starts one when there is none before it", () => {
  expect(blockItems("Just a sentence.\nIt goes on.")).toEqual([[plain("Just a sentence. It goes on.")]]);
  expect(blockItems("")).toEqual([]);
  expect(blockItems("-no space\n- ")).toEqual([[plain("-no space")]]);
});

test("bold and code parts are cut out of the text around them", () => {
  expect(blockItems("- **Rounding** goes to `max(a, b)` always")).toEqual([
    [bold("Rounding"), plain(" goes to "), code("max(a, b)"), plain(" always")],
  ]);
  // The marker inside code is code text, not bold.
  expect(blockItems("- `a**b**c`")).toEqual([[code("a**b**c")]]);
});

test("a marker that is never closed stays plain text", () => {
  expect(blockItems("- an **open bold and `open code")).toEqual([[plain("an **open bold and `open code")]]);
  expect(blockItems("- **a** then **b")).toEqual([[bold("a"), plain(" then **b")]]);
  expect(blockItems("- ****")).toEqual([[plain("****")]]);
});

test("text with no markers is one plain part, and HTML is not parsed", () => {
  expect(blockItems("- <b>x</b> & y")).toEqual([[plain("<b>x</b> & y")]]);
});

test("each member counts the items of all its blocks, and says why it has none", () => {
  const cols = decisionColumns(team);
  expect(cols.map((c) => [c.role, c.count, c.empty])).toEqual([
    ["api", 2, null],
    ["ledger", 3, null],
    ["web", 0, "no report yet"],
    ["audit", 0, "no Decisions block"],
  ]);
  // Two blocks join into one list, in file order.
  expect(cols[1]!.items.map((i) => i[0]!.text)).toEqual([
    "A partial refund writes one REFUND_PARTIAL entry per capture.",
    "Amounts stay in minor units.",
    "Rounding goes to the larger side of the split.",
  ]);
});

test("a block that holds no item is reported as empty, not as missing", () => {
  const t = { ...team, members: [{ ...team.members[0]!, blocks: [{ heading: "## Decisions I made", line: 1, text: "\n\n" }] }] };
  expect(decisionColumns(t)[0]).toMatchObject({ count: 0, empty: "empty Decisions block", note: null });
});
