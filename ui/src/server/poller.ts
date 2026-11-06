import type { Problem, Snapshot } from "../shared/api.ts";
import type { Runner } from "./cli.ts";

export interface PollerDeps {
  run: Runner;
  intervalMs: number;
  /** Copied into every snapshot, so the page knows whether to show action controls. */
  actions?: boolean;
  /** Milliseconds since the epoch. Injected so tests need no real time. */
  now?: () => number;
}

export interface Poller {
  /** Runs one poll. Returns null when another poll is still running. */
  poll(): Promise<Snapshot | null>;
  latest(): Snapshot | null;
  subscribe(fn: (s: Snapshot) => void): () => void;
  /** Polls now and then every interval. Returns a function that stops it. */
  start(): () => void;
}

export function createPoller(deps: PollerDeps): Poller {
  const now = deps.now ?? Date.now;
  const subscribers = new Set<(s: Snapshot) => void>();
  let latest: Snapshot | null = null;
  let running = false;
  let lastGoodAt: string | null = null;

  async function poll(): Promise<Snapshot | null> {
    if (running) return null;
    running = true;
    try {
      const started = now();
      const status = await deps.run({ cmd: "status" });
      const teams = await deps.run({ cmd: "teams" });
      const finished = now();
      const problems: Problem[] = [...status.problems, ...teams.problems];
      // A part that failed keeps its last good data, so one slow command does
      // not blank the page.
      const stale = !status.ok || !teams.ok;
      if (!stale) lastGoodAt = new Date(finished).toISOString();
      const snapshot: Snapshot = {
        at: new Date(finished).toISOString(),
        durationMs: finished - started,
        intervalMs: deps.intervalMs,
        stale,
        lastGoodAt,
        status: status.ok ? status.data : (latest?.status ?? []),
        teams: teams.ok ? teams.data.teams : (latest?.teams ?? []),
        problems,
        actions: deps.actions ?? false,
      };
      latest = snapshot;
      for (const fn of subscribers) {
        try {
          fn(snapshot);
        } catch {
          // A broken subscriber must not stop the others.
        }
      }
      return snapshot;
    } finally {
      running = false;
    }
  }

  return {
    poll,
    latest: () => latest,
    subscribe(fn) {
      subscribers.add(fn);
      return () => subscribers.delete(fn);
    },
    start() {
      void poll();
      const timer = setInterval(() => void poll(), deps.intervalMs);
      return () => clearInterval(timer);
    },
  };
}
