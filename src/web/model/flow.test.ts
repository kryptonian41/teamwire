import { expect, test } from "bun:test";
import { LogView } from "../../shared/teamwire.ts";
import log from "../../../fixtures/log.json";
import { buildFlow, filterByPair, layoutFlow, worseOf } from "./flow.ts";

const L = LogView.parse(log);

test("nodes are the lead, each role and every external party", () => {
  const g = buildFlow(L);
  const ids = g.nodes.map((n) => `${n.kind}:${n.id}`).sort();
  expect(ids).toContain("lead:lead");
  expect(ids).toContain("external:api@billing");
  expect(ids).toContain("external:db@billing");
  expect(ids).toContain("member:fraud");
});

test("an edge counts messages and keeps the worst delivery state", () => {
  const g = buildFlow(L);
  const e = (from: string, to: string) => g.edges.find((x) => x.from === from && x.to === to);
  expect(e("lead", "ledger")?.count).toBe(2);
  expect(e("lead", "ledger")?.worst).toBe("acked");
  // m002 reached web as handed_off; m004 as pending: pending is worse.
  expect(e("lead", "web")?.worst).toBe("handed_off");
  expect(e("ledger", "web")?.worst).toBe("pending");
  expect(g.edges.reduce((n, x) => n + x.count, 0)).toBe(L.messages.reduce((n, m) => n + m.to.length, 0));
});

test("worst-first order", () => {
  expect(worseOf("acked", "undelivered")).toBe("undelivered");
  expect(worseOf("handed_off", "pending")).toBe("pending");
  expect(worseOf("unconfirmed", "pending")).toBe("unconfirmed");
});

test("extra roles appear even with no messages", () => {
  const g = buildFlow({ messages: [] }, ["a", "b"]);
  expect(g.nodes.map((n) => n.id)).toEqual(["lead", "a", "b"]);
});

test("layout puts the lead at the centre and keeps everything inside the box", () => {
  const lay = layoutFlow(buildFlow(L));
  const lead = lay.nodes.find((n) => n.id === "lead")!;
  expect([lead.x, lead.y]).toEqual([lay.width / 2, lay.height / 2]);
  const dist = (n: { x: number; y: number }) => Math.hypot(n.x - lead.x, n.y - lead.y);
  const members = lay.nodes.filter((n) => n.kind === "member");
  const externals = lay.nodes.filter((n) => n.kind === "external");
  for (const m of members) expect(dist(m)).toBeCloseTo(dist(members[0]!), 5);
  for (const x of externals) expect(dist(x)).toBeGreaterThan(dist(members[0]!));
  for (const n of lay.nodes) {
    expect(n.x).toBeGreaterThan(0);
    expect(n.x).toBeLessThan(lay.width);
    expect(n.y).toBeGreaterThan(0);
    expect(n.y).toBeLessThan(lay.height);
  }
});

test("filters the log to one sender and recipient pair", () => {
  const got = filterByPair(L.messages, "lead", "web");
  expect(got.map((m) => m.display_id)).toEqual(["m002", "m008"]);
  expect(filterByPair(L.messages, "ledger", "web").map((m) => m.display_id)).toEqual(["m004"]);
  expect(filterByPair(L.messages, "web", "ledger")).toEqual([]);
});

test("labels sit outside their node, and 18 members keep their circles apart", () => {
  const roles = Array.from({ length: 18 }, (_, i) => `role${i}`);
  const lay = layoutFlow(buildFlow(L, roles));
  for (const n of lay.nodes) {
    expect(Math.hypot(n.label.x - n.x, n.label.y - n.y)).toBeGreaterThan(lay.r);
  }
  for (const a of lay.nodes) {
    for (const b of lay.nodes) {
      if (a !== b) expect(Math.hypot(a.x - b.x, a.y - b.y)).toBeGreaterThan(2 * lay.r + 8);
    }
  }
  expect(lay.r).toBeLessThan(22);
});

test("a label never lands on another node", () => {
  const roles = Array.from({ length: 18 }, (_, i) => `role${i}`);
  const lay = layoutFlow(buildFlow(L, roles));
  for (const n of lay.nodes) {
    for (const o of lay.nodes) {
      if (o !== n) expect(Math.hypot(n.label.x - o.x, n.label.y - o.y)).toBeGreaterThan(lay.r);
    }
  }
});
