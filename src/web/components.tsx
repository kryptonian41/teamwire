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
    <span className={`chip small cat-${DELIVERY_CATEGORY[state]}`} title={`${to}: ${state}`}>
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

export function Loading({ what }: { what: string }) {
  return <p className="note">Loading {what}…</p>;
}
