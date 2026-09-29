//! Live check, run by hand: `cargo run -p teamwire-harness-claude --example live_check`.
//!
//! Starts ONE real Claude Code session in a scratch directory, with the
//! per-run `--settings` and `--mcp-config` this adapter builds, and reports
//! whether the three facts that only a real session can prove hold:
//! the Stop hook runs, the `settings.json` `env` reaches the Bash tool and the
//! hook, and the MCP server named in `mcp.json` is started. No test runs
//! this, because it needs the real `claude`, an account and a network.
//!
//! It reads `~/.claude.json` for trust and writes nothing under `~/.claude`.
//! Everything else goes under one temporary directory that is printed.

#![expect(
    clippy::print_stdout,
    reason = "an example reports each step to the person who ran it"
)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use teamwire_core::{
    Address, Command, Harness, MemberKind, MemberLaunch, OtelConfig, Role, SessionId, TeamName,
};
use teamwire_harness_claude::{ClaudeCode, ClaudeConfig};

/// Stands in for `teamwire`: logs how it was called and which identity it saw.
const STUB: &str = "#!/bin/sh\nprintf '%s | role=%s token_set=%s\\n' \"$*\" \"$TEAMWIRE_ROLE\" \"${TEAMWIRE_TOKEN:+yes}\" >> \"$(dirname \"$0\")/calls.log\"\ncase \"$1\" in hook) cat > /dev/null ;; mcp) sleep 20 ;; esac\n";

fn step(ok: bool, what: &str) -> bool {
    println!("[{}] {what}", if ok { "ok" } else { "FAIL" });
    ok
}

fn main() -> Result<(), String> {
    let home = std::env::var("HOME").map_err(|e| e.to_string())?;
    let platform = teamwire_platform::native();
    let scratch: PathBuf =
        std::env::temp_dir().join(format!("teamwire-live-{}", std::process::id()));
    let member_dir = scratch.join("teams/live/members/probe");
    let work = scratch.join("work");
    for dir in [&member_dir, &work] {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    println!("scratch directory: {}", scratch.display());
    println!(
        "Claude asks to trust {} the first time; answer it in a terminal.",
        work.display()
    );

    let stub = scratch.join("teamwire");
    platform
        .write_atomic(&stub, STUB.as_bytes())
        .map_err(|e| e.to_string())?;
    std::fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    let contract = scratch.join("contract.md");
    platform
        .write_atomic(
            &contract,
            b"You are a probe. Do exactly what the user asks.\n",
        )
        .map_err(|e| e.to_string())?;

    let team = TeamName::parse("live").map_err(|e| e.to_string())?;
    let role = Role::parse("probe").map_err(|e| e.to_string())?;
    let session = SessionId::parse(&uuid_like(std::process::id())).map_err(|e| e.to_string())?;
    let member = MemberLaunch {
        address: Address::of(&team, &role),
        team,
        role,
        kind: MemberKind::Investigator,
        cwd: work.clone(),
        session_id: session,
        model: None,
        effort: None,
        member_dir: member_dir.clone(),
        contract_path: contract,
        identity: vec![("TEAMWIRE_TOKEN".into(), "live-check-token".into())],
        otel: OtelConfig::default(),
        teamwire_bin: stub,
    };
    let claude = ClaudeCode::new(
        platform.clone(),
        ClaudeConfig {
            claude_dir: Path::new(&home).join(".claude"),
            claude_json: Path::new(&home).join(".claude.json"),
            claude_bin: "claude".to_owned(),
        },
    );
    let spec = claude.launch_spec(&member).map_err(|e| e.to_string())?;
    for file in &spec.files {
        platform
            .write_atomic(&file.path, &file.bytes)
            .map_err(|e| e.to_string())?;
        std::fs::set_permissions(
            &file.path,
            std::os::unix::fs::PermissionsExt::from_mode(file.mode),
        )
        .map_err(|e| e.to_string())?;
    }

    // One non-interactive turn: `-p` runs the prompt, the Bash tool writes the
    // role it sees, and the session ends, which fires the Stop hook.
    let seen = scratch.join("bash-saw-role.txt");
    let prompt = format!(
        "Run this shell command and then reply 'done': printenv TEAMWIRE_ROLE > {}",
        seen.display()
    );
    let mut argv = spec.argv.clone();
    let program = argv.remove(0);
    let cmd = Command::new(program)
        .args(argv)
        .arg("-p")
        .arg(prompt)
        .cwd(&work);
    println!("starting one real Claude Code session (up to 3 minutes)...");
    let out = platform
        .run(&cmd, Duration::from_secs(180))
        .map_err(|e| e.to_string())?;
    println!("claude exited {}", out.exit_code);

    let calls = std::fs::read_to_string(scratch.join("calls.log")).unwrap_or_default();
    let mut all = step(out.success(), "the session ran and exited 0");
    all &= step(
        calls.contains("hook claude-stop --team live --role probe"),
        "the Stop hook ran",
    );
    all &= step(
        calls.contains("role=probe token_set=yes"),
        "settings.json env reached the hook",
    );
    all &= step(
        std::fs::read_to_string(&seen).is_ok_and(|t| t.trim() == "probe"),
        "settings.json env reached the Bash tool",
    );
    all &= step(
        calls.contains("mcp --team live --role probe"),
        "the MCP server from mcp.json was started",
    );
    println!(
        "{}",
        if all {
            "ALL CHECKS PASSED"
        } else {
            "SOME CHECKS FAILED"
        }
    );
    if all {
        Ok(())
    } else {
        Err("live check failed".to_owned())
    }
}

/// A well-formed version-4 UUID text from a number; only `--session-id` needs it.
fn uuid_like(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
