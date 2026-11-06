import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const css = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

// A browser lays these out; bun cannot. The test only pins the rules whose
// absence broke the page: a table's scroll box shrank to nothing inside the
// drawer's flex column, and the members table spilled out of its box.
test("drawer children do not shrink inside the flex column", () => {
  expect(css).toMatch(/\.drawer > \*\s*\{[^}]*flex-shrink:\s*0/);
});

test("the members table keeps its width and its one-line columns", () => {
  expect(css).toMatch(/\.members\s*\{[^}]*min-width:\s*1100px/);
  expect(css).toMatch(/\.members td:nth-child\(n\+2\)[^{]*\{[^}]*white-space:\s*nowrap/);
});

// The three places team uiplus will fill with buttons must take no space while empty,
// or the header, the drawer header and every question row carry a blank gap.
test("an empty action place takes no space", () => {
  expect(css).toMatch(/\.team-actions:empty,\s*\.member-actions:empty,\s*\.question-actions:empty\s*\{[^}]*display:\s*none/);
});
