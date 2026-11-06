import type { EventRow } from "../shared/teamwire.ts";
import { MemberEventCells, StateChip } from "./components.tsx";
import { href } from "./model/route.ts";
import { startedByLine } from "./model/status.ts";
import { contextLabel, formatAge, pastHandoff, type TeamModel } from "./model/teams.ts";

export function MemberTable({ team, rows }: { team: TeamModel; rows: readonly EventRow[] }) {
  if (team.members.length === 0) return <p className="quiet">No members reported.</p>;
  const name = team.name ?? "";
  const go = (role: string) => {
    location.hash = href.member(name, role);
  };
  return (
    <div className="table-wrap">
      <table className="members">
        <thead>
          <tr>
            <th>Role</th>
            <th>Title</th>
            <th>Kind</th>
            <th>State</th>
            <th>Gen</th>
            <th>Age / deadline</th>
            <th>Result</th>
            <th>Branch</th>
            <th>Model</th>
            <th>Verify</th>
            <th>Tokens</th>
          </tr>
        </thead>
        <tbody>
          {team.members.map((m) => {
            const age = formatAge(m.age_s, m.deadline_s);
            return (
              <tr
                key={m.role}
                className="clickable"
                tabIndex={0}
                onClick={() => go(m.role)}
                onKeyDown={(e) => e.key === "Enter" && go(m.role)}
              >
                <td>
                  <a href={href.member(name, m.role)}>{m.role}</a>
                  {m.older_build && <span className="chip cat-attention badge" title={startedByLine(m) ?? undefined}>older build</span>}
                </td>
                <td className="title">{m.title}</td>
                <td>{m.kind ?? "?"}</td>
                <td>
                  <StateChip word={m.state} category={m.category} />
                  {contextLabel(m.context) && (
                    <span className="ctx" title={`context: ${m.context?.tokens} of ${m.context?.window} tokens`}>
                      {" "}
                      {contextLabel(m.context)}
                    </span>
                  )}
                  {pastHandoff(m.context) && (
                    <span className="flag badge" title={`context ${m.context?.tokens} is at or past the handoff point ${m.context?.handoff_at}`}>
                      past handoff
                    </span>
                  )}
                </td>
                <td>{m.generation}</td>
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
                <td>{m.branch ? <code>{m.branch}</code> : "–"}</td>
                <MemberEventCells rows={rows} role={m.role} />
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
