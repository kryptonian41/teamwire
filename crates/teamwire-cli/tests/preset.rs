//! `teamwire preset`, and the six files of `examples/presets/`. Each file
//! must be a spec that `launch` accepts once its placeholders are filled in,
//! so the test fills them in and runs the real validation.
#![expect(
    clippy::disallowed_methods,
    reason = "the test starts the teamwire binary and makes a throwaway git repository"
)]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;
use teamwire_core::{Config, Harness, HarnessId, HostId, Registry};
use teamwire_fakes::{FakeHarness, FakeHost, FakePlatform, TestHome};
use teamwire_launch::{ValidateEnv, parse_spec, validate};

fn presets_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/presets")
}

fn preset_text(name: &str) -> String {
    std::fs::read_to_string(presets_dir().join(format!("{name}.json"))).unwrap()
}

const NAMES: [&str; 6] = [
    "survey",
    "implement-then-review",
    "parallel-tickets",
    "dependent-tickets",
    "contract-split",
    "closer",
];

fn git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The preset with its placeholders replaced by a real repository, real
/// brief files and the branches its members start from.
fn filled(name: &str, work: &Path) -> String {
    let repo = work.join("repo");
    if !repo.exists() {
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join(".gitignore"), ".worktrees/\n").unwrap();
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-q", "-m", "first"]);
    }
    let briefs = work.join("briefs");
    std::fs::create_dir_all(&briefs).unwrap();
    let text = preset_text(name)
        .replace("/abs/path/repo", &repo.display().to_string())
        .replace("/abs/path/briefs", &briefs.display().to_string());
    let doc: Value = serde_json::from_str(&text).unwrap();
    for child in doc["children"].as_array().unwrap() {
        if let Some(file) = child["task_file"].as_str() {
            std::fs::write(file, "the brief\n").unwrap();
        }
        if let Some(base) = child["base"].as_str().filter(|b| *b != "main") {
            git(&repo, &["branch", "-f", base, "main"]);
        }
    }
    text
}

fn validates(name: &str) {
    let work = tempfile::tempdir().unwrap();
    let text = filled(name, work.path());
    let home = TestHome::new().unwrap();
    let harness = Arc::new(FakeHarness::new());
    harness.set_interactive_caller(teamwire_core::SessionId::parse("lead-session").unwrap());
    let mut registry = Registry::new();
    registry
        .add_harness(Arc::clone(&harness) as Arc<dyn Harness>)
        .unwrap();
    registry.add_host(Arc::new(FakeHost::new())).unwrap();
    let platform = RealGit::default();
    let config = Config::default();
    let user_home = work.path().join("userhome");
    std::fs::create_dir_all(&user_home).unwrap();
    let env = ValidateEnv {
        home: home.path(),
        registry: &registry,
        platform: &platform,
        config: &config,
        lead_harness: HarnessId::from_static(FakeHarness::ID),
        lead_host: HostId::from_static(FakeHost::ID),
        user_home: Some(user_home),
        teamwire_bin: PathBuf::from("/usr/local/bin/teamwire"),
    };
    let spec = parse_spec(&text).unwrap_or_else(|e| unreachable!("{name}: {e:?}"));
    let plan = validate(&spec, false, &env).unwrap_or_else(|e| unreachable!("{name}: {e:?}"));
    assert_eq!(plan.members.len(), spec.children.len(), "{name}");
}

/// A platform that runs real processes (the launch check asks git about the
/// repository) and answers everything else like the in-process fake.
#[derive(Debug, Default)]
struct RealGit {
    inner: FakePlatform,
}

