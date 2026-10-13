import { expect, test } from "bun:test";
import type { Snapshot } from "../../shared/api.ts";
import { bannerFor } from "./banner.ts";

const at = "2026-10-13T07:00:00.000Z";
const now = Date.parse(at) + 7000;
const snap = (over: Partial<Snapshot> = {}): Snapshot => ({
  at,
  durationMs: 1,
  intervalMs: 5000,
  stale: false,
  lastGoodAt: at,
  status: [],
  teams: [],
  problems: [],
  ...over,
});

test("connected shows seconds since the poll", () => {
  const b = bannerFor(snap(), "connected", now);
  expect(b.kind).toBe("connected");
  expect(b.text).toContain("updated 7 s ago");
});

test("forbidden wins over everything", () => {
  expect(bannerFor(snap(), "forbidden", now).kind).toBe("forbidden");
  expect(bannerFor(null, "forbidden", now).text).toContain("URL");
});

test("stale carries the problem details", () => {
  const p = { code: "cli_failed" as const, detail: "teamwire exited 1" };
  const b = bannerFor(snap({ stale: true, problems: [p] }), "connected", now);
  expect(b.kind).toBe("stale");
  expect(b.problems).toEqual([p]);
});

test("stale age counts from the last good poll, not the latest failed one", () => {
  const failedAt = "2026-10-13T08:00:00.000Z";
  const b = bannerFor(snap({ at: failedAt, lastGoodAt: at, stale: true }), "connected", Date.parse(failedAt) + 2000);
  expect(b.text).toContain("3602 s ago");
});

test("stale with no good poll yet says so", () => {
  const b = bannerFor(snap({ stale: true, lastGoodAt: null }), "connected", now);
  expect(b.kind).toBe("stale");
  expect(b.text).toContain("No poll of teamwire has worked yet");
});

test("shape_changed is its own state", () => {
  const p = { code: "shape_changed" as const, detail: "status: bad enum" };
  expect(bannerFor(snap({ stale: true, problems: [p] }), "connected", now).kind).toBe("shape_changed");
});

test("reconnecting and connecting before the first snapshot", () => {
  expect(bannerFor(snap(), "reconnecting", now).kind).toBe("reconnecting");
  expect(bannerFor(null, "connecting", now).kind).toBe("connecting");
  expect(bannerFor(null, "connected", now).kind).toBe("connecting");
  expect(bannerFor(null, "reconnecting", now).kind).toBe("reconnecting");
});
