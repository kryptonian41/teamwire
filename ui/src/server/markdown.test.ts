import { describe, expect, test } from "bun:test";
import type { MdNode } from "../shared/api.ts";
import { parseMarkdown } from "./markdown.ts";

const all = (nodes: MdNode[]): MdNode[] => nodes.flatMap((n) => [n, ...all(n.c ?? [])]);

describe("parseMarkdown", () => {
  test("builds a tree with each node's metadata", () => {
    expect(parseMarkdown("## Plan *now*")).toEqual([
      { t: "heading", m: { level: 2 }, c: [{ t: "text", v: "Plan " }, { t: "emphasis", c: [{ t: "text", v: "now" }] }] },
    ]);
  });

  test("raw HTML stays text and no html node is made", () => {
    const doc = parseMarkdown("<script>alert(1)</script>\n\nhi <img src=x onerror=alert(1)>") ?? [];
    const nodes = all(doc);
    expect(nodes.some((n) => n.t === "html")).toBe(false);
    expect(nodes.filter((n) => n.t === "text").map((n) => n.v).join("")).toContain("<script>alert(1)</script>");
  });

  test("links keep their target for the page to check, and bare URLs become links", () => {
    const links = all(parseMarkdown("[x](javascript:alert(1)) see https://a.example/b") ?? []).filter((n) => n.t === "link");
    expect(links.map((n) => n.m?.href)).toEqual(["javascript:alert(1)", "https://a.example/b"]);
  });

  test("tables and lists hold no whitespace text between their rows and items", () => {
    const doc = parseMarkdown("| a | b |\n|---|--:|\n| 1 | 2 |\n\n- [x] one\n- two") ?? [];
    for (const n of all(doc).filter((n) => ["table", "thead", "tbody", "tr", "list"].includes(n.t))) {
      expect(n.c?.every((c) => c.t !== "text")).toBe(true);
    }
    expect(all(doc).find((n) => n.t === "listItem")?.m?.checked).toBe(true);
    expect(all(doc).filter((n) => n.t === "td").at(-1)?.m?.align).toBe("right");
  });

  test("marker characters in the input cannot point at other nodes", () => {
    const doc = parseMarkdown("a \u00000\u0001 b") ?? [];
    expect(all(doc).filter((n) => n.t === "text").map((n) => n.v).join("")).toBe("a 0 b");
  });

  test("a code block keeps its language and exact text", () => {
    const [code] = parseMarkdown("```rust\nfn x<T>() {}\n```") ?? [];
    expect(code?.m).toEqual({ language: "rust" });
    expect(code?.c?.map((n) => n.v).join("")).toBe("fn x<T>() {}\n");
  });
});

test("the mock page's hand-written tree matches the parser", async () => {
  const { mockMarkdown } = await import("../web/mockMarkdown.ts");
  const { text, doc } = mockMarkdown("brief for api", "payments");
  expect(parseMarkdown(text)).toEqual(doc);
});
