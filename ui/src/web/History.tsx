import { useState } from "react";
import type { ArchiveEntry, FileRef } from "../shared/api.ts";
import { AfkFiles, Empty, ErrorNote, Loading } from "./components.tsx";
import { FileView } from "./FileView.tsx";
import { useClient, useLoad } from "./hooks.ts";
import { groupArchives, groupFilesByRole } from "./model/history.ts";
import { href } from "./model/route.ts";
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
          {entry.files.length === 0 && <Empty title="This archive has no files" />}
          <AfkFiles id={dir} files={entry.files} load={(n) => client.archiveFile(dir, "afk", n)} />
          {groupFilesByRole(entry.files.filter((f) => f.kind !== "afk")).map((g) => (
            <div key={g.role} className="file-group">
              <h3>{g.role}</h3>
              <div className="chips">
                {g.files.map((f) => (
                  <button
                    key={`${f.kind}/${f.name}`}
                    className="btn"
                    aria-pressed={picked?.kind === f.kind && picked.name === f.name}
                    onClick={() => setPicked(f)}
                  >
                    {f.kind === "comms" ? f.name : f.kind} <span className="sub num">{f.bytes} B</span>
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
