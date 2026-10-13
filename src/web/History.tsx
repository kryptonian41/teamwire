import { useState } from "react";
import type { ArchiveEntry, FileRef } from "../shared/api.ts";
import { ErrorNote, Loading } from "./components.tsx";
import { FileView } from "./FileView.tsx";
import { useClient, useLoad } from "./hooks.ts";
import { groupArchives, groupFilesByRole } from "./model/history.ts";
import { href } from "./model/route.ts";
import { summarizeMetrics, type TeamMetrics } from "./model/teams.ts";

export function History({ liveTeams }: { liveTeams: ReadonlySet<string> }) {
  const client = useClient();
  const archives = useLoad(() => client.archives(), "archives");
  const metrics = useLoad(() => client.metrics(), "metrics");
  const summary = metrics.data ? summarizeMetrics(metrics.data.metrics) : new Map<string, TeamMetrics>();

  return (
    <>
      <h1>History</h1>
      {archives.loading && <Loading what="archives" />}
      {archives.error && <ErrorNote>{archives.error}</ErrorNote>}
      {metrics.error && <ErrorNote>Metrics: {metrics.error}</ErrorNote>}
      {archives.data && archives.data.length === 0 && <p className="empty">No closed teams yet.</p>}
      {archives.data &&
        groupArchives(archives.data).map((g) => (
          <section key={g.team}>
            <h2>{g.team}</h2>
            <SummaryLine m={summary.get(g.team)} live={liveTeams.has(g.team)} />
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Date</th>
                    <th>Closed at</th>
                    <th>Files</th>
                    <th>Archive</th>
                  </tr>
                </thead>
                <tbody>
                  {g.archives.map((a) => (
                    <tr key={a.dir}>
                      <td>{a.date}</td>
                      <td>{a.closedAt ? new Date(a.closedAt).toLocaleString() : "unknown"}</td>
                      <td>{a.files.length}</td>
                      <td>
                        <a href={href.archive(a.dir)}>{a.dir}</a>
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
  if (!m) return <p className="sub">No metrics for this team. Totals cover all of its archives.</p>;
  const done = Object.entries(m.completions)
    .map(([status, n]) => `${n} ${status}`)
    .join(", ");
  return (
    <p className="sub">
      Completions: {done || "none"} · {m.messagesSent} messages sent · {m.leadWakeups} lead wakeups · {m.verifyRuns} verify
      runs
      {live && " · Totals include the live team of the same name."}
    </p>
  );
}

export function Archive({ dir }: { dir: string }) {
  const client = useClient();
  const archives = useLoad(() => client.archives(), "archives");
  const [picked, setPicked] = useState<FileRef | null>(null);
  const entry: ArchiveEntry | undefined = archives.data?.find((a) => a.dir === dir);

  return (
    <>
      <p className="crumbs">
        <a href={href.history()}>← History</a>
      </p>
      <h1>{dir}</h1>
      {archives.loading && <Loading what="archive" />}
      {archives.error && <ErrorNote>{archives.error}</ErrorNote>}
      {archives.data && !entry && <p className="empty">No archive with that name.</p>}
      {entry && (
        <>
          <p className="sub">
            {entry.date} · {entry.closedAt ? `closed ${new Date(entry.closedAt).toLocaleString()}` : "close time unknown"}
          </p>
          {entry.files.length === 0 && <p className="empty">This archive has no files.</p>}
          {groupFilesByRole(entry.files).map((g) => (
            <div key={g.role}>
              <h3>{g.role}</h3>
              <div className="row" style={{ marginTop: 0 }}>
                {g.files.map((f) => (
                  <button
                    key={`${f.kind}/${f.name}`}
                    className="btn"
                    aria-pressed={picked?.kind === f.kind && picked.name === f.name}
                    onClick={() => setPicked(f)}
                  >
                    {f.kind === "comms" ? f.name : f.kind} <span className="sub">({f.bytes} B)</span>
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
        </>
      )}
    </>
  );
}
