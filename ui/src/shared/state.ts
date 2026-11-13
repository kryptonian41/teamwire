// How the page groups teamwire's free-form member state words. Every view
// colours a member by its category, so the mapping lives in one place.

export type StateCategory =
  /** Finished with status `done` and the lead has the result. */
  | "done"
  /** Working now, or still being created. */
  | "active"
  /** Alive and idle with no completion: `quiet`. */
  | "idle"
  /** Needs the lead to look: late, blocked, maybe dead, or a result stuck on its way. */
  | "attention"
  /** Finished as failed, or never started. */
  | "failed"
  /** A word this app does not know. Show it as teamwire printed it. */
  | "unknown";

const ATTENTION = new Set([
  "late",
  "blocked",
  "died?",
  "unreported",
  "undelivered",
  "unconfirmed",
  "prompted outside teamwire",
]);

/** The category of one `MemberView.state` word. */
export function classifyState(word: string): StateCategory {
  const done = /^done\((.*)\)$/.exec(word);
  if (done) {
    const status = done[1];
    if (status === "done") return "done";
    if (status === "failed") return "failed";
    return "attention";
  }
  // `working (at a dialog)`: a member whose result was not typed is mid-turn
  // at a dialog, which the watch counts as working (teamwire ADR 0043).
  if (word === "working" || word === "working (at a dialog)" || word === "not started") return "active";
  if (word === "quiet") return "idle";
  if (word === "failed to start") return "failed";
  if (ATTENTION.has(word)) return "attention";
  return "unknown";
}

/** Categories in the order an overview sorts members: worst first. */
export const CATEGORY_ORDER: readonly StateCategory[] = [
  "failed",
  "attention",
  "unknown",
  "active",
  "idle",
  "done",
];
