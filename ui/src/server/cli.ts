// The only place the server starts `teamwire`. The allow-list below is the
// complete set of argument forms from AGENTS.md; nothing else can be built.

import type { z } from "zod";
import type { Problem } from "../shared/api.ts";
import {
  DecisionsView,
  EventsView,
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
  | { cmd: "events"; team: string; after?: number }
  | { cmd: "eventsArchive"; team: string; archive: string }
  | { cmd: "metrics" }
  | { cmd: "metricsTeam"; team: string }
  | { cmd: "decisionsTeam"; team: string }
  /** Searches every team and archive. `text` is 1 to 200 characters and never starts with `-`. */
  | { cmd: "decisionsGrep"; text: string }
  /** `ref` is `<provider>:<id>`. It reaches the CLI as one `--ref=` item. */
  | { cmd: "decisionsRef"; ref: string }
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

/** The plan's ticket-ref rule, the same one launch uses: a lowercase provider, then an id without white space or control characters. */
const REF_RE = /^[a-z][a-z0-9_]{0,15}:[^\s\p{Cc}]{1,200}$/u;
/** A search text is one printable line, so it cannot hold a newline or a NUL (which `spawn` would refuse). */
const GREP_RE = /^[^\p{Cc}]{1,200}$/u;

const IDENTITY_VARS = ["TEAMWIRE_TOKEN", "TEAMWIRE_TOKEN_FILE", "TEAMWIRE_ROLE", "TEAMWIRE_TEAM"];

/** The archive directory names `teamwire events --archive` accepts for `team`. */
const archiveNameRe = (team: string) => new RegExp(`^${team}-\\d{4}-\\d{2}-\\d{2}(-\\d{6}(-[1-9]\\d*)?)?$`);

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
    case "events": {
      const team = name("team", call.team);
      if (call.after === undefined) return [...prefix, "events", team, "--json"];
      if (!Number.isSafeInteger(call.after) || call.after < 0) throw new Error("bad after value");
      return [...prefix, "events", team, "--after", String(call.after), "--json"];
    }
    case "eventsArchive": {
      const team = name("team", call.team);
      if (!archiveNameRe(team).test(call.archive)) throw new Error("bad archive name");
      return [...prefix, "events", team, "--archive", call.archive, "--json"];
    }
    case "metrics":
      return [...prefix, "metrics", "--json"];
    case "metricsTeam":
      return [...prefix, "metrics", name("team", call.team), "--json"];
    case "decisionsTeam":
      return [...prefix, "decisions", name("team", call.team), "--json"];
    case "decisionsGrep":
      // A leading dash would read as a flag, so it is refused.
      if (!GREP_RE.test(call.text) || call.text.startsWith("-")) throw new Error("bad search text");
      return [...prefix, "decisions", "--grep", call.text, "--json"];
    case "decisionsRef":
      // `--ref=` keeps an id that starts with a dash from reading as a flag.
      if (!REF_RE.test(call.ref)) throw new Error("bad ref");
      return [...prefix, "decisions", `--ref=${call.ref}`, "--json"];
    case "version":
      return [...prefix, "version"];
  }
}

/** A search reads every team and archive, so it gets the long limit. */
export function timeoutFor(call: CliCall): number {
  switch (call.cmd) {
    case "status":
    case "statusRole":
    case "decisionsGrep":
    case "decisionsRef":
      return STATUS_TIMEOUT_MS;
    default:
      return DEFAULT_TIMEOUT_MS;
  }
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
    case "events":
    case "eventsArchive":
      return EventsView;
    case "metrics":
    case "metricsTeam":
      return MetricsView;
    case "decisionsTeam":
    case "decisionsGrep":
    case "decisionsRef":
      return DecisionsView;
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
  events: z.infer<typeof EventsView>;
  eventsArchive: z.infer<typeof EventsView>;
  metrics: z.infer<typeof MetricsView>;
  metricsTeam: z.infer<typeof MetricsView>;
  decisionsTeam: z.infer<typeof DecisionsView>;
  decisionsGrep: z.infer<typeof DecisionsView>;
  decisionsRef: z.infer<typeof DecisionsView>;
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

/** The `teamwire` command a call runs, for messages. */
const commandName = (call: CliCall): string =>
  call.cmd === "eventsArchive" ? "events" : call.cmd === "metricsTeam" ? "metrics" : call.cmd.startsWith("decisions") ? "decisions" : call.cmd;

export function createRunner(config: CliConfig, opts: { timeoutMs?: number } = {}): Runner {
  return (async (call: CliCall) => {
    const name = commandName(call);
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
