import { describe, expect, test } from "bun:test";
import type { MdNode } from "../shared/api.ts";
import { MAX_MD_CHARS, MAX_MD_DEPTH, createBoundedParser, parseMarkdown, parseMarkdownBounded } from "./markdown.ts";

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

describe("limits", () => {
  test("a tree deeper than the limit falls back to plain text", () => {
    expect(parseMarkdown("> ".repeat(MAX_MD_DEPTH + 5) + "x")).toBeNull();
    expect(parseMarkdown("> ".repeat(10) + "x")).not.toBeNull();
  });

  test("www and email autolinks stay text, so no dead link is made", () => {
    const nodes = all(parseMarkdown("www.example.com and a@example.com") ?? []);
    expect(nodes.some((n) => n.t === "link")).toBe(false);
  });

  test("the bounded parse returns the tree off the server's thread", async () => {
    expect(await parseMarkdownBounded("**hi**")).toEqual([{ t: "paragraph", c: [{ t: "strong", c: [{ t: "text", v: "hi" }] }] }]);
  });

  test("a slow parse is stopped and gives null", async () => {
    const started = performance.now();
    expect(await parseMarkdownBounded("[".repeat(30000) + "]".repeat(30000), 200)).toBeNull();
    expect(performance.now() - started).toBeLessThan(1500);
  });

  test("a file over the size limit is not parsed", async () => {
    expect(await parseMarkdownBounded("x".repeat(MAX_MD_CHARS + 1))).toBeNull();
  });

  const counting = () => {
    const made = { n: 0 };
    const parse = createBoundedParser(() => {
      made.n++;
      return new Worker(new URL("./markdown-worker.ts", import.meta.url).href);
    });
    return { made, parse };
  };
  const slow = "[".repeat(30000) + "]".repeat(30000);

  test("ten parses in a row use one Worker", async () => {
    const { made, parse } = counting();
    for (let i = 0; i < 10; i++) expect(await parse(`**${i}**`)).not.toBeNull();
    expect(made.n).toBe(1);
  });

  test("a parse after a timeout starts a new Worker and still returns its tree", async () => {
    const { made, parse } = counting();
    expect(await parse(slow, 200)).toBeNull();
    expect(await parse("**hi**")).toEqual([{ t: "paragraph", c: [{ t: "strong", c: [{ t: "text", v: "hi" }] }] }]);
    expect(made.n).toBe(2);
  });

  test("a Worker that fails to start gives null once, and the next parse starts one", async () => {
    let calls = 0;
    const parse = createBoundedParser(() => {
      if (calls++ === 0) throw new Error("cannot start");
      return new Worker(new URL("./markdown-worker.ts", import.meta.url).href);
    });
    expect(await parse("**hi**")).toBeNull();
    expect(await parse("**hi**")).not.toBeNull();
    expect(calls).toBe(2);
  });

  test("parses sent together resolve in order with their own trees", async () => {
    const { parse } = counting();
    const out = await Promise.all(["# a", "# b", "# c"].map((t) => parse(t)));
    expect(out.map((doc) => doc?.[0]?.c?.[0])).toEqual([{ t: "text", v: "a" }, { t: "text", v: "b" }, { t: "text", v: "c" }]);
  });
});