impl teamwire_core::Platform for RealGit {
    fn pid_alive(&self, pid: teamwire_core::Pid) -> bool {
        self.inner.pid_alive(pid)
    }
    fn env_of(
        &self,
        pid: teamwire_core::Pid,
        var: &str,
    ) -> Result<Option<String>, teamwire_core::PlatformError> {
        self.inner.env_of(pid, var)
    }
    fn run(
        &self,
        cmd: &teamwire_core::Command,
        _timeout: std::time::Duration,
    ) -> Result<teamwire_core::RunOutput, teamwire_core::PlatformError> {
        let mut c = std::process::Command::new(&cmd.program);
        c.args(&cmd.args);
        if let Some(dir) = &cmd.cwd {
            c.current_dir(dir);
        }
        c.envs(cmd.env.iter().map(|(k, v)| (k, v)));
        c.stdin(std::process::Stdio::null());
        let out = c.output().map_err(|e| {
            teamwire_core::PlatformError::new(teamwire_core::PlatformErrorCode::Io, e.to_string())
        })?;
        Ok(teamwire_core::RunOutput {
            exit_code: out.status.code().unwrap_or(-1),
            stdout: out.stdout,
            stderr: out.stderr,
            timed_out: false,
        })
    }
    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), teamwire_core::PlatformError> {
        self.inner.write_atomic(path, bytes)
    }
    fn sleep_hold(&self) -> Option<&dyn teamwire_core::SleepHold> {
        self.inner.sleep_hold()
    }
    fn free_bytes(&self, path: &Path) -> Result<u64, teamwire_core::PlatformError> {
        self.inner.free_bytes(path)
    }
    fn on_network_fs(&self, path: &Path) -> Result<bool, teamwire_core::PlatformError> {
        self.inner.on_network_fs(path)
    }
}

#[test]
fn survey_is_a_valid_spec() {
    validates("survey");
}

#[test]
fn implement_then_review_is_a_valid_spec() {
    validates("implement-then-review");
}

#[test]
fn parallel_tickets_is_a_valid_spec() {
    validates("parallel-tickets");
}

#[test]
fn dependent_tickets_is_a_valid_spec() {
    validates("dependent-tickets");
}

#[test]
fn contract_split_is_a_valid_spec() {
    validates("contract-split");
}

#[test]
fn closer_is_a_valid_spec() {
    validates("closer");
}

#[test]
fn no_preset_holds_a_machine_specific_path() {
    let mut seen = 0;
    for entry in std::fs::read_dir(presets_dir()).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        for forbidden in ["/Users/", "/home/", "~/", ".py", "knit"] {
            assert!(
                !text.contains(forbidden),
                "{} contains {forbidden}",
                path.display()
            );
        }
        seen += 1;
    }
    assert_eq!(
        seen,
        NAMES.len(),
        "the directory holds exactly the six shapes"
    );
}

struct Run {
    code: i32,
    out: String,
    err: String,
}

fn teamwire(args: &[&str]) -> Run {
    let home = tempfile::tempdir().unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .arg("--home")
        .arg(home.path())
        .args(args)
        .env_remove("HERDR_ENV")
        .output()
        .unwrap();
    Run {
        code: out.status.code().unwrap(),
        out: String::from_utf8(out.stdout).unwrap(),
        err: String::from_utf8(out.stderr).unwrap(),
    }
}

#[test]
fn without_a_name_the_six_shapes_are_listed() {
    let run = teamwire(&["preset"]);
    assert_eq!(run.code, 0, "{}", run.err);
    let names: Vec<&str> = run
        .out
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    assert_eq!(names, NAMES);
}

#[test]
fn a_named_shape_prints_its_rules_and_a_spec_skeleton_that_parses() {
    for name in NAMES {
        let run = teamwire(&["preset", name]);
        assert_eq!(run.code, 0, "{name}: {}", run.err);
        assert!(run.out.starts_with(&format!("{name}: ")), "{}", run.out);
        assert!(run.out.contains(":\n  1. "), "{name}: {}", run.out);

        let (_, skeleton) = run
            .out
            .split_once("`teamwire launch --spec FILE`:\n")
            .unwrap();
        let printed: Value = serde_json::from_str(skeleton).unwrap();
        let mut file: Value = serde_json::from_str(&preset_text(name)).unwrap();
        file.as_object_mut()
            .unwrap()
            .retain(|key, _| !key.starts_with('_'));
        assert_eq!(printed, file, "{name}: the skeleton is the file's spec");
        assert!(parse_spec(skeleton).is_ok(), "{name}");
    }
}

