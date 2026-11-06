import { useState } from "react";
import type { ArchiveEntry, FileRef } from "../shared/api.ts";
import { AfkFiles, Empty, ErrorNote, Loading, MemberEventCells, RolePlaceContext } from "./components.tsx";
import { Drawer } from "./Drawer.tsx";
import { FileView } from "./FileView.tsx";
import { useClient, useLoad, useTimeline } from "./hooks.ts";
import { groupArchives, groupFilesByRole } from "./model/history.ts";
import { removalText, rolesIn } from "./model/memberEvents.ts";
import { Timeline } from "./Timeline.tsx";
import { href } from "./model/route.ts";
import { buildDetail, knownRoles, type DetailSource } from "./model/detail.ts";
import { formatDuration, summarizeMetrics, type TeamMetrics } from "./model/teams.ts";

export function History({ liveTeams }: { liveTeams: ReadonlySet<string> }) {
  const client = useClient();
  const archives = useLoad(() => client.archives(), "archives");
  const metrics = useLoad(() => client.metrics(), "metrics");
  const summary = metrics.data ? summarizeMetrics(metrics.data.metrics) : new Map<string, TeamMetrics>();

  return (
    <>
      <div className="page-head">
        <h1>History</h1>
        <p className="lede">Closed teams, newest archive first, with totals from teamwire's metrics.</p>
      </div>
      {archives.loading && <Loading what="archives" />}
      {archives.error && <ErrorNote>{archives.error}</ErrorNote>}
      {metrics.error && <ErrorNote>Metrics: {metrics.error}</ErrorNote>}
      {archives.data && archives.data.length === 0 && (
        <Empty title="No closed teams yet">A team is archived here when its lead closes it.</Empty>
      )}
      {archives.data &&
        groupArchives(archives.data).map((g) => (
          <section key={g.team} className="history-team">
            <h2>
              <code>{g.team}</code>
            </h2>
            <SummaryLine m={summary.get(g.team)} live={liveTeams.has(g.team)} />
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Date</th>
                    <th>Closed at</th>
                    <th className="num">Files</th>
                    <th>Archive</th>
                  </tr>
                </thead>
                <tbody>
                  {g.archives.map((a) => (
                    <tr key={a.dir}>
                      <td>{a.date}</td>
                      <td>{a.closedAt ? new Date(a.closedAt).toLocaleString() : "unknown"}</td>
                      <td className="num">{a.files.length}</td>
                      <td>
                        <a href={href.archive(a.dir)}>
                          <code>{a.dir}</code>
                        </a>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>
        ))}
    </>
  );
}

function SummaryLine({ m, live }: { m: TeamMetrics | undefined; live: boolean }) {
  if (!m) return <p className="summary">No metrics for this team. Totals cover all of its archives.</p>;
  const done = Object.entries(m.completions)
    .map(([status, n]) => `${n} ${status}`)
    .join(", ");
  const run = m.durations["teamwire.member.run.duration"];
  return (
    <p className="summary">
      Completions: {done || "none"} · {m.messagesSent} messages sent · {m.leadWakeups} lead wakeups · {m.verifyRuns} verify
      runs
      {run && run.count > 0 && ` · member runs averaged ${formatDuration(run.mean)} over ${run.count}`}
      {live && " · Totals include the live team of the same name."}
    </p>
  );
}

export function Archive({ dir, member }: { dir: string; member: string | null }) {
  const client = useClient();
  const archives = useLoad(() => client.archives(), "archives");
  const [picked, setPicked] = useState<FileRef | null>(null);
  const entry: ArchiveEntry | undefined = archives.data?.find((a) => a.dir === dir);
  const timeline = useTimeline(() => client.archiveTimeline(dir), dir);
  const roles = rolesIn(timeline.rows);
  const source: DetailSource = { page: "archive", rows: timeline.rows, files: entry?.files ?? [] };
  const go = (role: string) => {
    location.hash = href.archiveMember(dir, role);
  };

  return (
    <RolePlaceContext.Provider value={{ page: "archive", dir, roles: knownRoles(source) }}>
      <p className="crumbs">
        <a href={href.history()}>← History</a>
      </p>
      <div className="page-head">
        <h1>{dir}</h1>
        {entry && (
          <p className="lede">
            {entry.date} · {entry.closedAt ? `closed ${new Date(entry.closedAt).toLocaleString()}` : "close time unknown"}
          </p>
        )}
      </div>
      {archives.loading && <Loading what="archive" />}
      {archives.error && <ErrorNote>{archives.error}</ErrorNote>}
      {archives.data && !entry && (
        <Empty title="No archive with that name">
          The list of archives is on the <a href={href.history()}>History</a> page.
        </Empty>
      )}
      {entry && (
        <>
          <h2>Members</h2>
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Role</th>
                  <th>Verify</th>
                  <th className="num">Tokens</th>
                  <th>Model</th>
                  <th>Why removed</th>
                </tr>
              </thead>
              <tbody>
                {roles.map((role) => (
                  <tr key={role} className="clickable" tabIndex={0} onClick={() => go(role)} onKeyDown={(e) => e.key === "Enter" && go(role)}>
                    <td className="member">
                      <a href={href.archiveMember(dir, role)}>{role}</a>
                    </td>
                    <MemberEventCells rows={timeline.rows} role={role} />
                    <td>{removalText(timeline.rows, role)}</td>
                  </tr>
                ))}
                <tr className="clickable" tabIndex={0} onClick={() => go("lead")} onKeyDown={(e) => e.key === "Enter" && go("lead")}>
                  <td className="member">
                    <a href={href.archiveMember(dir, "lead")}>lead</a>
                  </td>
                  <td>—</td>
                  <td className="num">—</td>
                  <td>—</td>
                  <td>—</td>
                </tr>
              </tbody>
            </table>
          </div>
          <h2>Files</h2>
          {entry.files.length === 0 && <Empty title="This archive has no files" />}
          <AfkFiles id={dir} files={entry.files} load={(n) => client.archiveFile(dir, "afk", n)} />
          {/* Each member's own files are in its drawer; message files belong to no member. */}
          {groupFilesByRole(entry.files.filter((f) => f.kind === "comms")).map((g) => (
            <div key={g.role} className="file-group">
              <h3>
                <code>{g.role}</code>
              </h3>
              <div className="chips">
                {g.files.map((f) => (
                  <button
                    key={`${f.kind}/${f.name}`}
                    className="chip"
                    aria-pressed={picked?.kind === f.kind && picked.name === f.name}
                    onClick={() => setPicked(f)}
                  >
                    {f.name} <span className="num">{f.bytes} B</span>
                  </button>
                ))}
              </div>
            </div>
          ))}
          {picked && (
            <>
              <h2>
                {picked.kind} · {picked.name}
              </h2>
              <FileView id={`${dir}/${picked.kind}/${picked.name}`} load={() => client.archiveFile(dir, picked.kind, picked.name)} />
            </>
          )}
          <h2>Timeline</h2>
          <Timeline rows={timeline.rows} loading={timeline.loading} error={timeline.error} />
        </>
      )}
      {member !== null && (
        <Drawer
          role={member}
          detail={buildDetail(member, source)}
          loading={timeline.loading || archives.loading}
          scope={dir}
          team={null}
          loadFile={(f) => client.archiveFile(dir, f.kind, f.name)}
          onClose={() => {
            location.hash = href.archive(dir);
          }}
        />
      )}
    </RolePlaceContext.Provider>
  );
}
