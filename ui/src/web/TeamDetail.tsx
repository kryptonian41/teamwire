import { useState } from "react";
import type { Snapshot } from "../shared/api.ts";
import type { StateCategory } from "../shared/state.ts";
import type { LogMessage } from "../shared/teamwire.ts";
import { DeliveryChip, ErrorNote, Loading, StateChip } from "./components.tsx";
import { Drawer } from "./Drawer.tsx";
import { FileView } from "./FileView.tsx";
import { Flow, type Selected } from "./Flow.tsx";
import { useClient, useLoad } from "./hooks.ts";
import { buildFlow, filterByPair } from "./model/flow.ts";
import { href } from "./model/route.ts";
import { countByCategory, formatAge, leadCategory, livenessInfo, type TeamModel } from "./model/teams.ts";
import { CATEGORY_ORDER } from "../shared/state.ts";

export function TeamDetail({
  team,
  member,
  snapshot,
}: {
  team: TeamModel | undefined;
  member: string | null;
  snapshot: Snapshot | null;
}) {
  const client = useClient();
  const name = team?.name ?? "";
  const files = useLoad(() => client.teamFiles(name), name, snapshot?.at ?? "");
  if (!snapshot) return <p className="note">Waiting for the first poll…</p>;
  if (!team || team.name === null) {
    return (
      <>
        <p className="crumbs">
          <a href={href.overview()}>← Teams</a>
        </p>
        <p className="empty">No team with that name.</p>
      </>
    );
  }
  const s = team.status;
  const live = livenessInfo(s.lead);
  const open = member === null ? null : team.members.find((m) => m.role === member);
  const counts = countByCategory(team.members);

  return (
    <>
      <p className="crumbs">
        <a href={href.overview()}>← Teams</a>
      </p>
      <div className="page-head">
        <h1>{team.title}</h1>
        <span className="sub">{team.name}</span>
        <span className="chip small cat-idle">{s.state ?? "state unknown"}</span>
        <span className="row" style={{ margin: 0 }}>
          <span className={`dot cat-${live.category}`} aria-hidden="true" />
          <span className="sub">{live.label}</span>
        </span>
        {CATEGORY_ORDER.filter((c) => counts[c] > 0).map((c) => (
          <span key={c} className={`chip small cat-${c}`}>
            {counts[c]} {c}
          </span>
        ))}
      </div>
      {s.errors.length > 0 && (
        <ul className="errors">
          {s.errors.map((e, i) => (
            <li key={i}>{e}</li>
          ))}
        </ul>
      )}

      <h2>Members</h2>
      <MemberTable team={team} />

      {(s.stuck_deliveries.length > 0 || s.undelivered_results.length > 0) && (
        <>
          <h2>Needs a look</h2>
          {s.stuck_deliveries.length > 0 && (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Message</th>
                    <th>To</th>
                    <th>Delivery</th>
                    <th>Waiting</th>
                  </tr>
                </thead>
                <tbody>
                  {s.stuck_deliveries.map((d) => (
                    <tr key={`${d.display_id}>${d.to}`}>
                      <td>{d.display_id}</td>
                      <td>{d.to}</td>
                      <td>
                        <DeliveryChip to={d.to} state={d.state} />
                      </td>
                      <td>{d.age_s} s</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          {s.undelivered_results.length > 0 && (
            <ul>
              {s.undelivered_results.map((r) => (
                <li key={`${r.role}-${r.n}`}>
                  Result {r.n} from <strong>{r.role}</strong>: {r.report_state} (<code>{r.path}</code>)
                </li>
              ))}
            </ul>
          )}
        </>
      )}

      <MessageSection team={team} snapshotAt={snapshot.at} />

      {team.removed.length > 0 && (
        <details>
          <summary>Removed members ({team.removed.length})</summary>
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Role</th>
                  <th>Title</th>
                  <th>Kind</th>
                  <th>Generation</th>
                </tr>
              </thead>
              <tbody>
                {team.removed.map((m) => (
                  <tr key={m.role}>
                    <td>{m.role}</td>
                    <td>{m.title}</td>
                    <td>{m.kind}</td>
                    <td>{m.generation}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </details>
      )}

      {open && (
        <Drawer
          team={team.name}
          member={open}
          files={files.data ?? []}
          onClose={() => {
            location.hash = href.team(team.name ?? "");
          }}
        />
      )}
    </>
  );
}

function MemberTable({ team }: { team: TeamModel }) {
  if (team.members.length === 0) return <p className="empty">No members reported.</p>;
  const name = team.name ?? "";
  const go = (role: string) => {
    location.hash = href.member(name, role);
  };
  return (
    <div className="table-wrap">
      <table>
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
                </td>
                <td>{m.title}</td>
                <td>{m.kind ?? "?"}</td>
                <td>
                  <StateChip word={m.state} category={m.category} />
                </td>
                <td>{m.generation}</td>
                <td>
                  <div className={`bar${age.overdue ? " overdue" : ""}`} title={age.overdue ? "Past its deadline" : undefined}>
                    <span style={{ width: `${Math.round(age.ratio * 100)}%` }} />
                    <em>{age.label}</em>
                  </div>
                </td>
                <td>{m.result ?? "–"}</td>
                <td>{m.branch ? <code>{m.branch}</code> : "–"}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function MessageSection({ team, snapshotAt }: { team: TeamModel; snapshotAt: string }) {
  const client = useClient();
  const name = team.name ?? "";
  const log = useLoad(() => client.teamLog(name), name, snapshotAt);
  const [selected, setSelected] = useState<Selected | null>(null);

  const categories = new Map<string, StateCategory>(team.members.map((m) => [m.role, m.category]));
  categories.set("lead", leadCategory(team.status.lead));

  return (
    <>
      <h2>Message flow</h2>
      {log.loading && <Loading what="messages" />}
      {log.error && <ErrorNote>{log.error}</ErrorNote>}
      {log.data && (
        <>
          <Flow
            graph={buildFlow(log.data, team.members.map((m) => m.role))}
            categories={categories}
            selected={selected}
            onSelect={setSelected}
          />
          <h2>Messages</h2>
          {selected && (
            <div className="filter">
              <span>
                Showing {selected.from} → {selected.to}
              </span>
              <button className="btn" onClick={() => setSelected(null)}>
                Show all
              </button>
            </div>
          )}
          <MessageTable
            team={name}
            messages={selected ? filterByPair(log.data.messages, selected.from, selected.to) : log.data.messages}
          />
        </>
      )}
    </>
  );
}

function MessageTable({ team, messages }: { team: string; messages: LogMessage[] }) {
  const client = useClient();
  const [open, setOpen] = useState<string | null>(null);
  if (messages.length === 0) return <p className="empty">No messages.</p>;
  return (
    <div className="table-wrap">
      <table className="msgs">
        <thead>
          <tr>
            <th>Time</th>
            <th>Id</th>
            <th>Type</th>
            <th>From</th>
            <th>To</th>
            <th>Excerpt</th>
          </tr>
        </thead>
        <tbody>
          {messages.map((m) => (
            <tr key={m.display_id}>
              <td title={m.time}>{new Date(m.time).toLocaleTimeString()}</td>
              <td>{m.display_id}</td>
              <td>{m.type}</td>
              <td>{m.from}</td>
              <td>
                <div className="deliveries">
                  {m.to.map((to) => {
                    const d = m.deliveries.find((x) => x.to === to);
                    return d ? <DeliveryChip key={to} to={to} state={d.state} /> : <span key={to} className="chip small cat-idle">{to} · no delivery row</span>;
                  })}
                </div>
              </td>
              <td className="excerpt">
                <button className="link-btn" onClick={() => setOpen(open === m.display_id ? null : m.display_id)}>
                  {m.excerpt}
                </button>
                {open === m.display_id && (
                  <div className="fulltext">
                    <FileView id={`${team}/comms/${m.display_id}`} load={() => client.teamFile(team, "comms", m.display_id)} />
                  </div>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
