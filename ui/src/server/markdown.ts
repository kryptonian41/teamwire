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
    return children(Bun.markdown.render(clean, callbacks, { noHtmlBlocks: true, noHtmlSpans: true, autolinks: true }), true);
  } catch {
    return null;
  }
}
