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
      <header className="top">
        <a className="brand" href={href.overview()}>
          teamwire
        </a>
        <nav>
          <a href={href.overview()} aria-current={route.page === "overview" ? "page" : undefined}>
            Teams
          </a>
          <a href={href.history()} aria-current={route.page === "history" || route.page === "archive" ? "page" : undefined}>
            History
          </a>
        </nav>
      </header>
      <div className={`banner banner-${banner.kind}`} role="status">
        <span>{banner.text}</span>
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
      <main>
        <Routes route={route} teams={teams} snapshot={snapshot} />
      </main>
    </ClientContext.Provider>
  );
}

function Routes({ route, teams, snapshot }: { route: Route; teams: TeamModel[]; snapshot: Snapshot | null }) {
  switch (route.page) {
    case "overview":
      return <Overview teams={teams} loaded={snapshot !== null} />;
    case "team":
      return <TeamDetail team={teams.find((t) => t.name === route.team)} member={route.member} snapshot={snapshot} />;
    case "history":
      return <History />;
    case "archive":
      return <Archive dir={route.dir} />;
    default:
      return <p className="empty">Nothing at {route.hash}. <a href={href.overview()}>Back to the teams</a>.</p>;
  }
}
