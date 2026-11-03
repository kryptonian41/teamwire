//! The pi extension a member loads with `-e`: pi has no hook commands, so
//! this file runs teamwire's member hooks from inside the pi process. The
//! lead's extension (`lead_extension.rs`) shares its state-file and hook
//! code, `SHARED`.
//!
//! The text is a fixed template with one JSON value spliced in. JSON is valid
//! JavaScript, so no path or name can break out of the literal. The file
//! holds no secret: it names the token file, never the token.

use serde_json::{Value, json};

/// The `customType` of the session entry that holds pi's context reading.
pub(crate) const FILL_ENTRY: &str = "teamwire.fill";
/// The end of the name of the file that names a session's pi process and
/// its state: `<state_dir>/<session id>.teamwire.json`.
pub(crate) const STATE_SUFFIX: &str = ".teamwire.json";
/// Where a template's JSON value goes.
pub(crate) const CONFIG_SLOT: &str = "__TEAMWIRE_CONFIG__";

/// What the template needs about one member.
pub(crate) struct Values<'a> {
    pub(crate) teamwire_bin: &'a str,
    pub(crate) home: Option<&'a str>,
    pub(crate) team: &'a str,
    pub(crate) role: &'a str,
    pub(crate) token_file: &'a str,
    /// Where the extension writes each session's state file.
    pub(crate) state_dir: &'a str,
    pub(crate) hook_timeout_ms: u64,
}

/// `--home <home>` when a home is given, else nothing.
pub(crate) fn home_args(home: Option<&str>) -> Vec<Value> {
    home.map(|h| vec!["--home".into(), h.into()])
        .unwrap_or_default()
}

pub(crate) fn render(v: &Values<'_>) -> String {
    let mut env = serde_json::Map::new();
    env.insert("TEAMWIRE_TEAM".into(), v.team.into());
    env.insert("TEAMWIRE_ROLE".into(), v.role.into());
    if let Some(h) = v.home {
        env.insert("TEAMWIRE_HOME".into(), h.into());
    }
    env.insert("TEAMWIRE_TOKEN_FILE".into(), v.token_file.into());
    let config = json!({
        "bin": v.teamwire_bin,
        "homeArgs": home_args(v.home),
        "team": v.team,
        "role": v.role,
        "env": env,
        "hookTimeoutMs": v.hook_timeout_ms,
        "stateDir": v.state_dir,
        "stateSuffix": STATE_SUFFIX,
        "fillEntry": FILL_ENTRY,
    });
    [HEAD, IMPORTS, MEMBER_ENV, SHARED, MEMBER_MAIN]
        .concat()
        .replace(CONFIG_SLOT, &config.to_string())
}

const HEAD: &str = "// Written by teamwire launch for one member. Launch writes it again at each
// start, so an edit here does not last.
";

