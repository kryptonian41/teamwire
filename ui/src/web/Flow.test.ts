import { describe, expect, test } from "bun:test";
import { LogView } from "../shared/teamwire.ts";
import logFixture from "../../fixtures/log.json";
import { fitView } from "./Flow.tsx";
import { buildFlow, layoutFlow } from "./model/flow.ts";

describe("fitView", () => {
  const layout = layoutFlow(buildFlow(LogView.parse(logFixture), ["api", "ledger", "web", "emails", "audit", "fraud", "review"]));
  const v = fitView(layout);

  test("holds every circle and every label anchor", () => {
    for (const n of layout.nodes) {
      expect(n.x - layout.r).toBeGreaterThanOrEqual(v.x);
      expect(n.x + layout.r).toBeLessThanOrEqual(v.x + v.width);
      expect(n.y - layout.r).toBeGreaterThanOrEqual(v.y);
      expect(n.y + layout.r).toBeLessThanOrEqual(v.y + v.height);
      expect(n.label.y).toBeGreaterThanOrEqual(v.y);
      expect(n.label.y).toBeLessThanOrEqual(v.y + v.height);
    }
  });

  test("is smaller than the full layout, so the empty bands go", () => {
    expect(v.height).toBeLessThan(layout.height);
  });

  test("a longer label widens the box, so a state word is not cut", () => {
    const wide = fitView(layout, (id) => `${id} · done(done)`);
    expect(wide.width).toBeGreaterThan(v.width);
    for (const n of layout.nodes) {
      const w = `${n.id} · done(done)`.length * 7.5;
      const left = n.label.anchor === "start" ? n.label.x : n.label.anchor === "end" ? n.label.x - w : n.label.x - w / 2;
      expect(left).toBeGreaterThanOrEqual(wide.x);
      expect(left + w).toBeLessThanOrEqual(wide.x + wide.width);
    }
  });

  test("an empty graph keeps the layout's own box", () => {
    const empty = layoutFlow({ nodes: [], edges: [] });
    expect(fitView(empty)).toEqual({ x: 0, y: 0, width: empty.width, height: empty.height });
  });
});
