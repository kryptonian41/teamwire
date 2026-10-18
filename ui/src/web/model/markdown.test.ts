import { describe, expect, test } from "bun:test";
import { cellAlign, headingTag, plainText, safeHref } from "./markdown.ts";

describe("safeHref", () => {
  test("keeps absolute web and mail links", () => {
    expect(safeHref("https://example.com/a?b=1")).toBe("https://example.com/a?b=1");
    expect(safeHref("http://example.com")).toBe("http://example.com/");
    expect(safeHref("mailto:a@example.com")).toBe("mailto:a@example.com");
  });

  test("refuses script, data and file URLs, relative paths and non-strings", () => {
    for (const href of ["javascript:alert(1)", " JavaScript:alert(1)", "data:text/html,<b>", "file:///etc/passwd", "vbscript:x"]) {
      expect(safeHref(href)).toBeNull();
    }
    for (const href of ["docs/plan.md", "/api/teams", "#top", "", 3, undefined]) expect(safeHref(href)).toBeNull();
  });
});

test("plainText joins nested text", () => {
  expect(plainText([{ t: "text", v: "a " }, { t: "strong", c: [{ t: "text", v: "b" }] }])).toBe("a b");
});

test("headingTag moves levels below the page's own headings", () => {
  expect([1, 2, 3, 4, 5, 6, "x"].map(headingTag)).toEqual(["h3", "h4", "h5", "h6", "h6", "h6", "h3"]);
});

test("cellAlign takes only the three Markdown values", () => {
  expect([cellAlign("right"), cellAlign("justify"), cellAlign(undefined)]).toEqual(["right", undefined, undefined]);
});
