import { useEffect, useState } from "react";
import type { StateCategory } from "../shared/state.ts";
import type { LogMessage } from "../shared/teamwire.ts";
import { DeliveryChip, ErrorNote, Loading } from "./components.tsx";
import { FileView } from "./FileView.tsx";
import { Flow, type Selected } from "./Flow.tsx";
import { REFRESH_MS, useClient, useLoad, useNow } from "./hooks.ts";
import { buildFlow, filterByPair } from "./model/flow.ts";
import { answersLink } from "./model/status.ts";
import { leadCategory, refreshKey, type TeamModel } from "./model/teams.ts";

export function MessageSection({ team, pick }: { team: TeamModel; pick: { id: string; n: number } | null }) {
  const client = useClient();
  const name = team.name ?? "";
  const tick = useNow(REFRESH_MS);
  const log = useLoad(() => client.teamLog(name), name, `${refreshKey(team.status)}|${tick}`);
  const [selected, setSelected] = useState<Selected | null>(null);
  const [picked, setPicked] = useState<string | null>(null);
  const shown = log.data && (selected ? filterByPair(log.data.messages, selected.from, selected.to) : log.data.messages);

  // A timeline row asked for a message. The pair filter may hide it, so clear the filter first; the effect runs again then.
  useEffect(() => {
    if (!pick || !log.data) return;
    if (shown?.some((m) => m.display_id === pick.id)) {
      setPicked(pick.id);
      document.getElementById(`msg-${pick.id}`)?.scrollIntoView({ block: "center" });
    } else if (log.data.messages.some((m) => m.display_id === pick.id)) {
      setSelected(null);
    }
  }, [pick, log.data, selected]);

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
            <div className="filter" role="status">
              <span>
                Showing <strong>{selected.from}</strong> → <strong>{selected.to}</strong>
              </span>
              <button className="btn" onClick={() => setSelected(null)}>
                Show all
              </button>
            </div>
          )}
          <MessageTable team={name} messages={shown ?? []} picked={picked} setPicked={setPicked} />
        </>
      )}
    </>
  );
}

function MessageTable({ team, messages, picked, setPicked }: { team: string; messages: LogMessage[]; picked: string | null; setPicked: (id: string) => void }) {
  const client = useClient();
  const [open, setOpen] = useState<string | null>(null);
  if (messages.length === 0) return <p className="quiet">No messages yet.</p>;
  const ids = new Set(messages.map((m) => m.display_id));
  const pick = (id: string) => {
    setPicked(id);
    document.getElementById(`msg-${id}`)?.scrollIntoView({ block: "center" });
  };
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
            <tr key={m.display_id} id={`msg-${m.display_id}`} className={picked === m.display_id ? "picked" : undefined}>
              <td title={m.time}>{new Date(m.time).toLocaleTimeString()}</td>
              <td>{m.display_id}</td>
              <td>
                {m.type}
                {m.commit && <code className="commit-chip badge">{m.commit}</code>}
                {(() => {
                  const link = answersLink(m, ids);
                  if (!link) return null;
                  return link.target === null ? (
                    <span className="sub badge">{link.text}</span>
                  ) : (
                    <button className="link-btn badge" onClick={() => pick(link.target!)}>
                      {link.text}
                    </button>
                  );
                })()}
              </td>
              <td>{m.from}</td>
              <td>
                <div className="deliveries">
                  {m.to.map((to) => {
                    const d = m.deliveries.find((x) => x.to === to);
                    return d ? <DeliveryChip key={to} to={to} state={d.state} attempts={d.attempts} reason={d.reason} /> : <span key={to} className="chip cat-idle">{to} · no delivery row</span>;
                  })}
                </div>
              </td>
              <td className="excerpt">
                <button
                  className="link-btn"
                  aria-expanded={open === m.display_id}
                  onClick={() => setOpen(open === m.display_id ? null : m.display_id)}
                >
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
