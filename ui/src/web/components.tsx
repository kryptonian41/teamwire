// Small pieces that every view shares.

import { useState, type ReactNode } from "react";
import type { FileRef, FileText } from "../shared/api.ts";
import type { DeliveryState, EventRow } from "../shared/teamwire.ts";
import type { StateCategory } from "../shared/state.ts";
import { FileView } from "./FileView.tsx";
import { modelText, tokensText, verifyChip } from "./model/memberEvents.ts";
import { afkLinks, deliveryText } from "./model/status.ts";

/** A state word, coloured by its category. The word is always shown as text. */
export function StateChip({ word, category }: { word: string; category: StateCategory }) {
  return <span className={`chip cat-${category}`}>{word}</span>;
}

/** Delivery states reuse the category colours: good, working, waiting, late, broken. */
export const DELIVERY_CATEGORY: Record<DeliveryState, StateCategory> = {
  acked: "done",
  handed_off: "active",
  pending: "idle",
  unconfirmed: "attention",
  undelivered: "failed",
};

export function DeliveryChip({ to, state, attempts = 0, reason = null }: { to: string; state: DeliveryState; attempts?: number; reason?: string | null }) {
  return (
    <span className={`chip cat-${DELIVERY_CATEGORY[state]}`} title={`${to}: ${state}`}>
      {deliveryText({ to, state, attempts, reason })}
    </span>
  );
}

/** Links to an AFK run's files that exist; a click opens one in the file viewer below the links. */
export function AfkFiles({ id, files, load }: { id: string; files: readonly FileRef[]; load: (name: string) => Promise<FileText> }) {
  const [open, setOpen] = useState<string | null>(null);
  const links = afkLinks(files);
  if (links.length === 0) return null;
  return (
    <div className="afk-files">
      <p className="chips">
        {links.map((l) => (
          <button key={l.name} className="btn" aria-pressed={open === l.name} onClick={() => setOpen(open === l.name ? null : l.name)}>
            {l.label}
          </button>
        ))}
      </p>
      {open && <FileView id={`${id}/afk/${open}`} load={() => load(open)} />}
    </div>
  );
}

export function ErrorNote({ children }: { children: ReactNode }) {
  return (
    <p className="note error" role="alert">
      {children}
    </p>
  );
}

/** A view with nothing to show: what is missing, then what to do about it. */
export function Empty({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="empty">
      <p className="empty-title">{title}</p>
      {children && <p>{children}</p>}
    </div>
  );
}

export function Loading({ what }: { what: string }) {
  return (
    <p className="loading" role="status">
      Loading {what}…
    </p>
  );
}

/** The Model, Verify and Tokens cells of a members table row, from the event log. */
export function MemberEventCells({ rows, role }: { rows: readonly EventRow[]; role: string }) {
  const verify = verifyChip(rows, role);
  return (
    <>
      <td>{modelText(rows, role) ?? "—"}</td>
      <td>
        <StateChip word={verify.text} category={verify.category} />
      </td>
      <td className="num">{tokensText(rows, role)}</td>
    </>
  );
}
