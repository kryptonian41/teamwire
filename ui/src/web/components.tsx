// Small pieces that every view shares.

import type { ReactNode } from "react";
import type { DeliveryState } from "../shared/teamwire.ts";
import type { StateCategory } from "../shared/state.ts";

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

export function DeliveryChip({ to, state }: { to: string; state: DeliveryState }) {
  return (
    <span className={`chip cat-${DELIVERY_CATEGORY[state]}`} title={`${to}: ${state}`}>
      {to} · {state}
    </span>
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
