import { useMemo } from "react";
import type { ApiClient } from "./api.ts";
import { ClientContext, useHash, useNow, useSnapshot } from "./hooks.ts";
import { bannerFor } from "./model/banner.ts";
import { joinAll } from "./model/teams.ts";
import { href, parseRoute, type Route } from "./model/route.ts";
import type { TeamModel } from "./model/teams.ts";
import { Overview } from "./Overview.tsx";
import { Archive, History } from "./History.tsx";
import { TeamDetail } from "./TeamDetail.tsx";
import type { Snapshot } from "../shared/api.ts";

export function App({ client }: { client: ApiClient }) {
  const hash = useHash();
  const now = useNow();
  const { snapshot, connection } = useSnapshot(client);
  const route = parseRoute(hash);
  const banner = bannerFor(snapshot, connection, now);
  const teams = useMemo(() => (snapshot ? joinAll(snapshot.status, snapshot.teams) : []), [snapshot]);

  return (
    <ClientContext.Provider value={client}>
      <a
        className="skip"
        href="#main"
        onClick={(e) => {
          // The hash is the router, so jump by focus instead of changing it.
          e.preventDefault();
          document.getElementById("main")?.focus();
        }}
      >
        Skip to content
      </a>
      <header className="top">
        <div className="top-in">
          <a className="brand" href={href.overview()}>
            teamwire
          </a>
          <nav aria-label="Main">
            <a href={href.overview()} aria-current={current(route, "overview", "team")}>
              Teams
            </a>
            <a href={href.history()} aria-current={current(route, "history", "archive")}>
              History
            </a>
          </nav>
        </div>
      </header>
      <div className={`banner banner-${banner.kind}`} role="status">
        <div className="banner-in">
          <p>{banner.text}</p>
          {banner.problems.length > 0 && (
            <ul>
              {banner.problems.map((p, i) => (
                <li key={i}>
                  <code>{p.code}</code> {p.detail}
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
      <main id="main" tabIndex={-1}>
        <Routes route={route} teams={teams} snapshot={snapshot} />
      </main>
    </ClientContext.Provider>
  );
}

/** "page" on the section's own page, "true" on a page inside the section. */
function current(route: Route, page: Route["page"], inside: Route["page"]): "page" | "true" | undefined {
  if (route.page === page) return "page";
  return route.page === inside ? "true" : undefined;
}

function Routes({ route, teams, snapshot }: { route: Route; teams: TeamModel[]; snapshot: Snapshot | null }) {
  const liveTeams = new Set(snapshot?.status.flatMap((s) => (s.team === null ? [] : [s.team])));
  switch (route.page) {
    case "overview":
      return <Overview teams={teams} loaded={snapshot !== null} neverPolled={snapshot?.lastGoodAt === null} />;
    case "team":
      return <TeamDetail team={teams.find((t) => t.name === route.team)} member={route.member} snapshot={snapshot} />;
    case "history":
      return <History liveTeams={liveTeams} />;
    case "archive":
      return <Archive dir={route.dir} />;
    default:
      return <p className="empty">Nothing at {route.hash}. <a href={href.overview()}>Back to the teams</a>.</p>;
  }
}
