// What the page shows around the write actions: who gets which control, and
// what each exit code means to the user. The CLI enforces the rules; these
// functions only keep the page from offering what the CLI would refuse.

import type { ActionResult, LeadNote } from "../../shared/api.ts";
import type { MemberKind } from "../../shared/teamwire.ts";

export interface Outcome {
  ok: boolean;
  text: string;
}

/** Plan D4: the user messages implementers and investigators. Reviewers, closers and plan reviewers stay lead-only. */
export function canMessage(kind: MemberKind | null): boolean {
  return kind === "implementer" || kind === "investigator";
}

/** The CLI's own words: stderr, or stdout when stderr is empty. */
export function cliText(r: Pick<ActionResult, "stdout" | "stderr">): string {
  return (r.stderr || r.stdout).trim();
}

const statusHint = (team: string) => `check \`teamwire status ${team}\``;

/** The page text for a send, by exit code (the brief's item 5 table). */
export function sendOutcome(r: ActionResult, team: string): Outcome {
  // Plan D4: the exit code ignores the lead copy, so its failure shows on stderr only.
  const copyMissed = /^lead copy not delivered:/m.test(r.stderr) ? "; the lead's copy was not delivered" : "";
  const cli = cliText(r);
  switch (r.exit) {
    case null:
      return { ok: false, text: `outcome unknown: ${statusHint(team)}` };
    case 0:
      return { ok: true, text: `sent${copyMissed}` };
    case 1:
      return { ok: false, text: `refused: ${cli}${copyMissed}` };
    case 3:
      return { ok: false, text: `teamwire refused the arguments (a bug in the dashboard): ${cli}` };
    case 4:
      return { ok: false, text: `not delivered${copyMissed}` };
    case 5:
      return { ok: false, text: `unconfirmed: ${statusHint(team)} before sending again${copyMissed}` };
    case 6:
      return { ok: false, text: `store error: ${cli}` };
    default:
      return { ok: false, text: `teamwire send exited ${r.exit}: ${cli}` };
  }
}

/** Plan D11: a reply note starts with the question's id. */
export const replyPrefix = (questionId: string) => `re ${questionId}: `;

/** The role a "Reply with a note" goes to, or null when plan D11 does not offer one for this asker. */
export function replyTarget(asker: string, members: readonly { role: string; kind: MemberKind | null }[]): string | null {
  const m = members.find((x) => x.role === asker);
  return m && canMessage(m.kind) ? m.role : null;
}

export function leadNoteLines(notes: readonly LeadNote[]): string[] {
  return notes.map((n) => (n.told ? `The lead was told: ${n.text}` : `The lead was not told (${n.detail}): ${n.text}`));
}

/** The page text for a close or its dry run, by exit code. */
export function closeOutcome(r: ActionResult, team: string, dryRun: boolean): Outcome {
  const cli = cliText(r);
  switch (r.exit) {
    case null:
      return { ok: false, text: `outcome unknown: ${statusHint(team)}` };
    case 0:
      return { ok: true, text: dryRun ? "preview: nothing has changed yet" : "closed" };
    case 1:
      return { ok: false, text: `refused: ${cli}` };
    case 2:
      return { ok: false, text: `the close stopped part way: ${cli}` };
    case 3:
      return { ok: false, text: `teamwire refused the arguments (a bug in the dashboard): ${cli}` };
    case 6:
      return { ok: false, text: `store error: ${cli}` };
    default:
      return { ok: false, text: `teamwire close exited ${r.exit}: ${cli}` };
  }
}

export interface CloseFlags {
  stop: boolean;
  unverified: boolean;
}
export type CloseFlag = keyof CloseFlags;

const RISKS: Record<CloseFlag, string> = {
  stop: "--stop ends a running turn and loses that turn's work.",
  unverified: "--unverified removes a member whose work no verify has passed.",
};

/** One second confirm per ticked flag, naming what it risks. */
export function closeRisks(flags: CloseFlags): { flag: CloseFlag; text: string }[] {
  return (["stop", "unverified"] as const).filter((f) => flags[f]).map((flag) => ({ flag, text: RISKS[flag] }));
}

/** The real close runs only after a dry run that worked with exactly these flags, and a confirm for each risky flag. */
export function canClose(preview: { flags: CloseFlags; result: ActionResult } | null, flags: CloseFlags, confirmed: readonly CloseFlag[]): boolean {
  if (preview === null || preview.result.exit !== 0) return false;
  if (preview.flags.stop !== flags.stop || preview.flags.unverified !== flags.unverified) return false;
  return closeRisks(flags).every((r) => confirmed.includes(r.flag));
}

/** The real team close needs a dry run that worked and the team's name typed exactly; the server checks the name again. */
export function canCloseTeam(preview: ActionResult | null, typed: string, team: string): boolean {
  return preview !== null && preview.exit === 0 && typed === team;
}
