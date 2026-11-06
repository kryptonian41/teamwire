import { describe, expect, test } from "bun:test";
import type { ActionResult } from "../../shared/api.ts";
import { canMessage, leadNoteLines, replyPrefix, replyTarget, sendOutcome } from "./actions.ts";

const r = (exit: number | null, stderr = "", stdout = ""): ActionResult => ({ exit, stdout, stderr, leadNotes: [] });

describe("canMessage", () => {
  test("only implementers and investigators get message controls (plan D4)", () => {
    expect(canMessage("implementer")).toBe(true);
    expect(canMessage("investigator")).toBe(true);
    for (const kind of ["reviewer", "closer", "plan_reviewer", null] as const) expect(canMessage(kind)).toBe(false);
  });
});

describe("sendOutcome", () => {
  test("exit 0 is sent", () => {
    expect(sendOutcome(r(0, "", "m009"), "alpha")).toEqual({ ok: true, text: "sent" });
  });
  test("exit 0 with a lead copy line on stderr says the lead's copy was not delivered", () => {
    const res = sendOutcome(r(0, "NOT DELIVERED to lead: not_at_input_prompt\nlead copy not delivered: m009; run teamwire resend alpha m009 --to lead"), "alpha");
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
