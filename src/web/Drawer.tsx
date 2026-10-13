import { useEffect, useState } from "react";
import type { FileRef } from "../shared/api.ts";
import { StateChip } from "./components.tsx";
import { FileView } from "./FileView.tsx";
import { useClient, useLoad } from "./hooks.ts";
import { tabsFor } from "./model/files.ts";
import { formatAge, type JoinedMember } from "./model/teams.ts";

export function Drawer({
  team,
  member,
  files,
  onClose,
}: {
  team: string;
  member: JoinedMember;
  files: FileRef[];
  onClose: () => void;
}) {
  const client = useClient();
  const tabs = tabsFor(files, member.role);
  const [kind, setKind] = useState(tabs[0]?.kind ?? null);
  const age = formatAge(member.age_s, member.deadline_s);
  const resume = useLoad(() => client.resume(team, member.role), `${team}/${member.role}`);
  const [copied, setCopied] = useState(false);

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

  return (
    <>
      <div className="scrim" onClick={onClose} />
      <aside className="drawer" role="dialog" aria-label={`Member ${member.role}`}>
        <header>
          <div>
            <h2 style={{ margin: 0 }}>{member.role}</h2>
            <div className="sub">
              {member.title} · {member.kind ?? "kind unknown"} · generation {member.generation}
            </div>
          </div>
          <button className="btn" onClick={onClose} aria-label="Close">
            Close
          </button>
        </header>
        <div className="row" style={{ marginTop: 0 }}>
          <StateChip word={member.state} category={member.category} />
          <span className={age.overdue ? "badge bad" : "sub"}>{age.label}</span>
          {member.branch && <code>{member.branch}</code>}
        </div>

        <div className="resume">
          <strong>Resume</strong>
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
          <p className="empty">No files for this member yet.</p>
        ) : (
          <>
            <div className="tabs" role="tablist">
              {tabs.map((t) => (
                <button key={t.kind} role="tab" aria-selected={t.kind === active?.kind} onClick={() => setKind(t.kind)}>
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
    </>
  );
}
