import { useMemo } from "react";
import type { ApiClient } from "./api.ts";
import { ClientContext, useHash, useNow, useSnapshot } from "./hooks.ts";
import { bannerFor } from "./model/banner.ts";
import { joinAll } from "./model/teams.ts";
import { href, parseRoute } from "./model/route.ts";

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
        <Routes route={route} teams={teams} loaded={snapshot !== null} />
      </main>
    </ClientContext.Provider>
  );
}

import type { Route } from "./model/route.ts";
import type { TeamModel } from "./model/teams.ts";

function Routes(_: { route: Route; teams: TeamModel[]; loaded: boolean }) {
  return <p className="note">Views arrive in the next commits.</p>;
}
