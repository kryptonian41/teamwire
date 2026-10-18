// Rules for turning the server's Markdown tree into page elements. Kept apart
// from the React code so the security-relevant parts have plain unit tests.

import type { MdNode } from "../../shared/api.ts";

const SAFE_PROTOCOLS = new Set(["http:", "https:", "mailto:"]);

/**
 * The href a link may carry, or null. Only absolute http, https and mailto
 * URLs pass: a `javascript:` URL would run code, and a relative path from an
 * agent's file would point into this dashboard rather than at the file.
 */
export function safeHref(href: unknown): string | null {
  if (typeof href !== "string") return null;
  try {
    const url = new URL(href);
    return SAFE_PROTOCOLS.has(url.protocol) ? url.href : null;
  } catch {
    return null;
  }
}

/** The plain text under a node, for code blocks and image alt text. */
export function plainText(nodes: MdNode[] | undefined): string {
  return (nodes ?? []).map((n) => (n.t === "text" ? (n.v ?? "") : plainText(n.c))).join("");
}

/**
 * The heading tag for a Markdown heading level. A file sits under the page's
 * own h1 and h2, so its levels move down two and stop at h6.
 */
export function headingTag(level: unknown): "h3" | "h4" | "h5" | "h6" {
  const n = typeof level === "number" ? level : 1;
  return n <= 1 ? "h3" : n === 2 ? "h4" : n === 3 ? "h5" : "h6";
}

/** A table cell's alignment, only from the three values Markdown can set. */
export function cellAlign(align: unknown): "left" | "center" | "right" | undefined {
  return align === "left" || align === "center" || align === "right" ? align : undefined;
}
