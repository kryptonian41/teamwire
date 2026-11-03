import { expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { StatusAll, TeamsList } from "../shared/teamwire.ts";
import status from "../../fixtures/status.json";
import teams from "../../fixtures/teams.json";
import { joinAll } from "./model/teams.ts";
import { Overview } from "./Overview.tsx";

/** The HTML of one team's card: its link element. A card holds no other link. */
function card(html: string, team: string): string {
  const m = new RegExp(`<a class="team[^"]*" href="#/team/${team}">([\\s\\S]*?)</a>`).exec(html);
  if (m === null) throw new Error(`no card for ${team}`);
  return m[1]!;
}

function page(): string {
  return renderToStaticMarkup(
    createElement(Overview, { teams: joinAll(StatusAll.parse(status), TeamsList.parse(teams).teams), loaded: true, neverPolled: false }),
  );
}

test("probe: each reason sits on its own team's card only", () => {
  const html = page();
  const payments = card(html, "payments");
  const docs = card(html, "docs");
  const lost = card(html, "lost");
  expect(lost).not.toContain('class="flag">1 unread');
  for (const r of ["1 unread", "1 question waiting"]) {
    expect(payments).toContain(`>${r}<`);
    expect(docs).not.toContain(`>${r}<`);
  }
  expect(docs).toContain(">watch stale<");
  expect(payments).not.toContain(">watch stale<");
});
