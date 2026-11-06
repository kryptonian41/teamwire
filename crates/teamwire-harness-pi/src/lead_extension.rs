//! The extension of teamwire's lead package for pi (ADR 0030). pi loads it in
//! every session, members included, so it acts only in a session that is
//! not a member's. There it registers the lead's MCP server, keeps the
//! session's state file, runs the lead Stop hook at the end of each run and
//! the lead's tool hook on each tool call (ADR 0035), and runs
//! `teamwire watch` and `teamwire pr-watch` itself (ADR 0039): each line
//! either prints reaches the model as a steered message after the current
//! turn's tool calls, and starts a turn when pi is idle. pi has no tool like
//! Claude's Monitor, and either run through the bash tool would hold that
//! call until it timed out.
//!
//! The text is the shared code of `extension.rs` with one JSON value spliced
//! in, so no path can break out of the literal. It holds no secret.

use serde_json::json;

use crate::extension::{CONFIG_SLOT, IMPORTS, SHARED, STATE_SUFFIX, home_args};

/// The tool that starts a watch.
pub(crate) const WATCH_TOOL: &str = "teamwire_watch";
/// The tool that starts a pull-request watch.
pub(crate) const PR_WATCH_TOOL: &str = "teamwire_pr_watch";
/// The variable that names the lead's session to its MCP server. pi gives
/// a server no session variable and no `_meta` (ADR 0030 point 4).
pub(crate) const LEAD_SESSION_VAR: &str = "TEAMWIRE_PI_LEAD_SESSION";
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
        "prWatchTool": PR_WATCH_TOOL,
        "watchMessage": WATCH_MESSAGE,
        "watchStartMs": WATCH_START_MS,
        "leadSessionVar": LEAD_SESSION_VAR,
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

