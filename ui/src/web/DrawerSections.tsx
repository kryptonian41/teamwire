import { StateChip } from "./components.tsx";
import { Timeline } from "./Timeline.tsx";
import type { Detail } from "./model/detail.ts";
import type { Handoff, LeadSession, Usage, VerifyRun } from "./model/memberEvents.ts";
import { countText, usageTotal, verdictCategory } from "./model/memberEvents.ts";
import { spanText } from "./model/status.ts";

/** Why a fact is missing for this subject, such as `not available: removed`. */
export function Missing({ why }: { why: string }) {
  return <span className="quiet">{why}</span>;
}

const when = (time: string) => <span title={time}>{new Date(time).toLocaleString()}</span>;

/** What the event log says about the subject: the lead's sessions, handoffs, verify history, token use and its own events. */
export function DrawerEvents({ detail: d }: { detail: Detail }) {
  return (
    <>
      {d.sessions && <LeadSessions sessions={d.sessions} />}
      <h3>Handoffs</h3>
      {d.handoffs.ok ? <Handoffs handoffs={d.handoffs.value} /> : <p className="quiet">{d.handoffs.why}</p>}
      <h3>Verify history</h3>
      {d.verify.ok ? <VerifyRuns runs={d.verify.value} /> : <p className="quiet">{d.verify.why}</p>}
      <h3>Token use</h3>
      {d.tokens.ok ? <Tokens tokens={d.tokens.value} /> : <p className="quiet">{d.tokens.why}</p>}
      <h3>Events</h3>
      <Timeline rows={d.events} loading={false} error={null} />
    </>
  );
}

function LeadSessions({ sessions }: { sessions: LeadSession[] }) {
  return (
    <>
      <h3>Lead sessions</h3>
      {sessions.length === 0 ? (
        <p className="quiet">No team.created or team.lead_adopted event recorded.</p>
      ) : (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Event</th>
                <th>Time</th>
                <th>Harness</th>
                <th>Host</th>
                <th>Old session</th>
                <th>New session</th>
                <th>Folder</th>
              </tr>
            </thead>
            <tbody>
              {sessions.map((s) => (
                <tr key={s.seq}>
                  <td>{s.event === "created" ? "team created" : "lead adopted"}</td>
                  <td>{when(s.time)}</td>
                  <td>{s.harness ?? "—"}</td>
                  <td>{s.host ?? "—"}</td>
                  <td>{s.oldSession ? <code>{s.oldSession}</code> : "—"}</td>
                  <td>{s.newSession ? <code>{s.newSession}</code> : "—"}</td>
                  <td>{s.folder ? <code>{s.folder}</code> : "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}

function Handoffs({ handoffs }: { handoffs: Handoff[] }) {
  if (handoffs.length === 0) return <p className="quiet">No handoff recorded.</p>;
  return (
    <div className="table-wrap">
      <table>
        <thead>
          <tr>
            <th>Generation</th>
            <th>Time</th>
            <th>Old session</th>
            <th>New session</th>
          </tr>
        </thead>
        <tbody>
          {handoffs.map((h) => (
            <tr key={h.seq}>
              <td>{h.generation ?? "?"}</td>
              <td>{when(h.time)}</td>
              <td>{h.oldSession ? <code>{h.oldSession}</code> : "none"}</td>
              <td>{h.newSession ? <code>{h.newSession}</code> : "chosen by the harness"}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function VerifyRuns({ runs }: { runs: VerifyRun[] }) {
  if (runs.length === 0) return <p className="quiet">No verify run recorded.</p>;
  return (
    <ol className="verify-runs">
      {runs.map((r) => (
        <li key={r.seq}>
          <span className="chips">
            <StateChip word={r.verdict ?? "unknown"} category={verdictCategory(r.verdict)} />
            <span className="sub">
              {[r.exit === null ? null : `exit code ${r.exit}`, r.seconds === null ? null : spanText(r.seconds), new Date(r.time).toLocaleString()]
                .filter((p) => p !== null)
                .join(" · ")}
            </span>
          </span>
          {r.headSha && (
            <p className="sub">
              Checked commit <code>{r.headSha}</code>
            </p>
          )}
          {r.flags.length > 0 && (
            <ul className="flags">
              {r.flags.map((f) => (
                <li key={f}>
                  <code>{f}</code>
                  {r.flagsWaived && <span className="sub"> · waived</span>}
                </li>
              ))}
            </ul>
          )}
          {r.waivers.map((w) => (
            <p key={w.seq} className="sub">
              Waived{w.headSha ? ` at ${w.headSha}` : ""}: {w.flags.length > 0 ? w.flags.join(", ") : "no flags listed"}
            </p>
          ))}
        </li>
      ))}
    </ol>
  );
}

function Tokens({ tokens }: { tokens: [string, Usage][] }) {
  if (tokens.length === 0) return <p className="quiet">No token use recorded.</p>;
  return (
    <div className="table-wrap">
      <table>
        <thead>
          <tr>
            <th>Model</th>
            <th>In</th>
            <th>Out</th>
            <th>Cache read</th>
            <th>Cache written</th>
            <th>Total</th>
          </tr>
        </thead>
        <tbody>
          {tokens.map(([name, u]) => (
            <tr key={name}>
              <td>{name}</td>
              <td className="num">{countText(u.input)}</td>
              <td className="num">{countText(u.output)}</td>
              <td className="num">{countText(u.cacheRead)}</td>
              <td className="num">{countText(u.cacheCreation)}</td>
              <td className="num">{countText(usageTotal(u))}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
