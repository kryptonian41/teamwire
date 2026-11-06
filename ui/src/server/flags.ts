import { homedir } from "node:os";
import { join } from "node:path";
import { DEFAULT_INTERVAL_MS, DEFAULT_PORT } from "../shared/api.ts";

const MIN_INTERVAL_MS = 2000;

export interface Options {
  port: number;
  intervalMs: number;
  home: string;
  bin: string;
  /** Write actions, only with `TW_UI_ACTIONS=1` (plan D8): a plain `bun run start` stays read-only. */
  actions: boolean;
}

export function parseFlags(argv: string[], env: Record<string, string | undefined>): Options {
  const opts: Options = {
    port: DEFAULT_PORT,
    intervalMs: DEFAULT_INTERVAL_MS,
    home: env.TEAMWIRE_HOME || join(homedir(), ".teamwire"),
    bin: "teamwire",
    actions: env.TW_UI_ACTIONS === "1",
  };
  for (let i = 0; i < argv.length; i += 2) {
    const flag = argv[i];
    const value = argv[i + 1];
    if (value === undefined) throw new Error(`${flag} needs a value`);
    switch (flag) {
      case "--port": {
        const n = Number(value);
        if (!Number.isInteger(n) || n < 1 || n > 65535) throw new Error("--port must be 1 to 65535");
        opts.port = n;
        break;
      }
      case "--interval": {
        const n = Number(value);
        if (!Number.isInteger(n) || n < MIN_INTERVAL_MS) throw new Error(`--interval must be at least ${MIN_INTERVAL_MS}`);
        opts.intervalMs = n;
        break;
      }
      case "--home":
        opts.home = value;
        break;
      case "--teamwire":
        opts.bin = value;
        break;
      default:
        throw new Error(`unknown flag ${flag}`);
    }
  }
  return opts;
}
