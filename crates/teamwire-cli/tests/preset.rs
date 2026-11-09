//! `teamwire preset`, and the nine files of `examples/presets/`. Each file
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
use teamwire_core::{Config, Harness, HarnessId, HostId, Registry, TaskClass};
use teamwire_fakes::{FakeHarness, FakeHost, FakePlatform, TestHome};
use teamwire_launch::{ValidateEnv, parse_spec, validate};

fn presets_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/presets")
}

fn preset_text(name: &str) -> String {
    std::fs::read_to_string(presets_dir().join(format!("{name}.json"))).unwrap()
}

const NAMES: [&str; 9] = [
    "survey",
    "implement-then-review",
    "parallel-tickets",
    "dependent-tickets",
    "contract-split",
    "speckit",
    "plan-review",
    "integrator",
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
    let (repo, briefs) = fixture(work);
    // A pane that starts in an earlier member's worktree joins once launch
    // has made that worktree. In one spec the real path would collide with
    // that member's own worktree, so a stand-in directory takes its place.
    let worktrees = work.join("worktrees");
    let text = preset_text(name)
        .replace(
            "/abs/path/repo/.worktrees",
            &worktrees.display().to_string(),
        )
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
        // Only the stand-in is made, so a pane cwd that maps to nothing else
        // still fails validation.
        if let Some(cwd) = child["cwd"]
            .as_str()
            .filter(|c| Path::new(c).starts_with(&worktrees))
        {
            std::fs::create_dir_all(cwd).unwrap();
        }
    }
    text
}

/// The repository and the briefs folder that `filled` puts in a preset.
fn fixture(work: &Path) -> (PathBuf, PathBuf) {
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
    (repo, briefs)
}

fn validates(name: &str, work: &Path) {
    validates_text(name, &filled(name, work), work);
}

