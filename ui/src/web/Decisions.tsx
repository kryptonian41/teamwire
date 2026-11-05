import { useState } from "react";
import type { DecisionQuery } from "../shared/api.ts";
import type { DecisionTeam } from "../shared/teamwire.ts";
import { ErrorNote, Loading, RefChips } from "./components.tsx";
import { useClient, useLoad } from "./hooks.ts";
import { decisionColumns, queryFromText, queryLabel } from "./model/decisions.ts";
import { href } from "./model/route.ts";

/**
 * The team's "Decisions I made" lists, one column per member, and a search over every team.
 * `query` is set by the search box or by a click on a ref chip in the page header.
 */
export function Decisions({
  team,
  refresh,
  query,
  onQuery,
}: {
  team: string;
  refresh: string;
  query: DecisionQuery | null;
  onQuery: (q: DecisionQuery | null) => void;
}) {
  const client = useClient();
  const own = useLoad(() => client.teamDecisions(team), team, refresh);
  const [text, setText] = useState("");
  const parsed = queryFromText(text);
  return (
    <section id="decisions" aria-labelledby="decisions-h">
      <h2 id="decisions-h">Decisions</h2>
      <form
        className="filter"
        role="search"
        onSubmit={(e) => {
          e.preventDefault();
          if (parsed) onQuery(parsed);
        }}
      >
        <input
          type="search"
          value={text}
          maxLength={200}
          placeholder="Search every team’s decisions"
          aria-label="Search decisions in every team"
          onChange={(e) => setText(e.target.value)}
        />
        <button className="btn" type="submit" disabled={parsed === null}>
          Search
        </button>
      </form>
      {query && <Results query={query} onClear={() => onQuery(null)} />}
      {own.loading && <Loading what="decisions" />}
      {own.error && <ErrorNote>{own.error}</ErrorNote>}
      {own.data && (
        <>
          {own.data.errors.length > 0 && <ErrorNote>{own.data.errors.join("; ")}</ErrorNote>}
          {own.data.teams.length === 0 ? <p className="quiet">No decisions found for this team.</p> : own.data.teams.map((t) => <Columns key={t.team} team={t} />)}
        </>
      )}
    </section>
  );
}

function Results({ query, onClear }: { query: DecisionQuery; onClear: () => void }) {
  const client = useClient();
  const key = "grep" in query ? `grep:${query.grep}` : `ref:${query.ref}`;
  const found = useLoad(() => client.searchDecisions(query), key);
  return (
    <div className="decision-results">
      <div className="filter" role="status">
        <span>
          Matches for <strong>{queryLabel(query)}</strong> in every team
        </span>
        <button className="btn" onClick={onClear}>
          Clear
        </button>
      </div>
      {found.loading && <Loading what="matches" />}
      {found.error && <ErrorNote>{found.error}</ErrorNote>}
      {found.data && (
        <>
          {found.data.errors.length > 0 && <ErrorNote>{found.data.errors.join("; ")}</ErrorNote>}
          {found.data.teams.length === 0 && <p className="quiet">No team matches.</p>}
          {found.data.teams.map((t) => (
            <div key={`${t.team}-${t.created_at}`}>
              <h3>
                {t.live ? <a href={href.team(t.team)}>{t.title}</a> : t.title} <span className="sub">{t.team}{t.live ? "" : " (archived)"}</span>
              </h3>
              <RefChips refs={t.refs} />
              <Columns team={t} />
            </div>
          ))}
        </>
      )}
    </div>
  );
}

/** One column per member. The columns scroll sideways when there are many. */
function Columns({ team }: { team: DecisionTeam }) {
  const cols = decisionColumns(team);
  if (cols.length === 0) return <p className="quiet">No members.</p>;
  return (
    <div className="decision-cols" style={{ gridTemplateColumns: `repeat(${cols.length}, minmax(240px, 1fr))` }}>
      {cols.map((c) => (
        <div key={c.role} className="decision-col">
          <h4>
            {c.role} <span className="sub">{c.kind}</span>
          </h4>
          {c.note && <p className="quiet">{c.note}</p>}
          {/* Report text is untrusted, so it is shown as plain text and never parsed. */}
          {c.blocks.map((b, i) => (
            <pre key={i} className="decision-text">
              {b}
            </pre>
          ))}
        </div>
      ))}
    </div>
  );
}
