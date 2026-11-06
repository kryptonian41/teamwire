import { useEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import type { FileText } from "../shared/api.ts";
import { StateChip } from "./components.tsx";
import { MessageButton } from "./Actions.tsx";
import { DrawerEvents, Missing } from "./DrawerSections.tsx";
import { FileView } from "./FileView.tsx";
import { useClient, useLoad } from "./hooks.ts";
import type { Detail, Fact, FileTab } from "./model/detail.ts";

/** A fact's value through `render`, or the reason it is missing. */
function show<T>(fact: Fact<T>, render: (v: T) => ReactNode): ReactNode {
  return fact.ok ? render(fact.value) : <Missing why={fact.why} />;
}

/**
 * The drawer for a live member, a removed member, an archived member or the
 * lead. It only lays out the record that `buildDetail` made; the rules about
 * what each subject has live in model/detail.ts.
 */
export function Drawer({
  role,
  detail,
  loading,
  scope,
  team,
  loadFile,
  onCloseMember,
  onClose,
}: {
  role: string;
  /** Null when no source knows the role. */
  detail: Detail | null;
  /** The sources are still loading, so a null record may be filled in soon. */
  loading: boolean;
  /** The team or archive the files belong to, for the file view's cache key. */
  scope: string;
  /** The live team, for the action buttons and the resume command; null on an archive. */
  team: string | null;
  loadFile: (tab: FileTab) => Promise<FileText>;
  /** Opens the close dialog, which the team page keeps after the member is gone. */
  onCloseMember: () => void;
  onClose: () => void;
}) {
  const closeRef = useRef<HTMLButtonElement>(null);

  // Move focus into the dialog so the keyboard starts where the eye does, and
  // give it back on close. Opened from a URL there is no opener, so fall back
  // to the role's link on the page. The page behind is inert while the
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
      const back = opener?.isConnected ? opener : document.querySelector<HTMLElement>(`a[href$="/member/${role}"]`);
      back?.focus();
    };
  }, [scope, role]);

  useEffect(() => {
    const on = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", on);
    return () => window.removeEventListener("keydown", on);
  }, [onClose]);

  const title = detail?.subject === "lead" ? "The team's lead" : detail?.roster.ok ? detail.roster.value.title : null;
  const actions = detail?.actions ?? null;

  return createPortal(
    <>
      <div className="scrim" onClick={onClose} />
      <aside className="drawer" role="dialog" aria-modal="true" aria-label={role === "lead" ? "Lead" : `Member ${role}`}>
        <header>
          <div>
            <h2>{role}</h2>
            {title && <p className="sub">{title}</p>}
          </div>
          <div className="drawer-controls">
            {detail && <StateChip word={detail.state.word} category={detail.state.category} />}
            {/* Empty while actions are off, and then it takes no space. Plan D4: reviewers, closers and plan reviewers get no message control. */}
            <div className="member-actions">
              {team && actions?.reprompt && <MessageButton team={team} to={role} type="reprompt" label="Reprompt" title={`Reprompt ${role}`} />}
              {team && actions?.note && <MessageButton team={team} to={role} type="note" label="Note" title={`Note to ${role}`} />}
              {team && actions?.close && (
                <button className="btn" onClick={onCloseMember}>
                  Close member
                </button>
              )}
            </div>
            <button ref={closeRef} className="btn" onClick={onClose} aria-label="Close">
              Close
            </button>
          </div>
        </header>
        {detail ? (
          <Body detail={detail} scope={scope} team={team} loadFile={loadFile} />
        ) : loading ? (
          <p className="loading" role="status">
            Loading…
          </p>
        ) : (
          <p className="quiet">Nothing here knows a member named {role}. It may belong to another team, or its events may be gone.</p>
        )}
      </aside>
    </>,
    document.body,
  );
}

function Body({ detail: d, scope, team, loadFile }: { detail: Detail; scope: string; team: string | null; loadFile: (tab: FileTab) => Promise<FileText> }) {
  const roster = d.roster;
  return (
    <>
      <dl className="facts">
        <dt>Kind</dt>
        <dd>{show(d.kind, (k) => k)}</dd>
        <dt>Generation</dt>
        <dd>{show(d.generation, (g) => g)}</dd>
        <dt>Branch</dt>
        <dd>{show(d.branch, (b) => (b ? <code>{b}</code> : "—"))}</dd>
        <dt>Model</dt>
        <dd>{show(d.model, (m) => m ?? "—")}</dd>
        <dt>Deadline</dt>
        <dd>
          {show(d.age, (age) => (
            <>
              <span className="num">{age.label}</span>
              {age.overdue && <StateChip word="past its deadline" category="attention" />}
            </>
          ))}
        </dd>
        <dt>May change</dt>
        <dd>{show(roster, (r) => (r.owns.length === 0 ? "nothing (read-only)" : r.owns.map((o) => <code key={o}>{o}</code>)))}</dd>
        <dt>Accepts from members</dt>
        <dd>{show(roster, (r) => (r.acceptsFromMembers.length === 0 ? "nothing" : r.acceptsFromMembers.join(", ")))}</dd>
        <dt>Lead can type to it</dt>
        <dd>{show(roster, (r) => (r.canBeTypedTo ? "yes" : "no"))}</dd>
        {d.build && (
          <>
            <dt>Build</dt>
            <dd>{d.build}</dd>
          </>
        )}
        {d.removal && (
          <>
            <dt>Why removed</dt>
            <dd>{d.removal}</dd>
          </>
        )}
        {d.session && (
          <>
            <dt>Session</dt>
            <dd>{show(d.session, (s) => (s ? <code>{s}</code> : "—"))}</dd>
          </>
        )}
      </dl>

      <DrawerEvents detail={d} />

      <div className="resume">
        <h3>Resume</h3>
        {team && d.resume.ok ? <Resume team={team} role={d.role} /> : !d.resume.ok && <Missing why={d.resume.why} />}
      </div>

      {show(d.files, (tabs) => (tabs.length === 0 ? <p className="quiet">No files for this member yet.</p> : <Files tabs={tabs} scope={scope} loadFile={loadFile} />))}
    </>
  );
}

function Resume({ team, role }: { team: string; role: string }) {
  const client = useClient();
  const resume = useLoad(() => client.resume(team, role), `${team}/${role}`);
  const [copied, setCopied] = useState(false);
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
  return (
    <>
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
          No resume command. {resume.data.problems.map((p) => p.detail).join(" ")}
        </span>
      )}
    </>
  );
}

function Files({ tabs, scope, loadFile }: { tabs: FileTab[]; scope: string; loadFile: (tab: FileTab) => Promise<FileText> }) {
  const key = (t: FileTab) => `${t.kind}/${t.name}`;
  const [picked, setPicked] = useState(key(tabs[0]!));
  const active = tabs.find((t) => key(t) === picked) ?? tabs[0]!;
  return (
    <>
      <div className="tabs" role="tablist">
        {tabs.map((t) => (
          <button key={key(t)} role="tab" aria-selected={key(t) === key(active)} onClick={() => setPicked(key(t))}>
            {t.label}
          </button>
        ))}
      </div>
      <FileView id={`${scope}/${key(active)}`} load={() => loadFile(active)} />
    </>
  );
}