#[test]
fn an_unknown_shape_is_a_usage_error_that_names_the_shapes() {
    let run = teamwire(&["preset", "nope"]);
    assert_eq!(run.code, 3);
    assert!(run.err.contains("no shape named nope"), "{}", run.err);
    assert!(run.err.contains("survey"), "{}", run.err);
    assert!(run.out.is_empty());
}

fn example(path: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .join(path),
    )
    .unwrap()
}

/// The subcommands `teamwire --help` lists.
fn subcommands() -> Vec<String> {
    let run = teamwire(&["--help"]);
    run.out
        .lines()
        .skip_while(|l| !l.starts_with("Commands:"))
        .skip(1)
        .take_while(|l| l.starts_with("  "))
        .filter_map(|l| l.split_whitespace().next().map(str::to_owned))
        .collect()
}

#[test]
fn lead_json_registers_the_lead_mcp_server_with_arguments_the_binary_accepts() {
    let doc: Value = serde_json::from_str(&example("mcp/lead.json")).unwrap();
    let server = &doc["mcpServers"]["teamwire"];
    assert_eq!(server["command"], "teamwire");
    assert_eq!(server["args"], serde_json::json!(["mcp", "--lead"]));
    let help = teamwire(&["mcp", "--help"]);
    assert_eq!(help.code, 0, "{}", help.err);
    assert!(help.out.contains("--lead"), "{}", help.out);
}

/// The word after `teamwire ` in every place the skill shows a command: text
/// in backticks, and lines inside a fenced block.
fn shown_commands(skill: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut fenced = false;
    for line in skill.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        let word_after = |text: &str| {
            text.trim_start_matches("teamwire ")
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
                .next()
                .unwrap_or_default()
                .to_owned()
        };
        if fenced {
            if line.trim_start().starts_with("teamwire ") {
                found.push((word_after(line.trim_start()), line.to_owned()));
            }
        } else {
            for span in line.split('`').skip(1).step_by(2) {
                if span.starts_with("teamwire ") {
                    found.push((word_after(span), line.to_owned()));
                }
            }
        }
    }
    found
}

#[test]
fn the_skill_names_only_commands_and_tools_that_exist() {
    let skill = example("skills/teamwire/SKILL.md");
    assert!(
        skill.starts_with("---\nname: teamwire\ndescription: "),
        "the front matter names the skill"
    );

    let commands = subcommands();
    assert!(commands.iter().any(|c| c == "launch"), "{commands:?}");
    let shown = shown_commands(&skill);
    assert!(shown.len() > 20, "the skill shows {} commands", shown.len());
    for (word, line) in &shown {
        assert!(
            commands.contains(word),
            "`teamwire {word}` is not a command: {line}"
        );
    }

    let mcp = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/api/mcp.md"),
    )
    .unwrap();
    let tools: Vec<&str> = skill
        .split('`')
        .skip(1)
        .step_by(2)
        .filter(|s| s.starts_with("team_"))
        .collect();
    assert!(!tools.is_empty());
    for tool in tools {
        assert!(
            mcp.contains(&format!("## `{tool}`")),
            "{tool} is not an MCP tool"
        );
    }

    // The cutover list registers the lead's server with `teamwire install`
    // (ADR 0012) and checks it live.
    let (_, cutover) = skill.split_once("## Cutover").unwrap();
    let (cutover, _) = cutover.split_once("## How it fits together").unwrap();
    for needed in [
        "`teamwire install`",
        "CLAUDE_CODE_SESSION_ID",
        "live_check.rs",
    ] {
        assert!(cutover.contains(needed), "the cutover list lacks {needed}");
    }

    for shape in NAMES {
        assert!(
            skill.contains(&format!("`{shape}`")),
            "the skill does not name the shape {shape}"
        );
    }

    // Past the section that explains the two tools, no Python script or
    // pane-typing command is named as a way to work.
    let (_, body) = skill.split_once("## How it fits together").unwrap();
    for forbidden in [
        ".py",
        "team-report.sh",
        "herdr agent prompt",
        "~/.claude/team/",
    ] {
        assert!(
            !body.contains(forbidden),
            "the skill's procedures name {forbidden}"
        );
    }
}
