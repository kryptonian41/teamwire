import { useState } from "react";
import type { Snapshot } from "../shared/api.ts";
import { AfkFiles, DeliveryChip, Empty, RefChips } from "./components.tsx";
import { Drawer } from "./Drawer.tsx";
import { REFRESH_MS, useClient, useLoad, useNow, useTimeline } from "./hooks.ts";
import { MemberTable } from "./MemberTable.tsx";
import { MessageSection } from "./Messages.tsx";
import { href } from "./model/route.ts";
import { afkLine, refText, startedByLine, unansweredLines, unreadLines, watchLine } from "./model/status.ts";
import { countByCategory, livenessInfo, refreshKey, type TeamModel } from "./model/teams.ts";
import { removalText } from "./model/memberEvents.ts";
import { Timeline } from "./Timeline.tsx";
import { Decisions } from "./Decisions.tsx";
import { Tokens } from "./Tokens.tsx";
import type { DecisionQuery } from "../shared/api.ts";
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
  const tick = useNow(REFRESH_MS);
  const refresh = team ? `${refreshKey(team.status)}|${tick}` : "";
  const files = useLoad(() => client.teamFiles(name), name, refresh);
  const timeline = useTimeline((after) => client.teamTimeline(name, after), name, refresh);
  const [pick, setPick] = useState<{ id: string; n: number } | null>(null);
  const [query, setQuery] = useState<DecisionQuery | null>(null);
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
        <RefChips
          refs={s.refs}
          onPick={(r) => {
            setQuery({ ref: refText(r) });
            document.getElementById("decisions")?.scrollIntoView({ block: "start" });
          }}
        />
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

      <Decisions team={name} refresh={refresh} query={query} onQuery={setQuery} />

      <Tokens team={name} refresh={refresh} />

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