/// The imports both extensions need, and the line that holds the config.
pub(crate) const IMPORTS: &str = r#"import { spawn } from "node:child_process";
import { mkdirSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

const TW = __TEAMWIRE_CONFIG__;
"#;

const MEMBER_ENV: &str = "
// pi's bash tool copies process.env for each command, so this reaches every
// shell command the member runs.
Object.assign(process.env, TW.env);
";

/// What both extensions do the same way: the state file, a hook run, and a
/// hold at the end of a run. Each hook's failure lets the turn go on, as a
/// failed hook command does for Claude Code: teamwire's hook prints nothing
/// on error.
pub(crate) const SHARED: &str = r#"
// When this process started, from the epoch, for teamwire's pid check.
const startedAtMs = Date.now() - Math.round(process.uptime() * 1000);

// The state file of the session in this process. pi writes its own session
// file only at the first message, so teamwire finds the process through this
// file from the start. A write replaces the whole file through a rename, so a
// reader never sees half of one.
let sessionId: string | undefined;
let running = false;
// What a lead adds to its state file. A member adds nothing.
let stateExtra: Record<string, unknown> = {};
function writeState(state: string) {
  if (!sessionId) return;
  try {
    mkdirSync(TW.stateDir, { recursive: true });
    const file = join(TW.stateDir, sessionId + TW.stateSuffix);
    const tmp = `${file}.${process.pid}.tmp`;
    writeFileSync(
      tmp,
      JSON.stringify({ pid: process.pid, started_at_ms: startedAtMs, state, ...stateExtra }),
    );
    renameSync(tmp, file);
  } catch {
    // A state that cannot be written leaves the session's liveness unknown.
  }
}

// Runs teamwire with `argv` and `input` on stdin; resolves to its JSON reply,
// or undefined when it printed nothing, failed or ran out of time.
function runHook(argv: string[], input: unknown): Promise<any> {
  return new Promise((resolve) => {
    let out = "";
    let settled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const finish = (value: any) => {
      if (settled) return;
      settled = true;
      if (timer) clearTimeout(timer);
      resolve(value);
    };
    let child;
    try {
      child = spawn(TW.bin, argv, {
        env: { ...process.env, ...TW.env },
        stdio: ["pipe", "pipe", "ignore"],
      });
    } catch {
      finish(undefined);
      return;
    }
    timer = setTimeout(() => {
      child.kill("SIGKILL");
      finish(undefined);
    }, TW.hookTimeoutMs);
    child.stdout.on("data", (chunk: Buffer) => {
      out += chunk.toString("utf8");
    });
    child.on("error", () => finish(undefined));
    child.on("close", () => {
      try {
        finish(out.trim() ? JSON.parse(out) : undefined);
      } catch {
        finish(undefined);
      }
    });
    child.stdin.on("error", () => {});
    child.stdin.end(JSON.stringify(input));
  });
}

// The text of the newest assistant message in `messages`, if it has any.
function finalText(messages: any[]): string | undefined {
  for (let i = messages.length - 1; i >= 0; i--) {
    const m = messages[i];
    if (m?.role !== "assistant" || !Array.isArray(m.content)) continue;
    const text = m.content
      .filter((c: any) => c?.type === "text" && typeof c.text === "string")
      .map((c: any) => c.text)
      .join("\n")
      .trim();
    if (text) return text;
  }
  return undefined;
}

// Keeps the session's state file current. `begin` runs first at each session
// start; when `on()` is then false, this process writes no state file.
function trackState(pi: ExtensionAPI, on: () => boolean, begin: (ctx: any) => void) {
  pi.on("session_start", (_event, ctx) => {
    begin(ctx);
    if (!on()) return;
    sessionId = ctx.sessionManager.getSessionId();
    running = false;
    writeState("idle");
  });
  // A quit, or a switch to another session: this process no longer runs
  // the session.
  pi.on("session_shutdown", () => {
    writeState("ended");
    sessionId = undefined;
  });
  pi.on("agent_start", () => {
    running = true;
    writeState("working");
  });
  pi.on("agent_settled", () => {
    running = false;
    writeState("idle");
  });
  // An extension's dialog waits for a person.
  pi.on("ui_prompt_start", () => writeState("blocked"));
  pi.on("ui_prompt_end", () => writeState(running ? "working" : "idle"));
}

// When a run completes, runs the turn-end hook `argv`. A reply
// `{"decision":"block","reason":...}` holds the session for one more turn
// that reads the reason.
function holdOnSettle(pi: ExtensionAPI, on: () => boolean, argv: string[]) {
  // Whether the run now settling was started by teamwire's own hold.
  let held = false;
  let lastText: string | undefined;

  pi.on("agent_end", (event) => {
    lastText = finalText(event.messages) ?? lastText;
  });

  pi.on("agent_before_settle", async (event, ctx) => {
    const text = lastText;
    lastText = undefined;
    // A person's Escape or a failed request is not the end of a turn.
    if (!on() || event.outcome !== "completed") {
      held = false;
      return;
    }
    const reply = await runHook(argv, {
      session_id: ctx.sessionManager.getSessionId(),
      hold_active: held,
      final_message: text,
      session_file: ctx.sessionManager.getSessionFile(),
    });
    if (reply?.decision === "block" && typeof reply.reason === "string") {
      held = true;
      return {
        entries: [
          ...event.entries,
          { type: "custom_message", customType: "teamwire", content: reply.reason, display: true },
        ],
        continue: true,
      };
    }
    held = false;
  });
}
"#;

const MEMBER_MAIN: &str = r#"
function args(...rest: string[]): string[] {
  return [...TW.homeArgs, ...rest, "--team", TW.team, "--role", TW.role];
}

export default function (pi: ExtensionAPI) {
  pi.registerMcpServer("teamwire", {
    command: TW.bin,
    args: args("mcp"),
    env: { TEAMWIRE_TOKEN_FILE: TW.env.TEAMWIRE_TOKEN_FILE },
  });

  const always = () => true;
  trackState(pi, always, () => {});
  holdOnSettle(pi, always, args("hook", "turn-end"));

  pi.on("turn_end", (event, ctx) => {
    const usage = ctx.getContextUsage();
    if (!usage || usage.tokens == null) return;
    return {
      entries: [
        ...event.entries,
        {
          type: "custom",
          customType: TW.fillEntry,
          data: { tokens: usage.tokens, window: usage.contextWindow },
        },
      ],
    };
  });

  pi.on("tool_result", async (event, ctx) => {
    // A call another tool made is not one of the member's own.
    if (event.parentToolCallId) return;
    const reply = await runHook(args("hook", "post-tool"), {
      session_id: ctx.sessionManager.getSessionId(),
      session_file: ctx.sessionManager.getSessionFile(),
    });
    const note = reply?.hookSpecificOutput?.additionalContext;
    if (typeof note !== "string" || !note) return;
    return {
      content: [...event.content, { type: "text", text: note }],
      structuredContent: event.structuredContent,
    };
  });
}
"#;

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    reason = "tests read the JSON by key; a missing key reads as null and fails the assert"
)]
mod tests {
    use super::*;

