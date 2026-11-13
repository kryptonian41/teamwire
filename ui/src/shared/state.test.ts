import { expect, test } from "bun:test";
import { classifyState } from "./state.ts";

test.each([
  ["done(done)", "done"],
  ["done(failed)", "failed"],
  ["done(blocked)", "attention"],
  ["working", "active"],
  ["working (at a dialog)", "active"],
  ["not started", "active"],
  ["quiet", "idle"],
  ["late", "attention"],
  ["blocked", "attention"],
  ["died?", "attention"],
  ["unreported", "attention"],
  ["undelivered", "attention"],
  ["unconfirmed", "attention"],
  ["prompted outside teamwire", "attention"],
  ["failed to start", "failed"],
  ["snoozing", "unknown"],
])("%s is %s", (word, category) => {
  expect(classifyState(word)).toBe(category as ReturnType<typeof classifyState>);
});
