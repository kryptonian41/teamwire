// Turns a team's message log into a graph: who talks to whom, how often, and
// how well the messages arrived.

import type { DeliveryState, LogView } from "../../shared/teamwire.ts";

/** From worst to best. The worst state seen on an edge colours the edge. */
export const DELIVERY_ORDER: readonly DeliveryState[] = ["undelivered", "unconfirmed", "pending", "handed_off", "acked"];

export type NodeKind = "lead" | "member" | "external";

export interface FlowNode {
  id: string;
  kind: NodeKind;
}

export interface FlowEdge {
  from: string;
  to: string;
  count: number;
  worst: DeliveryState;
}

export interface FlowGraph {
  nodes: FlowNode[];
  edges: FlowEdge[];
}

export const worseOf = (a: DeliveryState, b: DeliveryState): DeliveryState =>
  DELIVERY_ORDER.indexOf(a) <= DELIVERY_ORDER.indexOf(b) ? a : b;

function kindOf(id: string): NodeKind {
  if (id.includes("@")) return "external";
  return id === "lead" ? "lead" : "member";
}

/**
 * `roles` adds members who have sent nothing yet, so that a quiet member still
 * appears in the diagram. The lead is always a node.
 */
export function buildFlow(log: LogView, roles: readonly string[] = []): FlowGraph {
  const nodes = new Map<string, FlowNode>();
  const add = (id: string) => {
    if (!nodes.has(id)) nodes.set(id, { id, kind: kindOf(id) });
  };
  add("lead");
  for (const r of roles) add(r);

  const edges = new Map<string, FlowEdge>();
  for (const msg of log.messages) {
    add(msg.from);
    for (const to of msg.to) {
      add(to);
      const key = `${msg.from}\u0000${to}`;
      // A recipient with no delivery row has not been tried yet.
      const state = msg.deliveries.find((d) => d.to === to)?.state ?? "pending";
      const edge = edges.get(key);
      if (edge) {
        edge.count += 1;
        edge.worst = worseOf(edge.worst, state);
      } else {
        edges.set(key, { from: msg.from, to, count: 1, worst: state });
      }
    }
  }
  return { nodes: [...nodes.values()], edges: [...edges.values()] };
}

export interface Placed extends FlowNode {
  x: number;
  y: number;
}

export interface Layout {
  width: number;
  height: number;
  nodes: Placed[];
}

const RAD = Math.PI / 180;

/**
 * The lead sits in the centre, members on a circle around it, and external
 * parties on an arc outside that circle, across the top.
 */
export function layoutFlow(graph: FlowGraph, width = 640, height = 520): Layout {
  const cx = width / 2;
  const cy = height / 2;
  const members = graph.nodes.filter((n) => n.kind === "member");
  const externals = graph.nodes.filter((n) => n.kind === "external");
  const inner = Math.min(width, height) * 0.27;
  const outer = Math.min(width, height) * 0.44;

  const placed: Placed[] = [];
  for (const n of graph.nodes) {
    if (n.kind === "lead") placed.push({ ...n, x: cx, y: cy });
  }
  members.forEach((n, i) => {
    // Start at the bottom so the top stays free for external parties.
    const a = (90 + (360 * i) / members.length) * RAD;
    placed.push({ ...n, x: cx + inner * Math.cos(a), y: cy + inner * Math.sin(a) });
  });
  externals.forEach((n, i) => {
    const from = -150;
    const to = -30;
    const deg = externals.length === 1 ? -90 : from + ((to - from) * i) / (externals.length - 1);
    placed.push({ ...n, x: cx + outer * Math.cos(deg * RAD), y: cy + outer * Math.sin(deg * RAD) });
  });
  return { width, height, nodes: placed };
}
