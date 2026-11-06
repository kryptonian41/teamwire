import { StateChip } from "./components.tsx";
import { teamKey, type AttentionItem } from "./model/attention.ts";
import { href } from "./model/route.ts";

/**
 * The ranked queue: one row per thing that needs a person, grouped under each team's title. A row
 * about a member of its team opens the member's drawer; any other row opens the team page.
 */
export function Attention({ items, roles }: { items: readonly AttentionItem[]; roles: ReadonlyMap<string, ReadonlySet<string>> }) {
  const groups: { key: string; title: string; team: string | null; rows: AttentionItem[] }[] = [];
  for (const i of items) {
    const key = teamKey(i);
    const last = groups[groups.length - 1];
    if (last && last.key === key) last.rows.push(i);
    else groups.push({ key, title: i.teamTitle, team: i.team, rows: [i] });
  }
  const link = (i: AttentionItem): string | null => {
    if (i.team === null) return null;
    return i.member !== null && roles.get(i.team)?.has(i.member) ? href.member(i.team, i.member) : href.team(i.team);
  };
  return (
    <section aria-labelledby="queue-h">
      <h2 id="queue-h">
        Needs you <span className="count">{items.length}</span>
      </h2>
      <ol className="queue">
        {groups.map((g) => (
          <li key={g.key}>
            <h3>
              {g.title} <span className="sub">{g.team ?? "no name"}</span>
            </h3>
            <ul>
              {g.rows.map((i, n) => {
                const to = link(i);
                const body = (
                  <>
                    <StateChip word={i.word} category={i.category} />
                    <span className="role">{i.member ?? "—"}</span>
                    <span className="qtext">{i.text}</span>
                  </>
                );
                return <li key={n}>{to === null ? <span className="qrow">{body}</span> : <a className="qrow" href={to}>{body}</a>}</li>;
              })}
            </ul>
          </li>
        ))}
      </ol>
    </section>
  );
}
