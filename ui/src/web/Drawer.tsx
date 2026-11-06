import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { FileRef } from "../shared/api.ts";
import type { EventRow, RosterMember } from "../shared/teamwire.ts";
import { StateChip } from "./components.tsx";
import { MessageButton } from "./Actions.tsx";
import { canMessage } from "./model/actions.ts";
import { Timeline } from "./Timeline.tsx";
import { FileView } from "./FileView.tsx";
import { useClient, useLoad } from "./hooks.ts";
import { tabsFor } from "./model/files.ts";
import { countText, handoffHistory, modelText, rowsForRole, tokensByModel, usageTotal, verdictCategory, verifyHistory } from "./model/memberEvents.ts";
import { spanText, startedByLine } from "./model/status.ts";
import { formatAge, type JoinedMember } from "./model/teams.ts";

export function Drawer({
  team,
  member,
  roster,
  files,
  rows,
  actions,
  onClose,
}: {
  team: string;
  member: JoinedMember;
  roster: RosterMember | null;
  files: FileRef[];
  rows: readonly EventRow[];
  /** Write actions are on: show the member's action buttons. */
  actions: boolean;
  onClose: () => void;
}) {
  const client = useClient();
  const tabs = tabsFor(files, member.role);
  const [kind, setKind] = useState(tabs[0]?.kind ?? null);
  const age = formatAge(member.age_s, member.deadline_s);
  const resume = useLoad(() => client.resume(team, member.role), `${team}/${member.role}`);
  const [copied, setCopied] = useState(false);
  const closeRef = useRef<HTMLButtonElement>(null);

  // Move focus into the dialog so the keyboard starts where the eye does, and
  // give it back on close. Opened from a URL there is no opener, so fall back
  // to the member's link in the table. The page behind is inert while the
  // dialog is open, which keeps Tab inside it and backs up aria-modal; the
  // dialog is portalled out of #root so it is not inert itself.
  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement && document.activeElement !== document.body ? document.activeElement : null;
    const page = document.getElementById("root");
    if (page) page.inert = true;
    closeRef.current?.focus();
    return () => {
      if (page) page.inert = false;
      // The link's hash also names the tab, so match on its tail.
      const back = opener?.isConnected ? opener : document.querySelector<HTMLElement>(`a[href$="/member/${member.role}"]`);
      back?.focus();
    };
  }, [team, member.role]);

  useEffect(() => {
    const on = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", on);
    return () => window.removeEventListener("keydown", on);
  }, [onClose]);

  const active = tabs.find((t) => t.kind === kind) ?? tabs[0] ?? null;
  const command = resume.data?.command ?? null;

  const copy = async () => {
    if (!command) return;
    try {
      await navigator.clipboard.writeText(command);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // Clipboard access can be refused; the command stays visible to copy by hand.
    }
  };

  return createPortal(
    <>
      <div className="scrim" onClick={onClose} />
      <aside className="drawer" role="dialog" aria-modal="true" aria-label={`Member ${member.role}`}>
        <header>
          <div>
            <h2>{member.role}</h2>
            <p className="sub">{member.title}</p>
          </div>
          <div className="drawer-controls">
            <StateChip word={member.state} category={member.category} />
            {/* Empty while actions are off, and then it takes no space. Plan D4: reviewers, closers and plan reviewers get no message control. */}
            <div className="member-actions">
              {actions && canMessage(member.kind) && (
                <>
                  <MessageButton team={team} to={member.role} type="reprompt" label="Reprompt" title={`Reprompt ${member.role}`} />
                  <MessageButton team={team} to={member.role} type="note" label="Note" title={`Note to ${member.role}`} />
                </>
              )}
            </div>
            <button ref={closeRef} className="btn" onClick={onClose} aria-label="Close">
              Close
            </button>
          </div>
        </header>

        <dl className="facts">
          <dt>Kind</dt>
          <dd>{member.kind ?? "unknown"}</dd>
          <dt>Generation</dt>
          <dd>{member.generation}</dd>
          <dt>Branch</dt>
          <dd>{member.branch ? <code>{member.branch}</code> : "—"}</dd>
          <dt>Model</dt>
          <dd>{modelText(rows, member.role) ?? "—"}</dd>
          <dt>Deadline</dt>
          <dd>
            <span className="num">{age.label}</span>
            {age.overdue && <StateChip word="past its deadline" category="attention" />}
          </dd>
          {roster && (
            <>
              <dt>May change</dt>
              <dd>{roster.owns.length === 0 ? "nothing (read-only)" : roster.owns.map((o) => <code key={o}>{o}</code>)}</dd>
              <dt>Accepts from members</dt>
              <dd>{roster.accepts_from_members.length === 0 ? "nothing" : roster.accepts_from_members.join(", ")}</dd>
              <dt>Lead can type to it</dt>
              <dd>{roster.can_be_typed_to ? "yes" : "no"}</dd>
            </>
          )}
          {startedByLine(member) && (
            <>
              <dt>Build</dt>
              <dd>{startedByLine(member)}</dd>
            </>
          )}
        </dl>

        <MemberEvents rows={rows} role={member.role} />

        <div className="resume">
          <h3>Resume</h3>
          {resume.loading && <span className="note">Loading…</span>}
          {resume.error && <span className="note error">{resume.error}</span>}
          {resume.data && command && (
            <>
              <code>{command}</code>
              <button className="btn" onClick={copy}>
                {copied ? "Copied" : "Copy"}
              </button>
            </>
          )}
          {resume.data && !command && (
            <span className="note">
              No resume command.{" "}
              {resume.data.problems.map((p) => p.detail).join(" ")}
            </span>
          )}
        </div>

        {tabs.length === 0 ? (
          <p className="quiet">No files for this member yet.</p>
        ) : (
          <>
            <div className="tabs" role="tablist">
              {tabs.map((t) => (
                <button
                  key={t.kind}
                  role="tab"
                  aria-selected={t.kind === active?.kind}
                  onClick={() => setKind(t.kind)}
                >
                  {t.label}
                </button>
              ))}
            </div>
            {active && (
              <FileView
                id={`${team}/${active.kind}/${member.role}`}
                load={() => client.teamFile(team, active.kind, member.role)}
              />
            )}
          </>
        )}
      </aside>
    </>,
    document.body,
  );
}

/** What the event log says about the member: verify history, token use and its own events. */
function MemberEvents({ rows, role }: { rows: readonly EventRow[]; role: string }) {
  const runs = verifyHistory(rows, role);
  const tokens = [...tokensByModel(rows, role)];
  const handoffs = handoffHistory(rows, role);
  return (
    <>
      {handoffs.length > 0 && (
        <>
          <h3>Handoffs</h3>
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
                    <td title={h.time}>{new Date(h.time).toLocaleString()}</td>
                    <td>{h.oldSession ? <code>{h.oldSession}</code> : "none"}</td>
                    <td>{h.newSession ? <code>{h.newSession}</code> : "chosen by the harness"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}
      <h3>Verify history</h3>
      {runs.length === 0 ? (
        <p className="quiet">No verify run recorded.</p>
      ) : (
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
      )}
      <h3>Token use</h3>
      {tokens.length === 0 ? (
        <p className="quiet">No token use recorded.</p>
      ) : (
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
      )}
      <h3>Events</h3>
      <Timeline rows={rowsForRole(rows, role)} loading={false} error={null} />
    </>
  );
}
