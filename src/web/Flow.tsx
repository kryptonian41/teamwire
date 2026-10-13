import type { StateCategory } from "../shared/state.ts";
import { DELIVERY_CATEGORY } from "./components.tsx";
import { layoutFlow, type FlowGraph, type Placed } from "./model/flow.ts";
import { DELIVERY_ORDER } from "./model/flow.ts";

export interface Selected {
  from: string;
  to: string;
}

/** Who talks to whom. Inline SVG with a viewBox, so it shrinks to 375 px wide. */
export function Flow({
  graph,
  categories,
  selected,
  onSelect,
}: {
  graph: FlowGraph;
  /** Category of each node id. Missing ids are drawn as neutral. */
  categories: ReadonlyMap<string, StateCategory>;
  selected: Selected | null;
  onSelect: (s: Selected | null) => void;
}) {
  const layout = layoutFlow(graph);
  const at = new Map(layout.nodes.map((n) => [n.id, n] as const));
  const pairs = new Set(graph.edges.map((e) => `${e.from}>${e.to}`));
  const maxCount = Math.max(1, ...graph.edges.map((e) => e.count));

  return (
    <>
      <svg className="flow" viewBox={`0 0 ${layout.width} ${layout.height}`} role="img" aria-label="Message flow between members">
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
              {n.id.length > 18 ? `${n.id.slice(0, 17)}…` : n.id}
            </text>
          </g>
        ))}
      </svg>
      <div className="legend" aria-label="Edge colours, worst delivery state">
        {DELIVERY_ORDER.map((s) => (
          <span key={s} className={`chip small cat-${DELIVERY_CATEGORY[s]}`}>
            {s}
          </span>
        ))}
        <span className="sub">Wider line = more messages. Click a line to filter the log.</span>
      </div>
    </>
  );
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
