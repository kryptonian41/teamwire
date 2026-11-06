import type { EventRow } from "../shared/teamwire.ts";
import { MemberEventCells, StateChip } from "./components.tsx";
import { href, type Tab } from "./model/route.ts";
import { startedByLine } from "./model/status.ts";
import { contextLabel, formatAge, pastHandoff, type TeamModel } from "./model/teams.ts";

/** Seven columns, so the table fits the page at 1280 px. The branch is in the drawer. */
export function MemberTable({ team, tab, rows }: { team: TeamModel; tab: Tab; rows: readonly EventRow[] }) {
  if (team.members.length === 0) return <p className="quiet">No members reported.</p>;
  const name = team.name ?? "";
  const go = (role: string) => {
    location.hash = href.member(name, role, tab);
  };
  return (
    <div className="table-wrap">
      <table className="members">
        <thead>
          <tr>
            <th>Member</th>
            <th>State</th>
            <th>Age / deadline</th>
            <th>Result</th>
            <th>Verify</th>
            <th className="num">Tokens</th>
            <th>Model</th>
          </tr>
        </thead>
        <tbody>
          {team.members.map((m) => {
            const age = formatAge(m.age_s, m.deadline_s);
            const ctx = contextLabel(m.context);
            return (
              <tr
                key={m.role}
                className="clickable"
                tabIndex={0}
                onClick={() => go(m.role)}
                onKeyDown={(e) => e.key === "Enter" && go(m.role)}
              >
                <td className="member">
                  <a href={href.member(name, m.role, tab)}>{m.role}</a>
                  {m.older_build && <StateChip word="older build" category="attention" title={startedByLine(m) ?? undefined} />}
                  {pastHandoff(m.context) && (
                    <StateChip word="past handoff" category="attention" title={`context ${m.context?.tokens} is at or past the handoff point ${m.context?.handoff_at}`} />
                  )}
                  <small>
                    {[m.title, m.kind ?? "kind unknown", `gen ${m.generation}`, ctx === null ? null : `context ${ctx}`].filter((p) => p !== null).join(" · ")}
                  </small>
                </td>
                <td>
                  <StateChip word={m.state} category={m.category} />
                </td>
                <td>
                  <div className={`age${age.overdue ? " overdue" : ""}`} title={age.overdue ? "Past its deadline" : undefined}>
                    <span className="bar" aria-hidden="true">
                      <span style={{ width: `${Math.round(age.ratio * 100)}%` }} />
                    </span>
                    <span>
                      {age.label}
                      {age.overdue && <span className="visually-hidden"> past its deadline</span>}
                    </span>
                  </div>
                </td>
                <td>{m.result ?? "–"}</td>
                <MemberEventCells rows={rows} role={m.role} />
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
