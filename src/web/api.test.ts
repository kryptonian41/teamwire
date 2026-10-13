import { expect, test } from "bun:test";
import { api } from "../shared/api.ts";
import { mockClient, mockSnapshot, selectClient, httpClient } from "./api.ts";

test("mock snapshot advances member ages and keeps the fixture shape", () => {
  const a = mockSnapshot(0);
  const b = mockSnapshot(30);
  expect(b.status[0]!.members[0]!.age_s).toBe(a.status[0]!.members[0]!.age_s + 30);
  expect(a.stale).toBe(false);
  expect(a.lastGoodAt).toBe(a.at);
  expect(a.teams.length).toBe(2);
});

test("?mock=1 selects the mock client", () => {
  expect(selectClient("?mock=1")).toBe(mockClient);
  expect(selectClient("")).toBe(httpClient);
});

test("mock subscribe sends a snapshot at once and stops on unsubscribe", () => {
  const seen: string[] = [];
  const stop = mockClient.subscribe(
    (s) => seen.push(s.at),
    (c) => seen.push(c),
  );
  stop();
  expect(seen.length).toBe(2);
  expect(seen[1]).toBe("connected");
});

test("mock files list a comms file per logged message", async () => {
  const files = await mockClient.teamFiles("payments");
  expect(files.filter((f) => f.kind === "comms").length).toBeGreaterThan(0);
  expect(files.some((f) => f.kind === "outbox" && f.name === "api")).toBe(true);
  expect(api.teamFile("payments", "comms", "m001")).toBe("/api/teams/payments/file/comms/m001");
});
