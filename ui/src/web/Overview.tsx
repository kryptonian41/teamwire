import { Attention } from "./Attention.tsx";
import { Empty, RefChips, StateChip } from "./components.tsx";
import { attentionItems, rankAttention, teamKey } from "./model/attention.ts";
import { membersByCategory, sortTeams, type TeamModel } from "./model/teams.ts";
import { href } from "./model/route.ts";
import { afkChip } from "./model/status.ts";

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
  const queue = rankAttention(attentionItems(sorted));
  const roles = new Map(sorted.flatMap((t) => (t.name === null ? [] : [[t.name, new Set(t.members.map((m) => m.role))] as const])));
  // The first row of a team in the ranked queue is its worst.
  const worst = new Map<string, string>();
  for (const i of queue) if (!worst.has(teamKey(i))) worst.set(teamKey(i), i.word);
  const n = queue.length;
  return (
    <>
      <div className="page-head">
        <h1>{n === 0 ? "Nothing needs you" : `${n} ${n === 1 ? "thing needs" : "things need"} you`}</h1>
        <p className="lede">
          Across {teams.length} {teams.length === 1 ? "team" : "teams"}.{n > 0 && " Worst first."}
        </p>
      </div>
      {n > 0 && <Attention items={queue} roles={roles} />}
      <section aria-labelledby="teams-h">
        <h2 id="teams-h">
          Teams <span className="count">{teams.length}</span>
        </h2>
        <ul className="teams">
          {sorted.map((t) => (
            <li key={t.key}>
              <TeamRow team={t} flag={worst.get(teamKey({ team: t.name, teamTitle: t.title })) ?? "Fine"} />
            </li>
          ))}
        </ul>
      </section>
    </>
  );
}

/** One row: the title and facts, the member strip worst first, and the team's worst queue word. */
function TeamRow({ team, flag }: { team: TeamModel; flag: string }) {
  const s = team.status;
  const n = team.members.length;
  const facts = [team.name ?? "no name", `${n} ${n === 1 ? "member" : "members"}`, s.watch.state === "none" ? "no watch" : `watch ${s.watch.state}`, s.afk ? afkChip(s.afk) : null].filter((f) => f !== null);
  const body = (
    <>
      <div className="tname">
        <h3>{team.title}</h3>
        <p className="sub">{facts.join(" · ")}</p>
        <RefChips refs={s.refs} />
      </div>
      <div className="strip">
        {n === 0 ? <span className="quiet">No members reported.</span> : membersByCategory(team.members).map((m) => <StateChip key={m.role} word={`${m.role} · ${m.state}`} category={m.category} />)}
      </div>
      <span className="tflag">{flag}</span>
    </>
  );
  // A team with no name has no detail page to open.
  return team.name === null ? <div className="trow">{body}</div> : <a className="trow" href={href.team(team.name)}>{body}</a>;
}