// A bash command that runs `teamwire watch <team>`, with the team's name, or
// `teamwire pr-watch`, with no name.
const WATCH_IN_BASH =
  /(?:^|[\s;&|(`])(?:\S*\/)?teamwire\s+(?:--home\s+(?:'[^']*'|"[^"]*"|\S+)\s+)?(?:watch\s+([a-z][a-z0-9]{0,11})\b|pr-watch\b)/;

export default function (pi: ExtensionAPI) {
  // Whether this session is a lead's: decided when each session starts.
  let lead = false;
  // The session the MCP server was registered for, and whether the watch
  // tool is registered.
  let serverFor: string | undefined;
  let toolRegistered = false;
  // False from `session_shutdown` on. pi then marks this runtime stale, and
  // `sendMessage` throws.
  let live = true;
  // The watches this process runs, by team or by pull requests, and the ones
  // it is stopping.
  const watches = new Map<string, any>();
  const stopping = new Set<any>();

  // Steered, never a follow-up. pi hands the model a follow-up only once the
  // lead's whole run has stopped, so a line printed during a long run waits
  // for that run and then describes a roster that has already changed. pi's
  // agent loop reads the steering queue after each turn's tool calls have
  // finished and their results are in the context, so a steered line reaches
  // the model within the run and never between a call and its result.
  // `triggerTurn` starts a turn when the lead is idle; without it an idle
  // lead would store the line and never read it.
  //
  // A line can still be in a watch's pipe when pi replaces this runtime on
  // /new, /fork, /resume or /reload. A throw from a stream listener is
  // uncaught, and pi's terminal UI exits on one, so this never throws.
  const say = (text: string) => {
    if (!live) return;
    try {
      pi.sendMessage(
        { customType: TW.watchMessage, content: text, display: true },
        { triggerTurn: true, deliverAs: "steer" },
      );
    } catch {
      // The runtime went stale: the line has nobody to reach.
    }
  };

  // Starts `teamwire <args>` as the watch `key`. Resolves to undefined once
  // it runs, or to what went wrong when it ends within `watchStartMs`. An
  // exit code `fine` accepts is an ordinary end; a later other end is said
  // with `stopped`.
  function startWatch(
    key: string,
    args: string[],
    fine: (code: number | null) => boolean,
    stopped: (why: string) => string,
  ): Promise<string | undefined> {
    return new Promise((resolve) => {
      let child: any;
      try {
        child = spawn(TW.bin, [...TW.homeArgs, ...args], {
          stdio: ["ignore", "pipe", "pipe"],
        });
      } catch (e) {
        resolve(String(e));
        return;
      }
      watches.set(key, child);
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
        if (watches.get(key) === child) watches.delete(key);
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
        if (failed || stopping.delete(child)) return;
        if (pending.trim()) say(pending.trim());
        const why = err.trim().split("\n").pop() || `exit ${code ?? signal}`;
        if (!settled) {
          done(fine(code) ? undefined : why);
        } else if (!fine(code)) {
          say(stopped(why));
        }
      });
    });
  }

  // The server learns its session from this registration, so a new session
  // registers it again, which replaces the earlier one.
  function registerLead(session: string) {
    if (serverFor !== session) {
      serverFor = session;
      pi.registerMcpServer("teamwire", {
        command: TW.bin,
        args: [...TW.homeArgs, "mcp", "--lead"],
        env: { [TW.leadSessionVar]: session },
        exposure: "direct",
      });
    }
    if (toolRegistered) return;
    toolRegistered = true;
    pi.registerTool({
      name: TW.watchTool,
      label: "teamwire watch",
      description:
        "Start `teamwire watch <team>` for a team you lead. Each line the watch prints " +
        "reaches you as a message once your current turn's tool calls have finished, and " +
        "starts a turn when you are idle, so do not poll. " +
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
        const failed = await startWatch(team, ["watch", team], (code) => code === 0, (why) =>
          `[teamwire:${team}] the watch stopped: ${why}. Start it again with the ${TW.watchTool} tool.`);
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
    pi.registerTool({
      name: TW.prWatchTool,
      label: "teamwire pr-watch",
      description:
        "Start `teamwire pr-watch` on GitHub pull requests. Each line it prints (a failed " +
        "check, a merge state, DONE, MERGED) reaches you as a message once your current turn's " +
        "tool calls have finished, and starts a turn when you are idle, so do not poll. " +
        "It ends once every pull request is done.",
      promptSnippet: "Watch the checks of GitHub pull requests; their lines reach you as messages",
      parameters: Type.Object({
        repo: Type.String({ description: "The repository, as OWNER/REPO" }),
        prs: Type.Array(Type.Integer({ minimum: 1 }), {
          minItems: 1,
          description: "The pull request numbers",
        }),
        interval: Type.Optional(
          Type.Integer({ minimum: 1, description: "Seconds between polls; pr-watch's default when left out" }),
        ),
      }),
      async execute(_toolCallId: string, params: { repo: string; prs: number[]; interval?: number }) {
        const repo = String(params.repo ?? "").trim();
        const prs = (params.prs ?? []).map(String);
        const key = `pr-watch ${repo} ${prs.join(" ")}`;
        if (watches.has(key)) {
          return {
            content: [{ type: "text", text: `a watch of ${repo} ${prs.join(", ")} already runs in this session` }],
            details: undefined,
          };
        }
        const every = params.interval ? ["-i", String(params.interval)] : [];
        // Exit 1 means a check failed, which the last line already said.
        const failed = await startWatch(key, ["pr-watch", "-R", repo, ...every, ...prs],
          (code) => code === 0 || code === 1, (why) =>
          `[teamwire] the pull-request watch of ${repo} stopped: ${why}. Start it again with the ${TW.prWatchTool} tool.`);
        if (failed) throw new Error(`teamwire pr-watch ${repo} did not start: ${failed}`);
        return {
          content: [{ type: "text", text: `watching ${repo} ${prs.join(", ")}: each line reaches you as a message from teamwire` }],
          details: undefined,
        };
      },
    });
  }

  trackState(pi, () => lead, (ctx) => {
    live = true;
    lead = !inMember();
    stateExtra = {
      lead: true,
      mode: ctx.mode,
      session_file: ctx.sessionManager.getSessionFile() ?? null,
    };
    if (lead) registerLead(ctx.sessionManager.getSessionId());
  });
  holdOnSettle(pi, () => lead, [...TW.homeArgs, "hook", "lead-stop", "--harness", "pi"]);

  // A quit, a reload or a switch to another session ends this session's
  // watches; the lead starts them again from the session it then runs.
  pi.on("session_shutdown", () => {
    live = false;
    for (const child of watches.values()) {
      stopping.add(child);
      child.stdout.destroy();
      child.kill("SIGTERM");
    }
    watches.clear();
  });

  // The note of the lead's tool hook, which starts with the call and reaches
  // the model beside the next tool result.
  // shortcut: one pending note; when pi runs a turn's calls side by side, a
  // note can land beside another call of that turn, or the next call's note
  // replaces it. Key the notes by call id if a lead misses notes that way.
  let toolNote: Promise<string | undefined> | undefined;

  pi.on("tool_call", (event) => {
    if (!lead) return;
    // A watch run through bash holds that call until it times out, and its
    // lines would wake nobody.
    if (event.toolName === "bash") {
      const command = String((event.input as any)?.command ?? "");
      const found = WATCH_IN_BASH.exec(command);
      if (found && !found[1]) {
        return {
          block: true,
          reason:
            `In pi, watch pull requests with the ${TW.prWatchTool} tool (repo, prs, interval). ` +
            "A pull-request watch run through bash holds the call until every pull request is done, and its lines would not reach you.",
        };
      }
      if (found && !/\s--once\b/.test(command)) {
        return {
          block: true,
          reason:
            `In pi, start the watch with the ${TW.watchTool} tool (team "${found[1]}"). ` +
            "A watch run through bash holds the call until it times out, and its lines would not reach you.",
        };
      }
    }
    toolNote = runHook([...TW.homeArgs, "hook", "lead-tool", "--harness", "pi"], {
      session_id: serverFor,
      tool_name: event.toolName,
      tool_input: event.input,
      cwd: process.cwd(),
    }).then((reply) => {
      const note = reply?.hookSpecificOutput?.additionalContext;
      return typeof note === "string" && note ? note : undefined;
    });
  });

  pi.on("tool_result", async (event) => {
    // A call another tool made is not one of the lead's own.
    if (!lead || event.parentToolCallId || !toolNote) return;
    const note = await toolNote;
    toolNote = undefined;
    if (!note) return;
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
        assert_eq!(c["leadSessionVar"], LEAD_SESSION_VAR);
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
        assert!(text.contains(r#""hook", "lead-tool", "--harness", "pi""#));
        assert!(text.contains(r#""mcp", "--lead""#));
        assert!(!text.contains(r#"args("hook", "turn-end")"#));
        assert!(!text.contains(r#""post-tool""#));
        assert!(!text.contains("TEAMWIRE_TOKEN_FILE:"));
        // The shared state code is there once.
        assert_eq!(text.matches("function writeState").count(), 1);
    }
}
