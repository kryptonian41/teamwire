import type { StateCategory } from "../shared/state.ts";
import { DELIVERY_CATEGORY, StateChip } from "./components.tsx";
import { layoutFlow, type FlowGraph, type Layout, type Placed } from "./model/flow.ts";
import { DELIVERY_ORDER } from "./model/flow.ts";

export interface Selected {
  from: string;
  to: string;
}

/** Who talks to whom. Inline SVG with a viewBox, so it shrinks to 375 px wide. */
export function Flow({
  graph,
  categories,
  states,
  selected,
  onSelect,
}: {
  graph: FlowGraph;
  /** Category of each node id. Missing ids are drawn as neutral. */
  categories: ReadonlyMap<string, StateCategory>;
  /** State word of each node id, shown in its label so colour is never the only sign. */
  states: ReadonlyMap<string, string>;
  selected: Selected | null;
  onSelect: (s: Selected | null) => void;
}) {
  const layout = layoutFlow(graph);
  const at = new Map(layout.nodes.map((n) => [n.id, n] as const));
  const pairs = new Set(graph.edges.map((e) => `${e.from}>${e.to}`));
  const maxCount = Math.max(1, ...graph.edges.map((e) => e.count));
  const label = (id: string) => (states.has(id) ? `${id} · ${states.get(id)}` : id);
  const view = fitView(layout, label);

  return (
    <div className="flow-panel">
      <svg
        className="flow"
        viewBox={`${view.x} ${view.y} ${view.width} ${view.height}`}
        width={view.width}
        role="img"
        aria-label="Message flow between members"
      >
        <defs>
          <marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerUnits="userSpaceOnUse" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
            <path d="M0 0L10 5L0 10z" fill="context-stroke" />
          </marker>
        </defs>
        {graph.edges.map((e) => {
          const a = at.get(e.from);
          const b = at.get(e.to);
          if (!a || !b || a === b) return null;
          const both = pairs.has(`${e.to}>${e.from}`);
          const d = edgePath(a, b, both, layout.r);
          const width = 1 + (3 * e.count) / maxCount;
          const isSel = selected?.from === e.from && selected?.to === e.to;
          return (
            <path
              key={`${e.from}>${e.to}`}
              className={`edge cat-${DELIVERY_CATEGORY[e.worst]}${isSel ? " selected" : ""}`}
              d={d}
              strokeWidth={width}
              strokeOpacity={selected && !isSel ? 0.3 : 0.75}
              markerEnd="url(#arrow)"
              onClick={() => onSelect(isSel ? null : { from: e.from, to: e.to })}
            >
              <title>
                {e.from} → {e.to}: {e.count} message{e.count === 1 ? "" : "s"}, worst delivery {e.worst}
              </title>
            </path>
          );
        })}
        {layout.nodes.map((n) => (
          <g key={n.id} className={`node cat-${categories.get(n.id) ?? "idle"}${n.kind === "external" ? " external" : ""}`}>
            <title>
              {n.id} ({n.kind === "external" ? "other team" : n.kind}
              {categories.has(n.id) ? `, ${categories.get(n.id)}` : ""})
            </title>
            <circle cx={n.x} cy={n.y} r={layout.r} />
            <text x={n.label.x} y={n.label.y} textAnchor={n.label.anchor}>
              {label(n.id).length > LABEL_MAX ? `${label(n.id).slice(0, LABEL_MAX - 1)}…` : label(n.id)}
            </text>
          </g>
        ))}
      </svg>
      <div className="legend" role="group" aria-label="Edge colours, worst delivery state">
        {DELIVERY_ORDER.map((s) => (
          <StateChip key={s} word={s} category={DELIVERY_CATEGORY[s]} />
        ))}
        <span className="sub">Wider line = more messages. Click a line to filter the log.</span>
      </div>
    </div>
  );
}

/** Label text is 13px; this is a generous average glyph width for the system face. */
const CHAR_W = 7.5;
const LABEL_MAX = 30;
const PAD = 16;

/**
 * The box that holds every circle and label, plus a margin. The layout keeps
 * room for nodes that may not exist, so drawing its full size leaves wide
 * empty bands; the fitted box keeps the diagram at its natural scale.
 */
export function fitView(layout: Layout, label: (id: string) => string = (id) => id): { x: number; y: number; width: number; height: number } {
  if (layout.nodes.length === 0) return { x: 0, y: 0, width: layout.width, height: layout.height };
  let x0 = Infinity;
  let y0 = Infinity;
  let x1 = -Infinity;
  let y1 = -Infinity;
  for (const n of layout.nodes) {
    const w = Math.min(label(n.id).length, LABEL_MAX) * CHAR_W;
    const lx = n.label.anchor === "start" ? n.label.x : n.label.anchor === "end" ? n.label.x - w : n.label.x - w / 2;
    x0 = Math.min(x0, n.x - layout.r, lx);
    x1 = Math.max(x1, n.x + layout.r, lx + w);
    // A label's y is its baseline: the text rises about 13px above it.
    y0 = Math.min(y0, n.y - layout.r, n.label.y - 13);
    y1 = Math.max(y1, n.y + layout.r, n.label.y + 4);
  }
  const r = (v: number) => Math.round(v);
  return { x: r(x0 - PAD), y: r(y0 - PAD), width: r(x1 - x0 + 2 * PAD), height: r(y1 - y0 + 2 * PAD) };
}

/** A line from the edge of one circle to the edge of the other; curved when both directions exist. */
function edgePath(a: Placed, b: Placed, curved: boolean, R: number): string {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const len = Math.hypot(dx, dy) || 1;
  const ux = dx / len;
  const uy = dy / len;
  const off = curved ? 14 : 0;
  // The offset moves the line sideways, so the two directions do not overlap.
  const nx = -uy * off;
  const ny = ux * off;
  const x1 = a.x + ux * R + nx;
  const y1 = a.y + uy * R + ny;
  const x2 = b.x - ux * (R + 2) + nx;
  const y2 = b.y - uy * (R + 2) + ny;
  return `M${x1.toFixed(1)} ${y1.toFixed(1)}L${x2.toFixed(1)} ${y2.toFixed(1)}`;
}
