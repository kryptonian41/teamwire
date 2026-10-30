//! The extension of teamwire's lead package for pi (ADR 0030). pi loads it in
//! every session, members included, so it acts only in a session that is
//! not a member's. There it registers the lead's MCP server, keeps the
//! session's state file, runs the lead Stop hook at the end of each run, and
//! runs `teamwire watch` itself: each line the watch prints reaches the
//! model as a message, which starts a turn when pi is idle. pi has no tool
//! like Claude's Monitor, and a watch run through the bash tool would hold
//! that call until it timed out.
//!
//! The text is the shared code of `extension.rs` with one JSON value spliced
//! in, so no path can break out of the literal. It holds no secret.

use serde_json::json;

use crate::extension::{CONFIG_SLOT, IMPORTS, SHARED, STATE_SUFFIX, home_args};

/// The tool that starts a watch.
pub(crate) const WATCH_TOOL: &str = "teamwire_watch";
/// The `customType` of a watch line in the session.
const WATCH_MESSAGE: &str = "teamwire.watch";
/// How long the tool waits for a watch to fail at once, such as a second
/// watch of one team, before it says the watch runs.
const WATCH_START_MS: u64 = 2_000;
/// The lead Stop hook reads only local files.
const HOOK_TIMEOUT_MS: u64 = 10_000;

/// What the template needs.
pub(crate) struct LeadValues<'a> {
    pub(crate) teamwire_bin: &'a str,
    pub(crate) home: Option<&'a str>,
    /// Where the extension writes each session's state file.
    pub(crate) state_dir: &'a str,
}

pub(crate) fn render(v: &LeadValues<'_>) -> String {
    let config = json!({
        "bin": v.teamwire_bin,
        "homeArgs": home_args(v.home),
        "env": {},
        "hookTimeoutMs": HOOK_TIMEOUT_MS,
        "stateDir": v.state_dir,
        "stateSuffix": STATE_SUFFIX,
        "watchTool": WATCH_TOOL,
        "watchMessage": WATCH_MESSAGE,
        "watchStartMs": WATCH_START_MS,
    });
    [HEAD, IMPORTS, LEAD_IMPORTS, SHARED, LEAD_MAIN]
        .concat()
        .replace(CONFIG_SLOT, &config.to_string())
}

const HEAD: &str = "// Written by `teamwire install` for a pi lead. Install writes it again
// when it changes, so an edit here does not last.
";

const LEAD_IMPORTS: &str = r#"import { Type } from "@earendil-works/pi-ai";
"#;

const LEAD_MAIN: &str = r#"
// pi loads the user's packages in teamwire's members too. A member's own
// extension sets these when it loads, and every extension loads before the
// first session starts.
function inMember(): boolean {
  return Boolean(process.env.TEAMWIRE_ROLE || process.env.TEAMWIRE_TOKEN_FILE);
}

