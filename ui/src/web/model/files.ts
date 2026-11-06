// Which tabs the member drawer shows, from the files that exist for a role.

import type { FileKind, FileRef } from "../../shared/api.ts";

/** In display order. `comms` files belong to messages, not to a role. */
export const TAB_LABELS: Partial<Record<FileKind, string>> = {
  brief: "Brief",
  contract: "Contract",
  outbox: "Result",
  report: "Report",
  handoff: "Handoff",
  verify: "Check log",
  setup: "Setup log",
};

export interface Tab {
  kind: FileKind;
  label: string;
}

export function tabsFor(files: readonly FileRef[], role: string): Tab[] {
  const have = new Set(files.filter((f) => f.name === role).map((f) => f.kind));
  return (Object.keys(TAB_LABELS) as FileKind[])
    .filter((k) => have.has(k))
    .map((kind) => ({ kind, label: TAB_LABELS[kind] ?? kind }));
}
