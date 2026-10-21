import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { FileRef } from "../shared/api.ts";
import type { RosterMember } from "../shared/teamwire.ts";
import { StateChip } from "./components.tsx";
import { FileView } from "./FileView.tsx";
import { useClient, useLoad } from "./hooks.ts";
import { tabsFor } from "./model/files.ts";
import { href } from "./model/route.ts";
import { acceptsLine, ownsLine, startedByLine } from "./model/status.ts";
import { formatAge, type JoinedMember } from "./model/teams.ts";

export function Drawer({
  team,
  member,
  roster,
  files,
  onClose,
}: {
  team: string;
  member: JoinedMember;
  roster: RosterMember | null;
  files: FileRef[];
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
      const back = opener?.isConnected ? opener : document.querySelector<HTMLElement>(`a[href="${href.member(team, member.role)}"]`);
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
            <p className="sub">
              {member.title} · {member.kind ?? "kind unknown"} · generation {member.generation}
            </p>
          </div>
          <button ref={closeRef} className="btn" onClick={onClose} aria-label="Close">
            Close
          </button>
        </header>
        <p className="chips">
          <StateChip word={member.state} category={member.category} />
          {age.overdue ? <span className="flag bad">{age.label} · past its deadline</span> : <span className="sub num">{age.label}</span>}
          {member.branch && <code>{member.branch}</code>}
        </p>

        <ul className="facts">
          {startedByLine(member) && <li>{startedByLine(member)}</li>}
          {roster && <li>{ownsLine(roster)}</li>}
          {roster && <li>{acceptsLine(roster)}</li>}
        </ul>

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
