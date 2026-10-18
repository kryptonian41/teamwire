// Parses Markdown into the MdNode tree the page renders. Bun.markdown only
// returns strings, so each callback stores its node in a table and returns a
// marker; a parent's children string is then read back as markers. Raw HTML
// stays text (noHtmlBlocks, noHtmlSpans), so an agent's file can never add
// markup to the page.

import type { MdNode } from "../shared/api.ts";

const MARKER = /\u0000(\d+)\u0001/g;
// Containers whose only real children are elements; the parser's whitespace
// between them would be invalid DOM (text inside <tr>, <ul>, ...).
const STRUCTURAL = new Set(["list", "table", "thead", "tbody", "tr"]);
/** Deeper trees fall back to plain text: rendering and JSON both recurse per level. */
export const MAX_MD_DEPTH = 64;
/** Larger files are not parsed: the tree's JSON is many times the text's size. */
export const MAX_MD_CHARS = 256 * 1024;
/** Some inputs, such as thousands of nested brackets, parse in quadratic time. */
export const MD_PARSE_TIMEOUT_MS = 1500;
const ELEMENTS = [
  "heading", "paragraph", "blockquote", "code", "list", "listItem", "hr", "table", "thead", "tbody", "tr", "th", "td",
  "html", "strong", "emphasis", "link", "image", "codespan", "strikethrough",
] as const;

export function parseMarkdown(text: string): MdNode[] | null {
  const nodes: MdNode[] = [];
  const keep = (node: MdNode) => `\u0000${nodes.push(node) - 1}\u0001`;
  const children = (s: string, structural = false): MdNode[] => {
    const out: MdNode[] = [];
    let at = 0;
    const stray = (v: string) => {
      if (v !== "" && !(structural && v.trim() === "")) out.push({ t: "text", v });
    };
    for (const match of s.matchAll(MARKER)) {
      stray(s.slice(at, match.index));
      const node = nodes[Number(match[1])];
      if (node) out.push(node);
      at = match.index + match[0].length;
    }
    stray(s.slice(at));
    return out;
  };
  const callbacks: Record<string, (c: string, meta?: object) => string> = {
    text: (v) => keep({ t: "text", v }),
  };
  for (const t of ELEMENTS) {
    callbacks[t] = (c, meta) => {
      const node: MdNode = { t, c: children(c, STRUCTURAL.has(t)) };
      const m = Object.fromEntries(
        Object.entries(meta ?? {}).filter(([, v]) => ["string", "number", "boolean"].includes(typeof v)),
      );
      if (Object.keys(m).length > 0) node.m = m;
      return keep(node);
    };
  }
  try {
    // The markers use two control characters, so the input must not carry them.
    const clean = text.replace(/[\u0000\u0001]/g, "");
    // Only URL autolinks: www and email autolinks carry no scheme, so the page could not link them.
    const doc = children(Bun.markdown.render(clean, callbacks, { noHtmlBlocks: true, noHtmlSpans: true, autolinks: { url: true } }), true);
    return depth(doc) > MAX_MD_DEPTH ? null : doc;
  } catch {
    return null;
  }
}

/** The tree's depth, counted without recursion so a deep tree cannot overflow the stack here. */
function depth(doc: MdNode[]): number {
  let max = 0;
  const stack: [MdNode[], number][] = [[doc, 1]];
  for (let item = stack.pop(); item; item = stack.pop()) {
    const [list, d] = item;
    if (list.length > 0) max = Math.max(max, d);
    for (const n of list) if (n.c) stack.push([n.c, d + 1]);
  }
  return max;
}

/**
 * `parseMarkdown` in a Worker, stopped after `MD_PARSE_TIMEOUT_MS`. The parse
 * is synchronous, so on the server's own thread one slow file would stall
 * every request. Null means the page shows the plain text.
 */
export function parseMarkdownBounded(text: string, timeoutMs = MD_PARSE_TIMEOUT_MS): Promise<MdNode[] | null> {
  if (text.length > MAX_MD_CHARS) return Promise.resolve(null);
  return new Promise((resolve) => {
    const worker = new Worker(new URL("./markdown-worker.ts", import.meta.url).href);
    const finish = (doc: MdNode[] | null) => {
      clearTimeout(timer);
      worker.terminate();
      resolve(doc);
    };
    const timer = setTimeout(() => finish(null), timeoutMs);
    worker.onmessage = (event: MessageEvent<MdNode[] | null>) => finish(event.data);
    worker.onerror = () => finish(null);
    worker.postMessage(text);
  });
}
