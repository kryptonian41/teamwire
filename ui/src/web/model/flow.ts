// Turns a team's message log into a graph: who talks to whom, how often, and
// how well the messages arrived.

import type { DeliveryState, LogView } from "../../shared/teamwire.ts";

/** From worst to best. The worst state seen on an edge colours the edge. */
export const DELIVERY_ORDER: readonly DeliveryState[] = ["undelivered", "unconfirmed", "pending", "handed_off", "acked"];

/** `user` is the human user (`teamwire send --from user`, ADR 0040): a sender, never a member. */
export type NodeKind = "lead" | "member" | "external" | "user";

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
  if (id === "user") return "user";
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
  /** Where the label goes: outside the circle, along the line from the centre. */
  label: { x: number; y: number; anchor: "start" | "middle" | "end" };
}

export interface Layout {
  width: number;
  height: number;
  /** Radius of every node circle. */
  r: number;
  nodes: Placed[];
}

const RAD = Math.PI / 180;
const LABEL_GAP = 11;

/**
 * The lead sits in the centre, members on a circle around it, and external
 * parties and the user on an arc outside that circle, across the top. The circle grows
 * and the nodes shrink as members are added, so that neighbours stay apart.
 */
export function layoutFlow(graph: FlowGraph, minWidth = 640, minHeight = 520): Layout {
  const members = graph.nodes.filter((n) => n.kind === "member");
  const externals = graph.nodes.filter((n) => n.kind === "external" || n.kind === "user");
  const r = members.length > 10 ? 12 : 22;
  // Each member needs room for its circle and the label beside it.
  const inner = Math.max(minHeight * 0.27, (members.length * (2 * r + 42)) / (2 * Math.PI));
  const outer = inner + (externals.length > 0 ? r + 95 : 0);
  const reach = Math.max(inner, outer) + r + 95;
  const width = Math.max(minWidth, 2 * reach);
  const height = Math.max(minHeight, 2 * reach);
  const cx = width / 2;
  const cy = height / 2;

  const place = (n: FlowNode, x: number, y: number): Placed => {
    const dx = x - cx;
    const dy = y - cy;
    const len = Math.hypot(dx, dy);
    // The lead has no outward direction, so its label goes below.
    const ux = len === 0 ? 0 : dx / len;
    const uy = len === 0 ? 1 : dy / len;
    const d = r + LABEL_GAP;
    const anchor = ux > 0.35 ? "start" : ux < -0.35 ? "end" : "middle";
    // Text is drawn from its baseline, so labels below a node sit a little lower.
    const ly = y + uy * d + (uy > 0.35 ? 4 : uy < -0.35 ? 0 : 4);
    return { ...n, x, y, label: { x: x + ux * d, y: ly, anchor } };
  };

  const placed: Placed[] = [];
  for (const n of graph.nodes) if (n.kind === "lead") placed.push(place(n, cx, cy));
  members.forEach((n, i) => {
    // Start at the bottom so the top stays free for external parties.
    const a = (90 + (360 * i) / members.length) * RAD;
    placed.push(place(n, cx + inner * Math.cos(a), cy + inner * Math.sin(a)));
  });
  externals.forEach((n, i) => {
    const deg = externals.length === 1 ? -90 : -150 + (120 * i) / (externals.length - 1);
    placed.push(place(n, cx + outer * Math.cos(deg * RAD), cy + outer * Math.sin(deg * RAD)));
  });
  return { width, height, r, nodes: placed };
}

/** The messages sent on one edge: from `from` to a list that includes `to`. */
export function filterByPair<M extends { from: string; to: string[] }>(messages: readonly M[], from: string, to: string): M[] {
  return messages.filter((m) => m.from === from && m.to.includes(to));
}