fn validates_text(name: &str, text: &str, work: &Path) {
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
    let user_home = work.join("userhome");
    std::fs::create_dir_all(&user_home).unwrap();
    let env = ValidateEnv {
        home: home.path(),
        registry: &registry,
        platform: &platform,
        clock: &teamwire_fakes::FakeClock::new(),
        config: &config,
        lead_harness: HarnessId::from_static(FakeHarness::ID),
        lead_host: HostId::from_static(FakeHost::ID),
        user_home: Some(user_home),
        teamwire_bin: PathBuf::from("/usr/local/bin/teamwire"),
    };
    let spec = parse_spec(text).unwrap_or_else(|e| unreachable!("{name}: {e:?}"));
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

/// Every preset validates. They share one repository, because `filled` reuses
/// it and a git repository per preset only adds process starts.
#[test]
fn every_preset_is_a_valid_spec() {
    let work = tempfile::tempdir().unwrap();
    for name in NAMES {
        validates(name, work.path());
    }
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
        "the directory holds exactly the listed presets"
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
fn without_a_name_every_preset_is_listed() {
    let run = teamwire(&["preset"]);
    assert_eq!(run.code, 0, "{}", run.err);
    let names: Vec<&str> = run
        .out
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    assert_eq!(names, NAMES);
}

/// The six team shapes come first. `plan-review`, `integrator` and `closer`
/// are steps a lead adds to a team, so each says so at the start of its
/// description.
#[test]
fn the_steps_added_to_a_team_say_so_and_the_six_shapes_do_not() {
    for (name, start) in [
        ("plan-review", "FIRST STEP, not a team shape."),
        ("integrator", "FINAL STEP, not a team shape."),
        ("closer", "FINAL STEP, not a team shape."),
    ] {
        let file: Value = serde_json::from_str(&preset_text(name)).unwrap();
        assert!(
            file["_shape"].as_str().unwrap().starts_with(start),
            "{name}"
        );
    }
    for name in &NAMES[..6] {
        let file: Value = serde_json::from_str(&preset_text(name)).unwrap();
        assert!(
            !file["_shape"]
                .as_str()
                .unwrap()
                .contains("not a team shape")
        );
    }
}

/// The plan reviewer starts alone, before any implementer, so the preset is
/// one read-only member. One rule for each decision about when and how a
/// lead uses it.
#[test]
fn the_plan_review_preset_is_one_plan_reviewer_in_a_pane() {
    let file: Value = serde_json::from_str(&preset_text("plan-review")).unwrap();
    let members = file["children"].as_array().unwrap();
    assert_eq!(members.len(), 1);
    let member = &members[0];
    assert_eq!(member["role"], "planreview");
    assert_eq!(member["kind"], "plan_reviewer");
    assert_eq!(member["mode"], "pane");
    assert_eq!(member["task_class"], "judgement");
    assert_eq!(member["cwd"], "/abs/path/repo");
    assert_eq!(member["deadline_s"], 1200);
    assert!(member["task"].as_str().is_some_and(|t| !t.is_empty()));
    assert_eq!(file["_rules"].as_array().unwrap().len(), 6);
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

/// The skeleton as printed, without the members that carry a launch note
/// (they are added later, once their blockers are done), launches with no
/// branch made beforehand.
#[test]
fn the_dependent_skeleton_without_its_later_members_validates_as_printed() {
    let run = teamwire(&["preset", "dependent-tickets"]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(
        run.out.contains("`--add`"),
        "the text must say how to add the later members: {}",
        run.out
    );
    let (_, skeleton) = run
        .out
        .split_once("`teamwire launch --spec FILE`:\n")
        .unwrap();
    let work = tempfile::tempdir().unwrap();
    let (repo, briefs) = fixture(work.path());
    let mut doc: Value = serde_json::from_str(
        &skeleton
            .replace("/abs/path/repo", &repo.display().to_string())
            .replace("/abs/path/briefs", &briefs.display().to_string()),
    )
    .unwrap();
    let children = doc["children"].as_array_mut().unwrap();
    children.retain(|c| c.get("_launch").is_none() && c.get("_launch_after").is_none());
    assert!(!children.is_empty());
    for child in children {
        std::fs::write(child["task_file"].as_str().unwrap(), "the brief\n").unwrap();
    }
    validates_text("dependent-tickets", &doc.to_string(), work.path());
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

/// The AFK skill builds on the main one, so it may name only commands that
/// exist too, and `teamwire afk start` must be one of them.
#[test]
fn the_afk_skill_names_only_commands_that_exist() {
    let skill = example("skills/afk/SKILL.md");
    assert!(
        skill.starts_with("---\nname: afk\ndescription: "),
        "the front matter names the skill"
    );
    let commands = subcommands();
    let shown = shown_commands(&skill);
    assert!(
        shown
            .iter()
            .any(|(word, line)| word == "afk" && line.contains("afk start")),
        "the AFK skill does not show `teamwire afk start`"
    );
    for (word, line) in &shown {
        assert!(
            commands.contains(word),
            "`teamwire {word}` is not a command: {line}"
        );
    }
}

#[test]
fn the_promptwire_skill_names_only_commands_that_exist() {
    let skill = example("skills/promptwire/SKILL.md");
    assert!(
        skill.starts_with("---\nname: promptwire\ndescription: "),
        "the front matter names the skill"
    );
    let commands = subcommands();
    let shown = shown_commands(&skill);
    assert!(!shown.is_empty(), "the skill names no teamwire command");
    for (word, line) in &shown {
        assert!(
            commands.contains(word),
            "`teamwire {word}` is not a command: {line}"
        );
    }
}

#[test]
fn the_skill_names_only_commands_and_tools_that_exist() {
    // A command named only in the reference file is checked too.
    let skill = example("skills/teamwire/SKILL.md");
    let reference = example("skills/teamwire/reference.md");
    assert!(
        skill.starts_with("---\nname: teamwire\ndescription: "),
        "the front matter names the skill"
    );
    let skill = format!("{skill}\n{reference}");

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

    // The setup list registers the lead's server with `teamwire install`
    // (ADR 0012) and checks it live.
    let (_, setup) = skill.split_once("## Setting up a machine").unwrap();
    let (setup, _) = setup.split_once("## How it fits together").unwrap();
    for needed in [
        "`teamwire install`",
        "CLAUDE_CODE_SESSION_ID",
        "live_check.rs",
    ] {
        assert!(setup.contains(needed), "the setup list lacks {needed}");
    }

    for shape in NAMES {
        assert!(
            skill.contains(&format!("`{shape}`")),
            "the skill does not name the shape {shape}"
        );
    }

    // No script outside teamwire or pane-typing command is named as a way
    // to work, anywhere in the skill.
    for forbidden in [
        ".py",
        "team-report.sh",
        "herdr agent prompt",
        "~/.claude/team/",
    ] {
        assert!(
            !skill.contains(forbidden),
            "the skill's procedures name {forbidden}"
        );
    }
}

/// A preset names no model, effort or plugin, because those depend on the
/// harness each member runs on, and the lead picks them from what
/// `teamwire models` lists for that harness. Each member names the task
/// class that fits its seat instead, so a run's outcome can be read per class.
#[test]
fn every_member_names_a_task_class_and_no_model() {
    for name in NAMES {
        let file: Value = serde_json::from_str(&preset_text(name)).unwrap();
        for member in file["children"].as_array().unwrap() {
            let role = &member["role"];
            for key in ["model", "effort", "plugins"] {
                assert!(member.get(key).is_none(), "{name}/{role} sets {key}");
            }
            let class = member["task_class"]
                .as_str()
                .unwrap_or_else(|| unreachable!("{name}/{role} sets no task_class"));
            let class = TaskClass::parse(class).unwrap();
            let fits = match member["kind"].as_str() {
                // The integrator only merges in a set order and runs the gate.
                Some("implementer") if role == "integrator" => TaskClass::MechanicalEdit,
                Some("implementer") => TaskClass::ContainedFeature,
                Some("reviewer" | "plan_reviewer" | "closer") => TaskClass::Judgement,
                Some("investigator") => TaskClass::ReadOnlySurvey,
                other => unreachable!("{name}/{role}: kind {other:?}"),
            };
            assert_eq!(class, fits, "{name}/{role}");
        }
        let text = preset_text(name).to_lowercase();
        for vendor_name in ["sonnet", "opus", "haiku"] {
            assert!(!text.contains(vendor_name), "{name} names {vendor_name}");
        }
    }
}

/// The printed skeleton shows a member's task class beside its seat, right
/// after `mode`, rather than after the task text where it is easy to miss.
#[test]
fn the_skeleton_prints_the_task_class_right_after_the_mode() {
    for name in NAMES {
        let run = teamwire(&["preset", name]);
        assert_eq!(run.code, 0, "{name}: {}", run.err);
        let lines: Vec<&str> = run.out.lines().map(str::trim).collect();
        let modes: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.starts_with("\"mode\": "))
            .map(|(i, _)| i)
            .collect();
        assert!(!modes.is_empty(), "{name}: {}", run.out);
        for i in modes {
            assert!(
                lines[i + 1].starts_with("\"task_class\": "),
                "{name}: {} follows {}",
                lines[i + 1],
                lines[i]
            );
        }
    }
}

/// The lead reads each preset's rules and notes (every key that starts with
/// an underscore), and the pi or Codex note, beside the skill. Each item is
/// the text and the place it came from.
fn lead_notes() -> Vec<(String, String)> {
    fn underscore_texts(map: &serde_json::Map<String, Value>) -> Vec<String> {
        map.iter()
            .filter(|(key, _)| key.starts_with('_'))
            .flat_map(|(_, value)| match value {
                Value::String(text) => vec![text.clone()],
                Value::Array(items) => items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
                Value::Null | Value::Bool(_) | Value::Number(_) | Value::Object(_) => Vec::new(),
            })
            .collect()
    }
    let mut notes = Vec::new();
    for name in NAMES {
        let file: Value = serde_json::from_str(&preset_text(name)).unwrap();
        let mut texts = underscore_texts(file.as_object().unwrap());
        for member in file["children"].as_array().unwrap() {
            texts.extend(underscore_texts(member.as_object().unwrap()));
        }
        notes.extend(texts.into_iter().map(|t| (name.to_owned(), t)));
    }
    for note in ["skills/pi-lead.md", "skills/codex-lead.md"] {
        notes.push((note.to_owned(), example(note)));
    }
    notes
}

/// Every class a `--for <class>` in `text` names, leaving out a `<class>`
/// placeholder.
fn classes_named(text: &str) -> Vec<String> {
    text.split("--for ")
        .skip(1)
        .filter(|rest| !rest.starts_with('<'))
        .map(|rest| {
            rest.chars()
                .take_while(|c| c.is_ascii_lowercase() || *c == '_')
                .collect()
        })
        .collect()
}

/// A rule that sends the lead to a command or a task class that does not
/// exist ends in an error at the moment the lead follows it.
#[test]
fn preset_rules_and_harness_notes_name_only_commands_and_classes_that_exist() {
    let commands = subcommands();
    let notes = lead_notes();
    let mut shown = 0;
    let mut classes = 0;
    for (place, text) in &notes {
        for (word, line) in shown_commands(text) {
            assert!(
                commands.contains(&word),
                "{place}: `teamwire {word}` is not a command: {line}"
            );
            shown += 1;
        }
        for class in classes_named(text) {
            assert!(
                TaskClass::parse(&class).is_ok(),
                "{place}: `--for {class}` is not a task class"
            );
            classes += 1;
        }
    }
    assert!(shown >= 10, "the notes show {shown} commands");
    assert!(classes >= 4, "the notes name {classes} classes after --for");
}

fn preset_doc(name: &str) -> Value {
    serde_json::from_str(&preset_text(name)).unwrap()
}

fn task_of<'a>(doc: &'a Value, role: &str) -> &'a str {
    doc["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == role)
        .and_then(|m| m["task"].as_str())
        .unwrap_or_else(|| unreachable!("no task for {role}"))
}

/// The lead reads only the top of a report, so every inline task asks for
/// the summary there. A member with a `task_file` gets the rule from its
/// prompt instead.
#[test]
fn every_inline_task_asks_for_a_summary_at_the_top_of_the_report() {
    let mut seen = 0;
    for name in NAMES {
        for member in preset_doc(name)["children"].as_array().unwrap() {
            if let Some(task) = member["task"].as_str() {
                assert!(
                    task.contains("Start your report file with a summary of at most ten lines"),
                    "{name}/{}",
                    member["role"]
                );
                seen += 1;
            }
        }
    }
    assert!(seen >= 6, "{seen} inline tasks");
}

/// A read-only member that keeps claude-mem reads other members' findings
/// back, so each one carries a note that points at the plugins map.
#[test]
fn every_read_only_member_carries_the_plugins_note() {
    for name in NAMES {
        for member in preset_doc(name)["children"].as_array().unwrap() {
            let read_only = matches!(
                member["kind"].as_str(),
                Some("reviewer" | "plan_reviewer" | "closer" | "investigator")
            );
            if read_only {
                let note = member["_note"].as_str().unwrap_or_default();
                assert!(
                    note.contains("plugins map") && note.contains("Skills and plugins in members"),
                    "{name}/{}: {note}",
                    member["role"]
                );
            }
        }
    }
}

/// The contract member makes the branch the feature members start from, in
/// a wave of its own, and the feature members join later with `--add`.
#[test]
fn the_contract_split_starts_with_the_contract_member() {
    let doc = preset_doc("contract-split");
    let members = doc["children"].as_array().unwrap();
    let contract = &members[0];
    assert_eq!(contract["role"], "contract");
    assert_eq!(contract["kind"], "implementer");
    assert_eq!(contract["_wave"], 1);
    assert_eq!(contract["base"], "main");
    let base = contract["branch"].as_str().unwrap();
    assert!(contract["check"].as_str().is_some());
    for member in &members[1..] {
        assert_eq!(member["base"], base, "{}", member["role"]);
        assert_eq!(member["_wave"], 2, "{}", member["role"]);
        assert!(member.get("_launch").is_some(), "{}", member["role"]);
    }
}

/// The spec-kit shape: a spec member writes the feature folder, a plan
/// reviewer in its worktree runs analyze, and one implementer starts from the
/// spec branch. The team carries the feature as a `speckit` ref, and the
/// rules send the lead to the closer for converge.
#[test]
fn the_speckit_preset_runs_spec_then_analyze_then_impl() {
    let doc = preset_doc("speckit");
    let members = doc["children"].as_array().unwrap();
    let seats: Vec<(&str, &str, &str)> = members
        .iter()
        .map(|m| {
            (
                m["role"].as_str().unwrap(),
                m["kind"].as_str().unwrap(),
                m["mode"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        seats,
        [
            ("spec", "implementer", "worktree"),
            ("analyze", "plan_reviewer", "pane"),
            ("impl", "implementer", "worktree"),
        ]
    );
    let spec = &members[0];
    let spec_branch = spec["branch"].as_str().unwrap();
    assert!(spec.get("_launch_after").is_none());
    for later in &members[1..] {
        assert!(later.get("_launch_after").is_some(), "{}", later["role"]);
    }
    assert!(
        members[1]["cwd"]
            .as_str()
            .unwrap()
            .ends_with(&format!("/.worktrees/{}", spec_branch.replace('/', "-"))),
        "analyze starts in the spec member's worktree"
    );
    let imp = &members[2];
    assert_eq!(imp["base"], spec_branch);

    let refs = doc["refs"].as_array().unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0]["provider"], "speckit");
    let feature = refs[0]["id"].as_str().unwrap();
    let folder = format!("specs/{feature}");
    assert!(
        spec["owns"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o.as_str().unwrap().starts_with(&folder)),
        "the spec member owns the feature folder"
    );
    assert!(
        imp["owns"]
            .as_array()
            .unwrap()
            .contains(&Value::from(format!("{folder}/tasks.md"))),
        "the implementer owns tasks.md, where it ticks its tasks"
    );
    assert!(
        imp["owns"].as_array().unwrap().contains(&Value::from(".gitignore")),
        "implement writes .gitignore"
    );
    let setup = imp["setup"].as_str().unwrap();
    // Spec-kit reads only the key feature_directory.
    assert!(
        setup.contains(&format!("'{{\"feature_directory\": \"{folder}\"}}'"))
            && setup.contains("\"$1/.specify/feature.json\""),
        "{setup}"
    );
    let check = imp["check"].as_str().unwrap();
    // `! grep` passes on a missing file, so the check first needs tasks.md.
    assert!(
        check.starts_with(&format!("test -s {folder}/tasks.md && ")),
        "{check}"
    );
    assert!(
        check.contains(&format!(
            "! grep -nE '^- \\[ \\] T[0-9]+' {folder}/tasks.md"
        )),
        "{check}"
    );

    let rules: Vec<&str> = doc["_rules"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        rules
            .iter()
            .any(|r| r.contains("`converge`") && r.contains("`teamwire preset closer`")),
        "{rules:?}"
    );
    for needed in [
        "#4270",
        "`events:`",
        "`.specify/extensions/<id>/extension.yml`",
        "`.specify/integration-events.yml`",
        "N questions for the parent.",
        "ADR 0029",
    ] {
        assert!(
            rules.iter().any(|r| r.contains(needed)),
            "no rule names {needed}"
        );
    }
}

/// A spec-kit feature runs through members on three harnesses, so the
/// reference says how each one runs `implement`, and how a worktree member
/// finds its feature. The skill points at that section and says that
/// spec-kit's `specs/` folder is a product file a member commits.
#[test]
fn the_reference_says_how_each_harness_runs_spec_kit() {
    let reference = example("skills/teamwire/reference.md");
    let (head, _) = reference.split_once("\n## ").unwrap();
    assert!(
        head.contains("\"Spec-kit features\""),
        "the 'Read this when' list does not name the section"
    );
    let (_, section) = reference
        .split_once("\n## Spec-kit features\n")
        .unwrap_or_else(|| unreachable!("reference.md has no Spec-kit features section"));
    let section = section.split("\n## ").next().unwrap();
    for needed in [
        "`/speckit-implement <scope>`",
        "`$speckit-implement <scope>`",
        "`.pi/prompts/speckit.implement.md`",
        "`.specify/feature.json`",
        "`.specify/memory/constitution.md`",
        "`N questions for the parent.`",
        "`converge`",
        "`teamwire preset closer`",
        "`teamwire decisions --ref speckit:NNN-name`",
        "#4270",
        "`events:`",
        "`.specify/extensions/<id>/extension.yml`",
        "`.specify/integration-events.yml`",
        "`\"__speckit_event__\": true`",
    ] {
        assert!(section.contains(needed), "the section lacks {needed}");
    }
    // A per-story grep cannot pass in wave 1 and misses converge's tasks,
    // which carry no story label. The joined branch's check holds the grep.
    let flat = section.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(!flat.contains("[US1\\]"), "the section has a per-story grep");
    assert!(
        flat.contains("The integrator's `check` on the joined branch"),
        "the section does not move the unticked-task grep to the integrator"
    );
    let skill = example("skills/teamwire/SKILL.md");
    assert!(
        skill.contains("\"Spec-kit features\" in `reference.md`"),
        "the skill does not point at the section"
    );
    assert!(
        skill.contains("Spec-kit's `specs/` folder"),
        "the skill does not say where spec-kit's files go"
    );
}

/// A gate that fails on the base before any merge is the base's result, not
/// a reason for the integrator to stop as blocked.
#[test]
fn the_integrator_records_a_failing_base_and_carries_on() {
    let doc = preset_doc("integrator");
    let task = task_of(&doc, "integrator");
    assert!(task.contains("as the base's result"), "{task}");
    assert!(task.contains("do not stop as blocked"), "{task}");
    assert!(!preset_text("integrator").contains("426"));
}

/// One fresh review per branch, and none of the merged change.
#[test]
fn the_dependent_tickets_ask_for_no_second_review_of_the_merge() {
    let text = preset_text("dependent-tickets");
    assert!(!text.contains("one fresh reviewer reads the whole change"));
    assert!(text.contains("no second review of the merged change"));
}

/// The reviewer checks the diff against the implementer's brief, so its task
/// names where launch keeps that brief.
#[test]
fn the_reviewer_task_names_the_implementers_brief() {
    let doc = preset_doc("implement-then-review");
    let task = task_of(&doc, "review");
    assert!(
        task.contains("$TEAMWIRE_HOME/teams/myteam/briefs/impl.md"),
        "{task}"
    );
    assert!(task.contains("quoting the brief line"), "{task}");
}

/// teamwire stores no member's setup command where the closer can read it,
/// so the task lists them. The reset stays first, with a named-file restore
/// when a repository hook refuses it.
#[test]
fn the_closer_task_lists_setups_and_has_a_restore_fallback() {
    let doc = preset_doc("closer");
    let task = task_of(&doc, "closer");
    assert!(task.contains("Members' setup commands: CHANGEME"), "{task}");
    assert!(task.contains("git reset --hard"), "{task}");
    assert!(
        task.contains("git restore --source=HEAD --staged --worktree -- <files>"),
        "{task}"
    );
    assert!(task.contains("-e <path>"), "{task}");
}

/// The tdd and diagnosing-bugs skills are Claude Code plugins, so every note
/// that names them says which members it is for.
#[test]
fn the_skill_notes_name_the_members_they_are_for() {
    let mut seen = 0;
    for name in NAMES {
        for member in preset_doc(name)["children"].as_array().unwrap() {
            let note = member["_note"].as_str().unwrap_or_default();
            if note.contains("tdd") {
                assert!(
                    note.contains("For a Claude Code member with mattpocock-skills on"),
                    "{name}/{}: {note}",
                    member["role"]
                );
                seen += 1;
            }
        }
    }
    assert!(seen >= 6, "{seen} notes name the tdd skill");
}

/// A Codex or pi lead reads its own lead file where the skill quotes a line
/// that names Monitor. The lines are the ones the program prints for that
/// harness (plan D7 and D17), and the pull-request watch is the one each
/// harness can run (D12, D13).
#[test]
fn the_codex_and_pi_lead_files_quote_their_own_watch_lines() {
    let codex = example("skills/codex-lead.md");
    let bg = "nohup teamwire watch <team> --deliver >/dev/null 2>&1 &";
    for line in [
        format!("watch: {bg}"),
        format!(
            "[teamwire:<team>] no watch is running; start it in the background with `{bg}` before you stop."
        ),
        format!("(no watch is running: {bg})"),
        format!("no watch is running: start one with `{bg}`"),
        "nohup teamwire pr-watch --deliver <team> -R OWNER/REPO -i 45 3453 3454 >/dev/null 2>&1 &"
            .to_owned(),
        "teamwire lead --adopt <team>".to_owned(),
        "teamwire install --harness codex".to_owned(),
    ] {
        assert!(codex.contains(&line), "codex-lead.md lacks {line}");
    }
    let pi = example("skills/pi-lead.md");
    for line in [
        "watch: call the teamwire_watch tool with team <team>",
        "[teamwire:<team>] no watch is running; start it with the teamwire_watch tool (team \"<team>\") before you stop.",
        "(no watch is running: call the teamwire_watch tool with team <team>)",
        "no watch is running: start one with the teamwire_watch tool (team <team>)",
        "`teamwire_pr_watch`",
        "`repo`",
        "`prs`",
        "`interval`",
        "refuses `teamwire pr-watch` through bash",
        "/skill:promptwire",
        "teamwire install --harness pi",
    ] {
        assert!(pi.contains(line), "pi-lead.md lacks {line}");
    }
}

/// The lead skill and its reference file state each rule the instruction
/// audit of 2026-11-05 found missing or wrong. One phrase per rule.
#[test]
fn the_skill_states_the_rules_the_audit_found_missing() {
    let skill = example("skills/teamwire/SKILL.md");
    let reference = example("skills/teamwire/reference.md");
    let both = format!("{skill}\n{reference}");
    for phrase in [
        "## The run in order",
        "`TEAM-COMPLETE 3/3 done. If no wave is due: add the integrator when branches join, then the closer (teamwire preset closer).`",
        "warning: no closer",
        "a report that starts with\n    a summary of at most ten lines",
        "`Questions for the parent`, right after its\n  summary",
        "grep the whole repository for the name and for its old value",
        "has\n   one owner. Name that owner in both briefs.",
        "apps that comment on the pull request",
        "list each member's `setup` in the\n   closer's task",
        "teamwire pr-watch --deliver <team> -R OWNER/REPO -i 45 3453 3454",
        "`teamwire_pr_watch`",
        "brought in by merge <sha> of <branch>",
        "A brief never offers it a worktree. It runs tests in its own clone.",
        "gets a waiver per flag",
        "teamwire turns\non auto-compact for every Claude Code member",
        "teamwire sets no compaction for Codex and pi",
        "Put the whole brief, the shared parts too, in each `task_file`.",
        "Each `--add` spec stands alone",
        "A login, update or trust dialog needs the user",
        "comes once more\n   on top of these",
        "command still hands off",
        "Launch counts every implementer not\n  yet removed",
        "This section describes Claude Code.",
        "For a Codex or pi member, write the tdd",
        "**A turn that died.**",
        "Verify runs `check` with `sh -c`",
        "AFK runs are the one exception",
        "`[teamwire:<team> msg <id> <type> from <sender>]`",
    ] {
        assert!(both.contains(phrase), "the skill lacks: {phrase}");
    }
    for gone in [
        "With auto-compact\noff",
        "426 outside-owns",
        "No table in teamwire names",
        "and nothing else. Other failures",
    ] {
        assert!(!both.contains(gone), "the skill still says: {gone}");
    }
    assert_eq!(
        reference
            .matches("Every watch line says when it was printed")
            .count(),
        0,
        "the timestamp paragraph lives in SKILL.md only"
    );
}

/// The AFK skill numbers its sections and the steps of section 1, so each
/// pointer says which it means, and only the lead sends a member more work.
#[test]
fn the_afk_skill_points_at_sections_and_allows_fix_rounds() {
    let skill = example("skills/afk/SKILL.md");
    assert!(!skill.contains("start step 2"), "{skill}");
    assert!(!skill.contains("runs once and finishes"), "{skill}");
    assert!(skill.contains("Section 1 is the only place"));
    assert!(skill.contains("write roster entry D1 (section 5)"));
    assert!(skill.contains("Only you send it more work"));
}

/// Fix round 1 (the consist review). The closer preset says what the
/// closer's seat says: which fixes it reverts, and a restore check that a
/// setup-made untracked link cannot break.
#[test]
fn the_closer_preset_matches_the_closer_seat() {
    let doc = preset_doc("closer");
    let task = task_of(&doc, "closer");
    let rules = doc["_rules"].as_array().unwrap();
    let rule = rules
        .iter()
        .filter_map(Value::as_str)
        .find(|r| r.starts_with("It proves tests can fail."))
        .unwrap();
    for text in [task, rule] {
        assert!(!text.contains("graded wrong conclusion"), "{text}");
        assert!(!text.contains("prints nothing"), "{text}");
        assert!(
            text.contains("a finding that a reviewer reported and an owner fixed"),
            "{text}"
        );
        assert!(text.contains("shows none of those files"), "{text}");
        assert!(text.contains("-e <path>"), "{text}");
    }
    let skill = example("skills/teamwire/SKILL.md");
    assert!(!skill.contains("graded wrong conclusion"));
}

/// Fix round 1. The lead knows each summary prefix a member may write
/// (plan D26, D27), and the run list, the handoff note and the pull-request
/// step agree with the rest of the skill.
#[test]
fn the_skill_reads_every_summary_prefix_and_agrees_with_itself() {
    // Line breaks fall anywhere in prose, so compare the words only.
    let flat = |text: String| text.split_whitespace().collect::<Vec<_>>().join(" ");
    let skill = flat(example("skills/teamwire/SKILL.md"));
    let reference = flat(example("skills/teamwire/reference.md"));
    for phrase in [
        "starts with `nothing to do:`",
        "waive it with the member's report as the reason",
        "A handoff summary always starts with `handoff:`",
        "at most two fix rounds per owner, then `teamwire verify`",
        "steered into its session",
    ] {
        assert!(skill.contains(phrase), "the skill lacks: {phrase}");
    }
    assert!(!skill.contains("one fix round per owner"));
    assert!(!skill.contains("has the lines typed into its pane instead"));
    assert!(
        reference.contains("A role that does not write code commits nothing."),
        "{reference}"
    );
    assert!(!reference.contains("finish its step, commit, run"));
    let afk = flat(example("skills/afk/SKILL.md"));
    assert!(afk.contains("warning: no closer reported"), "{afk}");
    assert!(afk.contains("Close anyway"), "{afk}");
}

/// Fix round 2. `teamwire close` archives the team in the same command, so
/// an AFK lead has the reporter list a missing closer before it closes.
#[test]
fn the_afk_skill_lists_a_missing_closer_before_the_close() {
    let afk = example("skills/afk/SKILL.md");
    let (_, section) = afk.split_once("## 7. Report and close").unwrap();
    let listed = section.find("list the missing closer").unwrap();
    let close = section.find("Run `teamwire close <team>`").unwrap();
    assert!(listed < close, "{section}");
    assert!(!section.contains("before you close."), "{section}");
}
