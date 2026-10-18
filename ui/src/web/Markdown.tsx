import type { ReactNode } from "react";
import type { MdNode } from "../shared/api.ts";
import { cellAlign, headingTag, plainText, safeHref } from "./model/markdown.ts";

/**
 * A parsed Markdown file as page elements. Every string reaches React as a
 * text child or a checked attribute, never as HTML.
 */
export function Markdown({ doc }: { doc: MdNode[] }) {
  return <div className="md">{nodes(doc)}</div>;
}

function nodes(list: MdNode[] | undefined): ReactNode[] {
  return (list ?? []).map((n, i) => node(n, i));
}

function node(n: MdNode, key: number): ReactNode {
  const kids = () => nodes(n.c);
  const m = n.m ?? {};
  switch (n.t) {
    case "text":
      return n.v ?? "";
    case "heading": {
      const Tag = headingTag(m.level);
      return <Tag key={key}>{kids()}</Tag>;
    }
    case "paragraph":
      return <p key={key}>{kids()}</p>;
    case "blockquote":
      return <blockquote key={key}>{kids()}</blockquote>;
    case "code":
      return (
        <pre key={key} className="text">
          <code>{plainText(n.c)}</code>
        </pre>
      );
    case "list":
      return m.ordered === true ? (
        <ol key={key} start={typeof m.start === "number" ? m.start : undefined}>
          {kids()}
        </ol>
      ) : (
        <ul key={key}>{kids()}</ul>
      );
    case "listItem":
      return typeof m.checked === "boolean" ? (
        <li key={key} className="task">
          <input type="checkbox" checked={m.checked} disabled aria-label={m.checked ? "done" : "not done"} /> {kids()}
        </li>
      ) : (
        <li key={key}>{kids()}</li>
      );
    case "hr":
      return <hr key={key} />; // taste-skill-ignore: a break in the file itself, drawn as a filled bar
    case "table":
      return (
        <div key={key} className="table-wrap">
          <table>{kids()}</table>
        </div>
      );
    case "thead":
      return <thead key={key}>{kids()}</thead>;
    case "tbody":
      return <tbody key={key}>{kids()}</tbody>;
    case "tr":
      return <tr key={key}>{kids()}</tr>;
    case "th":
      return (
        <th key={key} style={{ textAlign: cellAlign(m.align) }}>
          {kids()}
        </th>
      );
    case "td":
      return (
        <td key={key} style={{ textAlign: cellAlign(m.align) }}>
          {kids()}
        </td>
      );
    case "strong":
      return <strong key={key}>{kids()}</strong>;
    case "emphasis":
      return <em key={key}>{kids()}</em>;
    case "strikethrough":
      return <del key={key}>{kids()}</del>;
    case "codespan":
      return <code key={key}>{plainText(n.c)}</code>;
    case "link": {
      const href = safeHref(m.href);
      return href ? (
        <a key={key} href={href} target="_blank" rel="noopener noreferrer">
          {kids()}
        </a>
      ) : (
        <span key={key} className="md-dead" title={typeof m.href === "string" ? m.href : undefined}>
          {kids()}
        </span>
      );
    }
    case "image": {
      // Never loaded: the dashboard makes no request to another host.
      const href = safeHref(m.src);
      const label = `Image: ${plainText(n.c) || (typeof m.src === "string" ? m.src : "no description")}`;
      return href ? (
        <a key={key} href={href} target="_blank" rel="noopener noreferrer">
          {label}
        </a>
      ) : (
        <span key={key}>{label}</span>
      );
    }
    default:
      return <span key={key}>{kids()}</span>;
  }
}
