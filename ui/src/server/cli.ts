// The only place the server starts `teamwire`. The allow-list below is the
// complete set of argument forms from AGENTS.md; nothing else can be built.

import type { z } from "zod";
import type { Problem } from "../shared/api.ts";
import {
  LogView,
  MetricsView,
  NAME_RE,
  StatusAll,
  StatusView,
  TeamsList,
} from "../shared/teamwire.ts";

export type CliCall =
  | { cmd: "status" }
  | { cmd: "statusRole"; team: string; role: string }
  | { cmd: "teams" }
  | { cmd: "log"; team: string }
  | { cmd: "metrics" }
  | { cmd: "version" };

export interface CliConfig {
  /** The binary to run. */
  bin: string;
  /** Passed as `--home DIR` before the command when set. */
  home?: string | undefined;
}

export type CliResult<T> =
  | { ok: true; data: T; problems: Problem[] }
  | { ok: false; problems: Problem[] };

/** `status` asks Claude Code and herdr about every member, so it is slow. */
export const STATUS_TIMEOUT_MS = 30_000;
export const DEFAULT_TIMEOUT_MS = 10_000;

const IDENTITY_VARS = ["TEAMWIRE_TOKEN", "TEAMWIRE_TOKEN_FILE", "TEAMWIRE_ROLE", "TEAMWIRE_TEAM"];

/** Builds the arguments after the binary. Throws on a name that is not a team or role name. */
export function buildArgs(call: CliCall, home?: string): string[] {
  const prefix = home ? ["--home", home] : [];
  const name = (kind: string, value: string): string => {
    if (!NAME_RE.test(value)) throw new Error(`bad ${kind} name`);
    return value;
  };
  switch (call.cmd) {
    case "status":
      return [...prefix, "status", "--json"];
    case "statusRole":
      return [...prefix, "status", name("team", call.team), "--role", name("role", call.role), "--json"];
    case "teams":
      return [...prefix, "teams", "--json"];
    case "log":
      return [...prefix, "log", name("team", call.team), "--json"];
    case "metrics":
      return [...prefix, "metrics", "--json"];
    case "version":
      return [...prefix, "version"];
  }
}

export function timeoutFor(call: CliCall): number {
  return call.cmd === "status" || call.cmd === "statusRole" ? STATUS_TIMEOUT_MS : DEFAULT_TIMEOUT_MS;
}

function schemaFor(call: CliCall): z.ZodType | null {
  switch (call.cmd) {
    case "status":
      return StatusAll;
    case "statusRole":
      return StatusView;
    case "teams":
      return TeamsList;
    case "log":
      return LogView;
    case "metrics":
      return MetricsView;
    case "version":
      return null;
  }
}

/** Typed result of each call, so callers need no casts. */
export interface CliOutput {
  status: z.infer<typeof StatusAll>;
  statusRole: z.infer<typeof StatusView>;
  teams: z.infer<typeof TeamsList>;
  log: z.infer<typeof LogView>;
  metrics: z.infer<typeof MetricsView>;
  version: string;
}

export type Runner = <C extends CliCall>(call: C) => Promise<CliResult<CliOutput[C["cmd"]]>>;

const READ_GRACE_MS = 2000;
/** The most output of one stream the server keeps. Real output is a few hundred KiB at most. */
export const MAX_OUTPUT_BYTES = 16 * 1024 * 1024;

function collect(stream: ReadableStream<Uint8Array>, onTooLarge: () => void) {
  const reader = stream.getReader();
  const decoder = new TextDecoder();
  let text = "";
  let bytes = 0;
  const done = (async () => {
    try {
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        bytes += value.length;
        if (bytes > MAX_OUTPUT_BYTES) {
          onTooLarge();
          break;
        }
        text += decoder.decode(value, { stream: true });
      }
    } catch {
      // Cancelled after the grace period.
    }
  })();
  return { done, text: () => text, stop: () => void reader.cancel().catch(() => {}) };
}

function firstLine(text: string): string {
  return text.split("\n").find((l) => l.trim() !== "")?.trim() ?? "";
}

function cleanEnv(): Record<string, string> {
  const env: Record<string, string> = {};
  for (const [k, v] of Object.entries(process.env)) {
    if (v !== undefined && !IDENTITY_VARS.includes(k)) env[k] = v;
  }
  return env;
}

export function createRunner(config: CliConfig, opts: { timeoutMs?: number } = {}): Runner {
  return (async (call: CliCall) => {
    const name = call.cmd;
    let args: string[];
    try {
      args = buildArgs(call, config.home);
    } catch (e) {
      return { ok: false, problems: [{ code: "bad_request", detail: (e as Error).message }] };
    }
    const limit = opts.timeoutMs ?? timeoutFor(call);
    let proc: Bun.Subprocess<"ignore", "pipe", "pipe">;
    try {
      proc = Bun.spawn([config.bin, ...args], {
        stdin: "ignore",
        stdout: "pipe",
        stderr: "pipe",
        env: cleanEnv(),
      });
    } catch (e) {
      return { ok: false, problems: [{ code: "cli_failed", detail: `could not start ${config.bin}: ${(e as Error).message}` }] };
    }
    let timedOut = false;
    let tooLarge = false;
    const overflow = () => {
      tooLarge = true;
      proc.kill("SIGKILL");
    };
    const timer = setTimeout(() => {
      timedOut = true;
      proc.kill("SIGKILL");
    }, limit);
    // Read while the child runs so a full pipe cannot block it. After it
    // exits, a process it started may still hold the pipes open, so the
    // reads get a short grace period and are then cancelled.
    const out = collect(proc.stdout, overflow);
    const err = collect(proc.stderr, overflow);
    const exitCode = await proc.exited;
    await Promise.race([Promise.all([out.done, err.done]), Bun.sleep(READ_GRACE_MS)]);
    out.stop();
    err.stop();
    const stdout = out.text();
    const stderr = err.text();
    clearTimeout(timer);
    if (tooLarge) {
      return { ok: false, problems: [{ code: "too_large", detail: `teamwire ${name} printed more than ${MAX_OUTPUT_BYTES} bytes and was killed` }] };
    }
    if (timedOut) {
      return { ok: false, problems: [{ code: "cli_timeout", detail: `teamwire ${name} passed ${limit} ms and was killed` }] };
    }

    const schema = schemaFor(call);
    const failure: Problem[] =
      exitCode === 0
        ? []
        : [{ code: "cli_failed", detail: `teamwire ${name} exited ${exitCode}: ${firstLine(stderr)}` }];

    if (schema === null) {
      if (exitCode !== 0) return { ok: false, problems: failure };
      return { ok: true, data: stdout.trim() as never, problems: [] };
    }

    let json: unknown;
    try {
      json = JSON.parse(stdout);
    } catch {
      const detail = exitCode === 0 ? `teamwire ${name} printed invalid JSON` : undefined;
      return {
        ok: false,
        problems: detail ? [{ code: "shape_changed", detail }] : failure,
      };
    }
    const parsed = schema.safeParse(json);
    if (!parsed.success) {
      const issue = parsed.error.issues[0];
      const where = issue ? `${issue.path.join(".") || "(root)"}: ${issue.message}` : "unknown";
      return { ok: false, problems: [...failure, { code: "shape_changed", detail: `teamwire ${name} output changed shape at ${where}` }] };
    }
    return { ok: true, data: parsed.data as never, problems: failure };
  }) as Runner;
}