// A bash command that runs `teamwire watch <team>`, with the team's name.
const WATCH_IN_BASH =
  /(?:^|[\s;&|(`])(?:\S*\/)?teamwire\s+(?:--home\s+(?:'[^']*'|"[^"]*"|\S+)\s+)?watch\s+([a-z][a-z0-9]{0,11})\b/;

export default function (pi: ExtensionAPI) {
  // Whether this session is a lead's: decided when each session starts.
  let lead = false;
  let registered = false;
  // The watches this process runs, by team, and the ones it is stopping.
  const watches = new Map<string, any>();
  const stopping = new Set<any>();

  const say = (text: string) =>
    pi.sendMessage(
      { customType: TW.watchMessage, content: text, display: true },
      { triggerTurn: true, deliverAs: "followUp" },
    );

  // Starts `teamwire watch <team>`. Resolves to undefined once it runs, or
  // to what went wrong when it ends within `watchStartMs`.
  function startWatch(team: string): Promise<string | undefined> {
    return new Promise((resolve) => {
      let child: any;
      try {
        child = spawn(TW.bin, [...TW.homeArgs, "watch", team], {
          stdio: ["ignore", "pipe", "pipe"],
        });
      } catch (e) {
        resolve(String(e));
        return;
      }
      watches.set(team, child);
      let settled = false;
      let failed = false;
      let pending = "";
      let err = "";
      const done = (value: string | undefined) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        resolve(value);
      };
      const timer = setTimeout(() => done(undefined), TW.watchStartMs);
      const forget = () => {
        if (watches.get(team) === child) watches.delete(team);
      };
      child.stdout.on("data", (chunk: Buffer) => {
        pending += chunk.toString("utf8");
        let at: number;
        while ((at = pending.indexOf("\n")) >= 0) {
          const line = pending.slice(0, at).trimEnd();
          pending = pending.slice(at + 1);
          if (line) say(line);
        }
      });
      child.stderr.on("data", (chunk: Buffer) => {
        err = (err + chunk.toString("utf8")).slice(-4000);
      });
      child.on("error", (e: unknown) => {
        failed = true;
        forget();
        done(String(e));
      });
      child.on("close", (code: number | null, signal: string | null) => {
        forget();
        if (pending.trim()) say(pending.trim());
        if (failed || stopping.delete(child)) return;
        const why = err.trim().split("\n").pop() || `exit ${code ?? signal}`;
        if (!settled) {
          done(code === 0 ? undefined : why);
        } else if (code !== 0) {
          say(`[teamwire:${team}] the watch stopped: ${why}. Start it again with the ${TW.watchTool} tool.`);
        }
      });
    });
  }

  function registerLead() {
    if (registered) return;
    registered = true;
    pi.registerMcpServer("teamwire", {
      command: TW.bin,
      args: [...TW.homeArgs, "mcp", "--lead"],
      exposure: "direct",
    });
    pi.registerTool({
      name: TW.watchTool,
      label: "teamwire watch",
      description:
        "Start `teamwire watch <team>` for a team you lead. Each line the watch prints " +
        "reaches you as a message, and starts a turn when you are idle, so do not poll. " +
        "Call it right after `teamwire launch`, and again when the watch stopped or " +
        "printed TEAM-COMPLETE before an `--add`.",
      promptSnippet: "Start the watch of a teamwire team you lead; its lines reach you as messages",
      parameters: Type.Object({
        team: Type.String({ description: "The team's name, as `teamwire launch` printed it" }),
      }),
      async execute(_toolCallId: string, params: { team: string }) {
        const team = String(params.team ?? "").trim();
        if (watches.has(team)) {
          return {
            content: [{ type: "text", text: `a watch of ${team} already runs in this session` }],
            details: undefined,
          };
        }
        const failed = await startWatch(team);
        if (failed) throw new Error(`teamwire watch ${team} did not start: ${failed}`);
        return {
          content: [
            {
              type: "text",
              text: `watching ${team}: each watch line reaches you as a message from teamwire`,
            },
          ],
          details: undefined,
        };
      },
    });
  }

  trackState(pi, () => lead, (ctx) => {
    lead = !inMember();
    stateExtra = {
      lead: true,
      mode: ctx.mode,
      session_file: ctx.sessionManager.getSessionFile() ?? null,
      cwd: ctx.cwd,
    };
    if (lead) registerLead();
  });
  holdOnSettle(pi, () => lead, [...TW.homeArgs, "hook", "lead-stop", "--harness", "pi"]);

  // A quit, a reload or a switch to another session ends this session's
  // watches; the lead starts them again from the session it then runs.
  pi.on("session_shutdown", () => {
    for (const child of watches.values()) {
      stopping.add(child);
      child.kill("SIGTERM");
    }
    watches.clear();
  });

  // A watch run through bash holds that call until it times out, and its
  // lines would wake nobody.
  pi.on("tool_call", (event) => {
    if (!lead || event.toolName !== "bash") return;
    const command = String((event.input as any)?.command ?? "");
    const found = WATCH_IN_BASH.exec(command);
    if (!found || /\s--once\b/.test(command)) return;
    return {
      block: true,
      reason:
        `In pi, start the watch with the ${TW.watchTool} tool (team "${found[1]}"). ` +
        "A watch run through bash holds the call until it times out, and its lines would not reach you.",
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
    use serde_json::Value;

    use super::*;

    fn config_of(text: &str) -> Value {
        let line = text
            .lines()
            .find_map(|l| l.strip_prefix("const TW = "))
            .and_then(|l| l.strip_suffix(';'))
            .unwrap_or_default();
        serde_json::from_str(line).unwrap_or(Value::Null)
    }

    fn values() -> LeadValues<'static> {
        LeadValues {
            teamwire_bin: "/opt/tw/teamwire",
            home: Some("/h"),
            state_dir: "/pi/sessions/teamwire",
        }
    }

    #[test]
    fn the_config_names_the_binary_the_home_and_the_state_folder() {
        let text = render(&values());
        let c = config_of(&text);
        assert_eq!(c["bin"], "/opt/tw/teamwire");
        assert_eq!(c["homeArgs"], json!(["--home", "/h"]));
        assert_eq!(c["env"], json!({}));
        assert_eq!(c["stateDir"], "/pi/sessions/teamwire");
        assert_eq!(c["stateSuffix"], STATE_SUFFIX);
        assert_eq!(c["watchTool"], WATCH_TOOL);
        assert!(!text.contains(CONFIG_SLOT));
    }

    #[test]
    fn without_a_home_no_home_flag_is_passed() {
        let text = render(&LeadValues {
            home: None,
            ..values()
        });
        assert_eq!(config_of(&text)["homeArgs"], json!([]));
    }

    #[test]
    fn the_lead_runs_the_lead_hook_and_server_and_never_a_member_hook() {
        let text = render(&values());
        assert!(text.contains(r#""hook", "lead-stop", "--harness", "pi""#));
        assert!(text.contains(r#""mcp", "--lead""#));
        assert!(!text.contains(r#"args("hook", "turn-end")"#));
        assert!(!text.contains(r#""post-tool""#));
        assert!(!text.contains("TEAMWIRE_TOKEN_FILE:"));
        // The shared state code is there once.
        assert_eq!(text.matches("function writeState").count(), 1);
    }
}
