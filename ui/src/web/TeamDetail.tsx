import { useEffect, useState } from "react";
import type { Snapshot } from "../shared/api.ts";
import type { StateCategory } from "../shared/state.ts";
import type { EventRow, LogMessage } from "../shared/teamwire.ts";
import { AfkFiles, DeliveryChip, Empty, ErrorNote, Loading, MemberEventCells, StateChip } from "./components.tsx";
import { Drawer } from "./Drawer.tsx";
import { FileView } from "./FileView.tsx";
import { Flow, type Selected } from "./Flow.tsx";
import { useClient, useLoad, useNow, useTimeline } from "./hooks.ts";
import { buildFlow, filterByPair } from "./model/flow.ts";
import { href } from "./model/route.ts";
import { afkLine, answersLink, startedByLine, unansweredLines, unreadLines, watchLine } from "./model/status.ts";
import { countByCategory, formatAge, leadCategory, livenessInfo, refreshKey, type TeamModel } from "./model/teams.ts";
import { removalText } from "./model/memberEvents.ts";
import { Timeline } from "./Timeline.tsx";
import { CATEGORY_ORDER } from "../shared/state.ts";

/** The longest the files and the log go without a reload. */
const REFRESH_MS = 60_000;

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
  const tick = useNow(REFRESH_MS);
  const refresh = team ? `${refreshKey(team.status)}|${tick}` : "";
  const files = useLoad(() => client.teamFiles(name), name, refresh);
  const timeline = useTimeline((after) => client.teamTimeline(name, after), name, refresh);
  const [pick, setPick] = useState<{ id: string; n: number } | null>(null);
  if (!snapshot) return <Empty title="Waiting for the first poll…" />;
  if (!team || team.name === null) {
    return (
      <>
        <p className="crumbs">
          <a href={href.overview()}>← Teams</a>
        </p>
        <Empty title="No team with that name">
          It may have closed. Closed teams are under <a href={href.history()}>History</a>.
        </Empty>
      </>
    );
  }
  const s = team.status;
  const live = livenessInfo(s.lead);
  const open = member === null ? null : team.members.find((m) => m.role === member);
  const counts = countByCategory(team.members);
  const unread = unreadLines(s);
  const unanswered = unansweredLines(s);
  const older = team.members.filter((m) => m.older_build);

  return (
    <>
      <p className="crumbs">
        <a href={href.overview()}>← Teams</a>
      </p>
      <div className="page-head">
        <h1>{team.title}</h1>
        <p className="meta">
          <span>{team.name}</span>
          <span className="chip cat-idle">{s.state ?? "state unknown"}</span>
          <span className="live">
            <span className={`dot cat-${live.category}`} aria-hidden="true" />
            {live.label}
          </span>
        </p>
        <p className="meta">
          <span>{watchLine(s.watch)}</span>
          {s.afk && <span className={s.afk.spent ? "flag bad" : undefined}>{afkLine(s.afk)}</span>}
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
        {s.afk && <AfkFiles id={name} files={files.data ?? []} load={(n) => client.teamFile(name, "afk", n)} />}
      </div>
      {s.errors.length > 0 && (
        <ul className="errors page-errors">
          {s.errors.map((e, i) => (
            <li key={i}>{e}</li>
          ))}
        </ul>
      )}

      <h2>Members</h2>
      <MemberTable team={team} rows={timeline.rows} />

      {(s.stuck_deliveries.length > 0 || s.undelivered_results.length > 0 || unread.length > 0 || unanswered.length > 0 || older.length > 0) && (
        <>
          <h2>Needs a look</h2>
          {(unread.length > 0 || unanswered.length > 0) && (
            <>
              <h3>Waiting on a reply</h3>
              <ul className="results">
                {[...unread, ...unanswered].map((line) => (
                  <li key={line}>{line}</li>
                ))}
              </ul>
            </>
          )}
          {older.length > 0 && (
            <>
              <h3>Older builds</h3>
              <ul className="results">
                {older.map((m) => (
                  <li key={m.role}>
                    <strong>{m.role}</strong>: {startedByLine(m)}
                  </li>
                ))}
              </ul>
            </>
          )}
          {s.stuck_deliveries.length > 0 && (
            <>
            <h3>Stuck deliveries</h3>
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
            </>
          )}
          {s.undelivered_results.length > 0 && (
            <>
            <h3>Undelivered results</h3>
            <ul className="results">
              {s.undelivered_results.map((r) => (
                <li key={`${r.role}-${r.n}`}>
                  Result {r.n} from <strong>{r.role}</strong>: {r.report_state} <code>{r.path}</code>
                </li>
              ))}
            </ul>
            </>
          )}
        </>
      )}

      <MessageSection team={team} pick={pick} />

      <h2>Timeline</h2>
      <Timeline
        rows={timeline.rows}
        loading={timeline.loading}
        error={timeline.error}
        onPickMessage={(id) => setPick({ id, n: (pick?.n ?? 0) + 1 })}
      />

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
                  <th>Why removed</th>
                </tr>
              </thead>
              <tbody>
                {team.removed.map((m) => (
                  <tr key={m.role}>
                    <td>{m.role}</td>
                    <td>{m.title}</td>
                    <td>{m.kind}</td>
                    <td>{m.generation}</td>
                    <td>{removalText(timeline.rows, m.role)}</td>
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
          roster={team.roster?.members.find((r) => r.role === open.role) ?? null}
          files={files.data ?? []}
          rows={timeline.rows}
          onClose={() => {
            location.hash = href.team(team.name ?? "");
          }}
        />
      )}
    </>
  );
}

function MemberTable({ team, rows }: { team: TeamModel; rows: readonly EventRow[] }) {
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

function MessageSection({ team, pick }: { team: TeamModel; pick: { id: string; n: number } | null }) {
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