    fn values() -> Values<'static> {
        Values {
            teamwire_bin: "/opt/tw/teamwire",
            home: Some("/h"),
            team: "demo",
            role: "api",
            token_file: "/h/teams/demo/members/api/token",
            state_dir: "/pi/sessions/teamwire",
            hook_timeout_ms: 60_000,
        }
    }

    /// The JSON literal spliced into the text, parsed back.
    fn config_of(text: &str) -> Value {
        let line = text
            .lines()
            .find_map(|l| l.strip_prefix("const TW = "))
            .and_then(|l| l.strip_suffix(';'))
            .unwrap_or_default();
        serde_json::from_str(line).unwrap_or(Value::Null)
    }

    #[test]
    fn the_config_names_the_member_and_its_token_file() {
        let text = render(&values());
        let c = config_of(&text);
        assert_eq!(c["bin"], "/opt/tw/teamwire");
        assert_eq!(c["homeArgs"], json!(["--home", "/h"]));
        assert_eq!(c["env"]["TEAMWIRE_TEAM"], "demo");
        assert_eq!(c["env"]["TEAMWIRE_ROLE"], "api");
        assert_eq!(c["env"]["TEAMWIRE_HOME"], "/h");
        assert_eq!(
            c["env"]["TEAMWIRE_TOKEN_FILE"],
            "/h/teams/demo/members/api/token"
        );
        assert_eq!(c["stateDir"], "/pi/sessions/teamwire");
        assert_eq!(c["stateSuffix"], STATE_SUFFIX);
        assert!(!text.contains("__TEAMWIRE_CONFIG__"));
    }

    #[test]
    fn a_quote_in_a_path_stays_inside_the_literal() {
        let v = Values {
            teamwire_bin: "/odd\"dir\n/teamwire",
            ..values()
        };
        assert_eq!(config_of(&render(&v))["bin"], "/odd\"dir\n/teamwire");
    }

    #[test]
    fn without_a_home_no_home_flag_is_passed() {
        let v = Values {
            home: None,
            ..values()
        };
        let c = config_of(&render(&v));
        assert_eq!(c["homeArgs"], json!([]));
        assert!(c["env"].get("TEAMWIRE_HOME").is_none());
    }
}
