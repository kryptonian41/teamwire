import { describe, expect, test } from "bun:test";
import type { ActionResult } from "../../shared/api.ts";
import { canClose, canCloseTeam, canMessage, canReprompt, closeOutcome, closeRisks, leadNoteLines, replyPrefix, replyTarget, sendOutcome } from "./actions.ts";

const r = (exit: number | null, stderr = "", stdout = ""): ActionResult => ({ exit, stdout, stderr, leadNotes: [] });

describe("canMessage", () => {
  test("only implementers and investigators get message controls (plan D4)", () => {
    expect(canMessage("implementer", false)).toBe(true);
    expect(canMessage("investigator", false)).toBe(true);
    for (const kind of ["reviewer", "closer", "plan_reviewer", null] as const) expect(canMessage(kind, false)).toBe(false);
  });
  test("a reconciler, and a member of any kind whose roster says quiet, get none", () => {
    expect(canMessage("reconciler", false)).toBe(false);
    for (const kind of ["implementer", "investigator", "reviewer", "closer", "plan_reviewer", "reconciler", null] as const) {
      expect(canMessage(kind, true)).toBe(false);
    }
  });
});

describe("sendOutcome", () => {
  test("exit 0 is sent", () => {
    expect(sendOutcome(r(0, "", "m009"), "alpha")).toEqual({ ok: true, text: "sent" });
  });
  test("exit 0 with a lead copy line on stderr says the lead's copy was not delivered", () => {
    const res = sendOutcome(r(0, "NOT DELIVERED to lead: not_at_input_prompt\nlead copy not delivered: m009; the lead's hook lists it at the lead's next tool call"), "alpha");
    expect(res).toEqual({ ok: true, text: "sent; the lead's copy was not delivered" });
  });
  test("a line that only mentions the phrase mid-line does not count", () => {
    expect(sendOutcome(r(0, "note: lead copy not delivered: never"), "alpha").text).toBe("sent");
  });
  test("each other exit code has its own text, with the CLI's text where the table says so", () => {
    expect(sendOutcome(r(1, "REFUSED: recipient_not_allowed: rev is a reviewer"), "alpha")).toEqual({ ok: false, text: "refused: REFUSED: recipient_not_allowed: rev is a reviewer" });
    expect(sendOutcome(r(3, "error: unexpected argument"), "alpha")).toEqual({ ok: false, text: "teamwire refused the arguments (a bug in the dashboard): error: unexpected argument" });
    expect(sendOutcome(r(4, "NOT DELIVERED to impl: gone"), "alpha")).toEqual({ ok: false, text: "not delivered" });
    expect(sendOutcome(r(5), "alpha")).toEqual({ ok: false, text: "unconfirmed: check `teamwire status alpha` before sending again" });
    expect(sendOutcome(r(6, "store busy"), "alpha")).toEqual({ ok: false, text: "store error: store busy" });
    expect(sendOutcome(r(9, "odd"), "alpha")).toEqual({ ok: false, text: "teamwire send exited 9: odd" });
  });
  test("the CLI's text falls back to stdout when stderr is empty", () => {
    expect(sendOutcome(r(1, "", "REFUSED on stdout"), "alpha").text).toBe("refused: REFUSED on stdout");
  });
  test("a time limit that passed is outcome unknown", () => {
    expect(sendOutcome(r(null), "alpha")).toEqual({ ok: false, text: "outcome unknown: check `teamwire status alpha`" });
  });
});

describe("Reply with a note (plan D11)", () => {
  const members = [
    { role: "impl", kind: "implementer" as const },
    { role: "inv", kind: "investigator" as const },
    { role: "rev", kind: "reviewer" as const },
  ];
  test("the text starts with re <question id>: ", () => expect(replyPrefix("m014")).toBe("re m014: "));
  test("offered only when the asker is an implementer or investigator of the team", () => {
    expect(replyTarget("impl", members)).toBe("impl");
    expect(replyTarget("inv", members)).toBe("inv");
    expect(replyTarget("rev", members)).toBeNull();
    expect(replyTarget("lead", members)).toBeNull();
    expect(replyTarget("impl@other", members)).toBeNull();
    expect(replyTarget("ghost", members)).toBeNull();
  });
  test("never offered to a quiet asker", () => {
    expect(replyTarget("inv", [{ role: "inv", kind: "investigator", quiet: true }])).toBeNull();
    expect(replyTarget("rec", [{ role: "rec", kind: "reconciler" }])).toBeNull();
  });
});

