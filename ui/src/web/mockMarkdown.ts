// Mock mode runs in the browser, where Bun.markdown does not exist, so the
// sample file carries a tree written by hand. A server test checks that the
// real parser gives the same tree for the same text.

import type { MdNode } from "../shared/api.ts";

const t = (v: string): MdNode => ({ t: "text", v });

export function mockMarkdown(title: string, where: string): { text: string; doc: MdNode[] } {
  const text = [
    `# ${title}`,
    "",
    `This is **invented** text from mock mode (${where}). Raw HTML stays text: <b>not bold</b>.`,
    "",
    "## Units",
    "",
    "- [x] Parse the refund request",
    "- [ ] Write the ledger entry",
    "",
    "| Check | Result |",
    "| --- | --: |",
    "| `cargo test -p refunds` | 12 passed |",
    "",
    "```sh",
    "teamwire status payments",
    "```",
    "",
    "> Ask the lead before changing `schema.sql`. See https://example.com/refunds.",
    "",
  ].join("\n");
  const doc: MdNode[] = [
    { t: "heading", m: { level: 1 }, c: [t(title)] },
    {
      t: "paragraph",
      c: [
        t("This is "),
        { t: "strong", c: [t("invented")] },
        t(` text from mock mode (${where}). Raw HTML stays text: <b>not bold</b>.`),
      ],
    },
    { t: "heading", m: { level: 2 }, c: [t("Units")] },
    {
      t: "list",
      m: { ordered: false, depth: 0 },
      c: [
        { t: "listItem", m: { index: 0, depth: 0, ordered: false, checked: true }, c: [t("Parse the refund request")] },
        { t: "listItem", m: { index: 1, depth: 0, ordered: false, checked: false }, c: [t("Write the ledger entry")] },
      ],
    },
    {
      t: "table",
      c: [
        { t: "thead", c: [{ t: "tr", c: [{ t: "th", c: [t("Check")] }, { t: "th", m: { align: "right" }, c: [t("Result")] }] }] },
        {
          t: "tbody",
          c: [
            {
              t: "tr",
              c: [
                { t: "td", c: [{ t: "codespan", c: [t("cargo test -p refunds")] }] },
                { t: "td", m: { align: "right" }, c: [t("12 passed")] },
              ],
            },
          ],
        },
      ],
    },
    { t: "code", m: { language: "sh" }, c: [t("teamwire status payments"), t("\n")] },
    {
      t: "blockquote",
      c: [
        {
          t: "paragraph",
          c: [
            t("Ask the lead before changing "),
            { t: "codespan", c: [t("schema.sql")] },
            t(". See "),
            { t: "link", m: { href: "https://example.com/refunds" }, c: [t("https://example.com/refunds")] },
            t("."),
          ],
        },
      ],
    },
  ];
  return { text, doc };
}
