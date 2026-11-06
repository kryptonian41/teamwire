import { attentionItems } from "./model/attention.ts";
import { href } from "./model/route.ts";
import type { TeamModel } from "./model/teams.ts";

/** What needs a look across all teams: one row each, naming the team and the member and linking to the team page. */
export function Attention({ teams }: { teams: readonly TeamModel[] }) {
  const items = attentionItems(teams);
  return (
    <section aria-labelledby="attention">
      <h2 id="attention">
        Needs attention <span className="count">{items.length}</span>
      </h2>
      {items.length === 0 ? (
        <p className="quiet">Nothing needs attention.</p>
      ) : (
        <ul className="attention">
          {items.map((i, n) => {
            const body = (
              <>
                <strong>{i.teamTitle}</strong>
                {i.member && <span className="who">{i.member}</span>}
                <span>{i.text}</span>
              </>
            );
            return <li key={n}>{i.team === null ? <span>{body}</span> : <a href={href.team(i.team)}>{body}</a>}</li>;
          })}
        </ul>
      )}
    </section>
  );
}