describe("leadNoteLines", () => {
  test("says plainly whether the lead was told", () => {
    expect(
      leadNoteLines([
        { text: "user is closing team alpha from the dashboard", told: true, detail: "" },
        { text: "user's close of team alpha was refused: busy", told: false, detail: "teamwire send exited 1: team is closing" },
      ]),
    ).toEqual(["The lead was told: user is closing team alpha from the dashboard", "The lead was not told (teamwire send exited 1: team is closing): user's close of team alpha was refused: busy"]);
  });
});

describe("closeOutcome", () => {
  test("each exit code of a close has its own text", () => {
    expect(closeOutcome(r(0, "", "[dry-run] remove impl"), "alpha", true)).toEqual({ ok: true, text: "preview: nothing has changed yet" });
    expect(closeOutcome(r(0), "alpha", false)).toEqual({ ok: true, text: "closed" });
    expect(closeOutcome(r(1, "REFUSED: impl is busy; add --stop"), "alpha", false)).toEqual({ ok: false, text: "refused: REFUSED: impl is busy; add --stop" });
    expect(closeOutcome(r(2, "remove_worktree: kept"), "alpha", false)).toEqual({ ok: false, text: "the close stopped part way: remove_worktree: kept" });
    expect(closeOutcome(r(3, "error: unexpected"), "alpha", true).text).toBe("teamwire refused the arguments (a bug in the dashboard): error: unexpected");
    expect(closeOutcome(r(6, "store busy"), "alpha", true).text).toBe("store error: store busy");
    expect(closeOutcome(r(null), "alpha", false)).toEqual({ ok: false, text: "outcome unknown: check `teamwire status alpha`" });
  });
});

describe("closing one member", () => {
  const none = { stop: false, unverified: false };
  test("each ticked flag names its risk", () => {
    expect(closeRisks(none)).toEqual([]);
    expect(closeRisks({ stop: true, unverified: true })).toEqual([
      { flag: "stop", text: "--stop ends a running turn and loses that turn's work." },
      { flag: "unverified", text: "--unverified removes a member whose work no verify has passed." },
    ]);
  });
  const preview = (exit: number, flags = none) => ({ flags, result: r(exit) });
  test("the real close needs a preview that worked, with the same flags", () => {
    expect(canClose(null, none, [])).toBe(false);
    expect(canClose(preview(1), none, [])).toBe(false);
    expect(canClose(preview(0), none, [])).toBe(true);
    expect(canClose(preview(0), { stop: true, unverified: false }, ["stop"])).toBe(false);
  });
  test("and a second confirm for each ticked flag", () => {
    const both = { stop: true, unverified: true };
    expect(canClose(preview(0, both), both, [])).toBe(false);
    expect(canClose(preview(0, both), both, ["stop"])).toBe(false);
    expect(canClose(preview(0, both), both, ["stop", "unverified"])).toBe(true);
  });
});

describe("closing the team", () => {
  test("needs a dry run that worked and the team's name typed exactly", () => {
    expect(canCloseTeam(null, "alpha", "alpha")).toBe(false);
    expect(canCloseTeam(r(1), "alpha", "alpha")).toBe(false);
    expect(canCloseTeam(r(0), "alph", "alpha")).toBe(false);
    expect(canCloseTeam(r(0), " alpha", "alpha")).toBe(false);
    expect(canCloseTeam(r(0), "alpha", "alpha")).toBe(true);
  });
});

describe("canReprompt", () => {
  test("a reprompt is offered only to a running implementer or investigator, as the CLI's rule says", () => {
    expect(canReprompt("implementer", "started", false)).toBe(true);
    expect(canReprompt("investigator", "creating", false)).toBe(true);
    expect(canReprompt("implementer", "failed", false)).toBe(false);
    expect(canReprompt("implementer", "removed", false)).toBe(false);
    expect(canReprompt("implementer", null, false)).toBe(false);
    expect(canReprompt("reviewer", "started", false)).toBe(false);
  });
  test("no reprompt for a running reconciler or a running quiet investigator", () => {
    expect(canReprompt("reconciler", "started", false)).toBe(false);
    expect(canReprompt("investigator", "started", true)).toBe(false);
  });
});
