// Decides what the connection banner at the top of the page says.

import type { Problem, Snapshot } from "../../shared/api.ts";
import type { Connection } from "../api.ts";

export type BannerKind = "connected" | "connecting" | "reconnecting" | "stale" | "shape_changed" | "forbidden";

export interface Banner {
  kind: BannerKind;
  text: string;
  problems: Problem[];
}

export function secondsSince(iso: string, now: number): number {
  return Math.max(0, Math.round((now - Date.parse(iso)) / 1000));
}

export function bannerFor(snapshot: Snapshot | null, connection: Connection, now: number): Banner {
  if (connection === "forbidden") {
    return {
      kind: "forbidden",
      text: "Not signed in. Open the URL that the server printed in your terminal; it carries the token.",
      problems: [],
    };
  }
  if (snapshot === null) {
    return connection === "reconnecting"
      ? { kind: "reconnecting", text: "Reconnecting to the server…", problems: [] }
      : { kind: "connecting", text: "Connecting…", problems: [] };
  }
  const changed = snapshot.problems.filter((p) => p.code === "shape_changed");
  if (changed.length > 0) {
    return {
      kind: "shape_changed",
      text: "teamwire's output changed shape, so this page may be out of date. Update teamwire-ui.",
      problems: changed,
    };
  }
  if (connection === "reconnecting") {
    return { kind: "reconnecting", text: "Reconnecting to the server… showing the last data received.", problems: [] };
  }
  if (snapshot.stale) {
    return {
      kind: "stale",
      text: `Data is stale. The last good poll was ${secondsSince(snapshot.at, now)} s ago.`,
      problems: snapshot.problems,
    };
  }
  return { kind: "connected", text: `Connected · updated ${secondsSince(snapshot.at, now)} s ago`, problems: [] };
}
