import { CATEGORY_ORDER } from "../shared/state.ts";
import { StateChip } from "./components.tsx";
import { countByCategory, hasProblem, livenessInfo, sortTeams, type TeamModel } from "./model/teams.ts";
import { href } from "./model/route.ts";

export function Overview({ teams, loaded, neverPolled }: { teams: TeamModel[]; loaded: boolean; neverPolled: boolean }) {
  if (!loaded) return <p className="note">Waiting for the first poll…</p>;
  // An empty list from a failed poll says nothing about the home.
  if (neverPolled && teams.length === 0) return <p className="note">Waiting for teamwire: no poll has worked yet.</p>;
  if (teams.length === 0) return <p className="empty">No teams under this teamwire home.</p>;
  return (
    <>
      <h1>Teams</h1>
      <div className="cards">
        {sortTeams(teams).map((t) => (
          <TeamCard key={t.key} team={t} />
        ))}
      </div>
    </>
  );
}

function TeamCard({ team }: { team: TeamModel }) {
  const s = team.status;
  const counts = countByCategory(team.members);
  const live = livenessInfo(s.lead);
  const body = (
    <>
      <div className="card-head">
        <div>
          <h2>{team.title}</h2>
          <div className="sub">{team.name ?? "no name"}</div>
        </div>
        <div className="row" style={{ marginTop: 0 }}>
          <span className="chip small cat-idle">{s.state ?? "state unknown"}</span>
          <span className={`dot cat-${live.category}`} title={live.label} role="img" aria-label={live.label} />
        </div>
      </div>
      <div className="row">
        {team.members.map((m) => (
          <StateChip key={m.role} word={`${m.role} · ${m.state}`} category={m.category} />
        ))}
      </div>
      {team.members.length > 0 && (
        <div className="counts row">
          {CATEGORY_ORDER.filter((c) => counts[c] > 0).map((c) => (
            <span key={c} className={`chip small cat-${c}`}>
              {counts[c]} {c}
            </span>
          ))}
        </div>
      )}
      <div className="row">
        {s.stuck_deliveries.length > 0 && <span className="badge">{s.stuck_deliveries.length} stuck deliveries</span>}
        {s.undelivered_results.length > 0 && (
          <span className="badge bad">{s.undelivered_results.length} undelivered results</span>
        )}
        {s.errors.length > 0 && <span className="badge bad">{s.errors.length} read errors</span>}
      </div>
      {s.errors.length > 0 && (
        <ul className="errors">
          {s.errors.map((e, i) => (
            <li key={i}>{e}</li>
          ))}
        </ul>
      )}
    </>
  );
  const cls = `card${hasProblem(team) ? " problem" : ""}`;
  // A team with no name has no detail page to open.
  return team.name === null ? <div className={cls}>{body}</div> : <a className={cls} href={href.team(team.name)}>{body}</a>;
}
