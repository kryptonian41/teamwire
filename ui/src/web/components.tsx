// Small pieces that every view shares.

import { useState, type ReactNode } from "react";
import type { FileRef, FileText } from "../shared/api.ts";
import type { DeliveryState, EventRow, TicketRef } from "../shared/teamwire.ts";
import type { StateCategory } from "../shared/state.ts";
import { FileView } from "./FileView.tsx";
import { modelText, tokensText, verifyChip } from "./model/memberEvents.ts";
import { afkLinks, deliveryText, refText } from "./model/status.ts";

/** One shape per category, so a state is never told by colour alone. */
export const SHAPE: Record<StateCategory, string> = { failed: "✕", attention: "!", unknown: "?", active: "▶", idle: "–", done: "✓" };

/** A state mark: the category's shape, then the word, coloured by the category. The word is always shown as text. */
export function StateChip({ word, category, title }: { word: string; category: StateCategory; title?: string }) {
  return (
    <span className={`chip cat-${category}`} title={title}>
      <span className="g" aria-hidden="true">
        {SHAPE[category]}
      </span>
      {word}
    </span>
  );
}

/** A team's ticket refs as text chips such as `linear:GKA-580`. No link: teamwire stores no tracker URL. With `onPick` a chip is a button. Renders nothing for none. */
export function RefChips({ refs, onPick }: { refs: readonly TicketRef[]; onPick?: (ref: TicketRef) => void }) {
  if (refs.length === 0) return null;
  return (
    <p className="chips" aria-label="Ticket refs">
      {refs.map((r) =>
        onPick ? (
          <button key={refText(r)} className="chip ref" title="Find decisions of teams with this ref" onClick={() => onPick(r)}>
            {refText(r)}
          </button>
        ) : (
          <span key={refText(r)} className="chip ref">
            {refText(r)}
          </span>
        ),
      )}
    </p>
  );
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
  return <StateChip word={deliveryText({ to, state, attempts, reason })} category={DELIVERY_CATEGORY[state]} title={`${to}: ${state}`} />;
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

/** The Verify, Tokens and Model cells of a members table row, from the event log. */
export function MemberEventCells({ rows, role }: { rows: readonly EventRow[]; role: string }) {
  const verify = verifyChip(rows, role);
  return (
    <>
      <td>{verify.text === "—" ? "—" : <StateChip word={verify.text} category={verify.category} />}</td>
      <td className="num">{tokensText(rows, role)}</td>
      <td>{modelText(rows, role) ?? "—"}</td>
    </>
  );
}
