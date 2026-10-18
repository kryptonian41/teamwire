import { CATEGORY_ORDER } from "../shared/state.ts";
import { Empty, StateChip } from "./components.tsx";
import { countByCategory, hasProblem, livenessInfo, sortTeams, type TeamModel } from "./model/teams.ts";
import { href } from "./model/route.ts";

export function Overview({ teams, loaded, neverPolled }: { teams: TeamModel[]; loaded: boolean; neverPolled: boolean }) {
  if (!loaded) return <Empty title="Waiting for the first poll…" />;
  // An empty list from a failed poll says nothing about the home.
  if (neverPolled && teams.length === 0) {
    return <Empty title="Waiting for teamwire">No poll has worked yet. The banner above says why.</Empty>;
  }
  if (teams.length === 0) {
    return <Empty title="No teams under this teamwire home">A team shows up here once its lead launches it.</Empty>;
  }
  const sorted = sortTeams(teams);
  const trouble = sorted.filter(hasProblem);
  const fine = sorted.filter((t) => !hasProblem(t));
  return (
    <>
      <div className="page-head">
        <h1>Teams</h1>
        <p className="lede">
          {teams.length} {teams.length === 1 ? "team" : "teams"}.{" "}
          {trouble.length === 0 ? "None needs a look." : `${trouble.length} ${trouble.length === 1 ? "needs" : "need"} a look.`}
        </p>
      </div>
      {trouble.length > 0 && <TeamGroup id="trouble" title="Needs a look" teams={trouble} />}
      {fine.length > 0 && <TeamGroup id="fine" title="Running fine" teams={fine} />}
    </>
  );
}

function TeamGroup({ id, title, teams }: { id: string; title: string; teams: TeamModel[] }) {
  return (
    <section aria-labelledby={id}>
      <h2 id={id}>
        {title} <span className="count">{teams.length}</span>
      </h2>
      <ul className="teams">
        {teams.map((t) => (
          <li key={t.key}>
            <TeamRow team={t} />
          </li>
        ))}
      </ul>
    </section>
  );
}

function TeamRow({ team }: { team: TeamModel }) {
  const s = team.status;
  const counts = countByCategory(team.members);
  const live = livenessInfo(s.lead);
  const problem = hasProblem(team);
  const body = (
    <>
      <div className="team-id">
        <h3>{team.title}</h3>
        <p className="meta">
          <span>{team.name ?? "no name"}</span>
          <span className="chip cat-idle">{s.state ?? "state unknown"}</span>
          <span className="live">
            <span className={`dot cat-${live.category}`} aria-hidden="true" />
            {live.label}
          </span>
        </p>
        {team.members.length > 0 && (
          <p className="chips">
            {CATEGORY_ORDER.filter((c) => counts[c] > 0).map((c) => (
              <span key={c} className={`chip cat-${c}`}>
                {counts[c]} {c}
              </span>
            ))}
          </p>
        )}
      </div>
      <div className="team-body">
        {team.members.length === 0 ? (
          <p className="quiet">No members reported.</p>
        ) : (
          <p className="chips">
            {team.members.map((m) => (
              <StateChip key={m.role} word={`${m.role} · ${m.state}`} category={m.category} />
            ))}
          </p>
        )}
        {(s.stuck_deliveries.length > 0 || s.undelivered_results.length > 0 || s.errors.length > 0) && (
          <p className="chips">
            {s.stuck_deliveries.length > 0 && <span className="flag">{s.stuck_deliveries.length} stuck deliveries</span>}
            {s.undelivered_results.length > 0 && (
              <span className="flag bad">{s.undelivered_results.length} undelivered results</span>
            )}
            {s.errors.length > 0 && <span className="flag bad">{s.errors.length} read errors</span>}
          </p>
        )}
        {s.errors.length > 0 && (
          <ul className="errors">
            {s.errors.map((e, i) => (
              <li key={i}>{e}</li>
            ))}
          </ul>
        )}
      </div>
    </>
  );
  const cls = `team${problem ? " problem" : ""}`;
  // A team with no name has no detail page to open.
  return team.name === null ? <div className={cls}>{body}</div> : <a className={cls} href={href.team(team.name)}>{body}</a>;
}
