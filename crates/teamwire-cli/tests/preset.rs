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

#[path = "preset/run.rs"]
mod run;

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

/// A preset's `_rules`, joined with spaces.
fn rules_text(name: &str) -> String {
    let doc = preset_doc(name);
    let rules: Vec<&str> = doc["_rules"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    rules.join(" ")
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
    // At a question stop the spec member's check fails, so it records
    // blocked; nothing excuses a failing check with done.
    let note = spec["_note"].as_str().unwrap();
    assert!(!note.contains("is expected"), "{note}");
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
        imp["owns"]
            .as_array()
            .unwrap()
            .contains(&Value::from(".gitignore")),
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
            "! grep -nE '^[[:space:]]*[-*+][[:space:]]+\\[[[:space:]]*\\][[:space:]]*T[0-9]+' {folder}/tasks.md"
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
        "`teamwire done --status blocked --summary \"3 questions for the parent. ...\"`",
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
        "`teamwire done --status blocked --summary \"3 questions for the parent. ...\"`",
        "a question round, not a failure",
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
    assert!(
        !flat.contains("[US1\\]"),
        "the section has a per-story grep"
    );
    assert!(
        flat.contains("The integrator's `check` on the joined branch"),
        "the section does not move the unticked-task grep to the integrator"
    );
    // The closer's own session resolves the lead's checkout, so it follows
    // the command file in its clone instead.
    // clarify asks questions of its own, so the spec member stops once per
    // batch and never answers a spec-kit question itself.
    let flat_rules = rules_text("speckit")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for text in [flat.as_str(), &flat_rules] {
        for needed in [
            "stops with its questions from `specify`, at most 3",
            "stops again with clarify's questions, at most 5, each with clarify's recommended option",
            "A further stop is allowed only when an answer raises a new question",
            "never answers a spec-kit question itself, unless your reprompt says to take the recommended option",
            "lists each such answer under Decisions I made",
        ] {
            assert!(text.contains(needed), "lacks {needed}: {text}");
        }
    }
    // Each fix is pinned in every place it changed. `! grep` passes on a
    // missing file, so every grep example starts with `test -s`.
    assert!(
        flat.contains("`test -s specs/NNN-name/tasks.md && <test command> && ! grep -nE '^[[:space:]]*[-*+][[:space:]]+\\[[[:space:]]*\\][[:space:]]*T[0-9]+' specs/NNN-name/tasks.md`"),
        "the reference's check example lacks test -s"
    );
    assert!(
        flat_rules.contains("`test -s specs/NNN-name/tasks.md && <tests> && ! grep -nE"),
        "the BUILD rule's check example lacks test -s"
    );
    assert!(
        flat_rules
            .contains("The integrator's `check` on the joined branch carries the whole-file grep"),
        "the SPLIT rule does not move the grep to the integrator"
    );
    assert!(
        flat.contains("**Ignore files.** `implement` writes `.gitignore`")
            && flat.contains("`.dockerignore`")
            && flat.contains("`.eslintignore`"),
        "the reference lacks the Ignore files rule"
    );
    // A tasks.md that no member wrote gets the fresh read too.
    let input = preset_doc("speckit")["_input"].as_str().unwrap().to_owned();
    for text in [flat.as_str(), &input] {
        assert!(
            text.contains(
                "the plan reviewer still runs `analyze` on it before any implementer starts"
            ),
            "{text}"
        );
    }
    // The user's checklist ticks live in the spec member's worktree until it
    // commits them, and the implementer's worktree holds only commits.
    for text in [flat.as_str(), &rules_text("speckit")] {
        assert!(
            text.contains("reprompt the spec member to commit them before any implementer starts"),
            "{text}"
        );
    }
    // The closer's converge section stays in its clone, so an owner's fix
    // round starts with its own converge.
    let adr = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/adr/0041-spec-kit-features-run-through-teamwire-seats.md"),
    )
    .unwrap();
    let adr = adr.split_whitespace().collect::<Vec<_>>().join(" ");
    // Each spec member scans specs/ in its own worktree, so the race is not
    // only between members that run at once.
    for text in [flat.as_str(), &flat_rules, &adr] {
        assert!(
            text.contains("scans `specs/` in its own worktree")
                && text.contains("even when they run one after the other")
                && !text.contains("scan at once"),
            "{text}"
        );
    }
    // The ADR states the check rule the reference and the SPLIT rule state.
    assert!(
        adr.contains("only the integrator's `check` on the joined branch greps the whole file")
            && !adr.contains("fails on any unticked task of its phases"),
        "{adr}"
    );
    for text in [flat.as_str(), &rules_text("speckit"), &adr] {
        assert!(
            text.contains("runs `converge` on its own branch, then `implement` on the new phase, then `converge` again"),
            "{text}"
        );
        // On a story branch converge reports the other stories as missing.
        assert!(
            text.contains(
                "in a split the fix round runs on a branch cut from the integrator's branch"
            ) && text.contains("story owners run no `converge`"),
            "{text}"
        );
        assert!(!text.contains("`implement` then `converge`"), "{text}");
        assert!(!text.contains("`implement`, then `converge`"), "{text}");
    }
    for text in [flat.as_str(), &rules_text("speckit")] {
        assert!(
            text.contains("`.claude/skills/speckit-converge/SKILL.md`")
                && text.contains("never invokes `/speckit-converge`"),
            "{text}"
        );
    }
    let skill = example("skills/teamwire/SKILL.md");
    assert!(
        skill.contains("\"Spec-kit features\" in `reference.md`"),
        "the skill does not point at the section"
    );
    let flat_skill = skill.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat_skill
            .contains("starts with `N questions for the parent.`, or `1 question for the parent.`, after an optional `handoff: `, is a question round, not a failure"),
        "the skill does not say how to read the spec member's blocked result"
    );
    // Split so that `git grep` for the retired wording finds only ADR 0041.
    let retired = format!("questions for the {}", "lead");
    for (place, text) in [
        ("reference", flat.as_str()),
        ("skill", &flat_skill),
        ("rules", &flat_rules),
    ] {
        assert!(!text.contains(&retired), "{place} still says {retired}");
    }
    assert!(
        reference
            .contains("> At spec-kit's question stops, this brief replaces your seat's rule on"),
        "the reference lacks the override line"
    );
    // A member's shell may have no `teamwire` on its PATH, so the override
    // line, which the spec member reads, names the completion, not a bare
    // command. The quote marks that start each line of the block are dropped.
    let unquoted = flat.replace(" > ", " ");
    let (_, override_line) = unquoted
        .split_once("At spec-kit's question stops,")
        .unwrap();
    let (override_line, _) = override_line
        .split_once("reprompts you with the answers.")
        .unwrap();
    assert!(
        override_line.contains(
            "record your completion with status `blocked` and the summary \
             `N questions for the parent. ...` with your count"
        ) && !override_line.contains("teamwire done"),
        "the override line names a bare teamwire: {override_line}"
    );
    assert!(
        skill.contains("Spec-kit's `specs/` folder"),
        "the skill does not say where spec-kit's files go"
    );
}

/// The text with every run of whitespace made one space, so a phrase matches
/// across a line break.
fn flat(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The reference's "Spec-kit features" section, flat.
fn speckit_section() -> String {
    let reference = example("skills/teamwire/reference.md");
    let (_, section) = reference
        .split_once("\n## Spec-kit features\n")
        .unwrap_or_else(|| unreachable!("reference.md has no Spec-kit features section"));
    flat(section.split("\n## ").next().unwrap())
}

/// ADR 0041, flat.
fn adr_0041() -> String {
    flat(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../docs/adr/0041-spec-kit-features-run-through-teamwire-seats.md"),
        )
        .unwrap(),
    )
}

/// The pilot showed what the spec stage really does: `specify` ranks no
/// option, `clarify` cannot take a batch of answers, both batches need one
/// place in `spec.md`, and `specify` ticks its own checklist.
#[test]
fn the_speckit_spec_stage_states_what_the_pilot_showed() {
    let rules = flat(&rules_text("speckit"));
    let section = speckit_section();
    let adr = adr_0041();
    for (place, text) in [("rules", &rules), ("reference", &section), ("adr", &adr)] {
        for needed in [
            "`specify` ranks no option",
            "marks its own recommended option for each question and says the mark is its own",
            "follows clarify's write steps by hand",
            "`## Clarifications`",
            "`### Session YYYY-MM-DD`",
            "`- Q: <question> → A: <answer>`",
        ] {
            assert!(text.contains(needed), "{place} lacks {needed}: {text}");
        }
        assert!(
            !text.contains("records those answers through `clarify`"),
            "{place} still says clarify records a batch: {text}"
        );
    }
    // ADR 0041 keeps its words; ADR 0042 records that `specify` leaves an
    // item unticked while the spec holds a question.
    assert!(
        adr.contains("`specify` writes `checklists/requirements.md` itself and ticks its items when it validates the spec"),
        "{adr}"
    );
    for (place, text) in [("rules", &rules), ("reference", &section)] {
        for needed in [
            "`specify` writes `checklists/requirements.md` itself and ticks each item that passes its validation",
            "re-runs `specify`'s validation, and ticks each item of `checklists/requirements.md` that now passes",
            "replaces each `[NEEDS CLARIFICATION]` marker with its answer",
            "A stop with no questions records no stop",
            "or `1 question for the parent.` for one",
            "matches `^[1-9][0-9]* questions? for the parent\\.`",
            "override line",
            "the member's own mark at stop 1 and clarify's at stop 2",
            "so `spec.md` has one place for every answer",
            "The reviewer-only rule is about the `checklist` command's checklists",
            "So the stop fires only when a checklist has unticked items",
        ] {
            assert!(text.contains(needed), "{place} lacks {needed}: {text}");
        }
    }
}

/// The pilot's build: `tasks.md` has checkpoints only after story phases,
/// `implement` appends ignore patterns, a "confirm the tests fail" task can be
/// half false, and the check's grep misses a lowercase tick or a deleted line.
#[test]
fn the_speckit_build_states_what_the_pilot_showed() {
    let doc = preset_doc("speckit");
    let rules = flat(&rules_text("speckit"));
    let section = speckit_section();
    let adr = adr_0041();
    let impl_note = flat(
        doc["children"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["role"] == "impl")
            .and_then(|m| m["_note"].as_str())
            .unwrap(),
    );
    for (place, text) in [
        ("rules", &rules),
        ("reference", &section),
        ("adr", &adr),
        ("impl note", &impl_note),
    ] {
        assert!(
            text.contains("at the end of every phase that has tasks"),
            "{place} lacks the phase commit rule: {text}"
        );
        for old in ["at each checkpoint", "per checkpoint", "phase checkpoint"] {
            assert!(!text.contains(old), "{place} still says {old}: {text}");
        }
    }
    for (place, text) in [("rules", &rules), ("reference", &section)] {
        for needed in [
            "a `**Checkpoint**` line after the Foundational phase and after each story phase, so Setup and Polish have none",
            "A phase with no tasks, such as an empty Foundational phase, gets no commit",
            "overrides any commit line in the notes of `tasks.md`, such as the template's \"Commit after each task or logical group\"",
            "follows the ignore-file step for `.gitignore` only, appends only the patterns the step lists, lists each one it added in its report, and writes no other ignore file unless its brief owns that file",
            "runs any task in `tasks.md` that claims \"confirm the new tests fail\"",
            "a \"confirm the tests fail\" task is finished when the cases it names fail",
            "never changes a test to make it fail",
            "counts the ticks at each of the implementer's commits and checks that no task line is missing",
            "passes on a lowercase `[x]` or a deleted task line",
        ] {
            assert!(text.contains(needed), "{place} lacks {needed}: {text}");
        }
    }
    let analyze = flat(task_of(&doc, "analyze"));
    assert!(
        analyze.contains("claims \"confirm the new tests fail\"")
            && analyze.contains("in your own scratch copy"),
        "{analyze}"
    );
}

/// The pilot's fix round: converge cannot see a gap the spec itself
/// carries, converge commits nothing, and worktrees share no files, so the
/// spec member announces each spec change as a commit to merge.
#[test]
fn the_speckit_fix_round_states_what_the_pilot_showed() {
    let doc = preset_doc("speckit");
    let note_of = |role: &str| {
        flat(
            doc["children"]
                .as_array()
                .unwrap()
                .iter()
                .find(|m| m["role"] == role)
                .and_then(|m| m["_note"].as_str())
                .unwrap(),
        )
    };
    let rules = flat(&rules_text("speckit"));
    let section = speckit_section();
    let adr = adr_0041();
    for (place, text) in [("rules", &rules), ("reference", &section), ("adr", &adr)] {
        for needed in [
            "A finding in the spec's own words goes to the spec member first",
            "cannot find a gap that the spec itself carries",
            "changes the spec and design files and commits, without touching `tasks.md`",
            "Converge makes no commit",
            "commits converge's appended section alone, before it implements it, with a message such as `converge round 1: ...`",
            "SPEC AMENDMENT:",
            "Once an implementer starts, only implementers write `tasks.md`",
            "The spec member never edits `tasks.md` again",
            // verify starts the owns diff after a merge only from `base`, so
            // a split member whose base is not the spec branch is flagged.
            "verify does not flag the merged spec files for the one implementer or for a split's wave 1 member, because their `base` is the spec branch",
            "verify flags each merged spec file as `outside-owns:specs/NNN-name/<file>`",
        ] {
            assert!(text.contains(needed), "{place} lacks {needed}: {text}");
        }
        assert!(
            !text.contains("verify does not flag the merged spec files, because"),
            "{place} says no member is flagged: {text}"
        );
    }
    for (place, text) in [("rules", &rules), ("reference", &section)] {
        let waive = "`teamwire verify <team> --role <role> --waive outside-owns:specs/NNN-name/spec.md --reason \"SPEC AMENDMENT notice m007\"`";
        assert!(text.contains(waive), "{place} lacks the waiver: {text}");
    }
    for (place, text) in [("rules", &rules), ("reference", &section)] {
        for needed in [
            "`teamwire send <team> --to <implementers>,lead --type notice --commit <sha> --text \"SPEC AMENDMENT: run 'git merge --no-ff --no-edit <sha>' now. <what changed and why>\"`",
            "on a notice that starts with `SPEC AMENDMENT:`, merge that commit before your next unit",
            "The lead sends nothing",
            "Uncommitted edits in the spec member's worktree never reach anyone",
        ] {
            assert!(text.contains(needed), "{place} lacks {needed}: {text}");
        }
    }
    let spec_note = note_of("spec");
    assert!(
        spec_note.contains("SPEC AMENDMENT:")
            && spec_note.contains(
                "after every commit that changes `spec.md`, `plan.md` or a design file once an implementer has started"
            ),
        "{spec_note}"
    );
    let impl_note = note_of("impl");
    assert!(impl_note.contains("`SPEC AMENDMENT:`"), "{impl_note}");
    // The user chose the merge over a shared specs folder; the ADR keeps why.
    for needed in [
        "shared specs folder outside the repository",
        "verify checks `owns` only inside the repository",
    ] {
        assert!(adr.contains(needed), "the ADR lacks {needed}: {adr}");
    }
}

/// A fast-forward merge puts the sender's commit on the member's own line,
/// where verify and `done` count it as the member's commit. So every
/// amendment notice merges with `--no-ff`, and each place says why.
#[test]
fn every_amendment_notice_merges_with_no_ff() {
    for (place, text) in [
        ("skill", flat(&example("skills/teamwire/SKILL.md"))),
        ("reference", flat(&example("skills/teamwire/reference.md"))),
        ("contract-split", flat(&preset_text("contract-split"))),
        ("speckit", flat(&preset_text("speckit"))),
    ] {
        assert!(
            text.contains("git merge --no-ff --no-edit <sha>"),
            "{place} lacks the --no-ff merge"
        );
        assert!(
            !text.contains(&format!("git merge {}<sha>", "--no-edit ")),
            "{place} still merges without --no-ff"
        );
        assert!(
            text.contains("`--no-ff` makes the merge a merge commit even"),
            "{place} does not say why"
        );
    }
}

/// Doctor now checks the BEFORE rule, so the lead reads its `spec-kit`
/// lines instead of checking by hand, and `specify init` installs a
/// workflow that nothing runs. Close waits only for a closer that started.
#[test]
fn the_speckit_before_rule_and_the_close_sentence_state_what_the_pilot_showed() {
    let rules = flat(&rules_text("speckit"));
    let section = speckit_section();
    for (place, text) in [("rules", &rules), ("reference", &section)] {
        for needed in [
            "`teamwire doctor --base <base>` from the repository",
            "read its nine `spec-kit ...` lines",
            "`spec-kit version`",
            "`spec-kit hooks`",
            "`spec-kit committed`",
            "`spec-kit constitution`",
            "`spec-kit integrations`",
            "`spec-kit git extension`",
            "`spec-kit events`",
            "`spec-kit feature pointer`",
            "`spec-kit workflows`",
            "`.specify/workflows/speckit/workflow.yml`",
            "`.specify/workflows/workflow-registry.json`",
            "Nothing runs it unless someone runs `specify workflow run`, and no teamwire member does",
        ] {
            assert!(text.contains(needed), "{place} lacks {needed}: {text}");
        }
        assert!(!text.contains("by hand: "), "{place}: {text}");
    }
    assert!(
        rules.contains("Do not git-ignore a command folder")
            && section.contains("No git ignore rule for a command folder"),
        "the guidance does not warn against ignoring a command folder"
    );
    assert!(
        !section.contains("Then check that spec-kit added nothing"),
        "{section}"
    );
    let adr = adr_0041();
    assert!(
        !adr.contains("checks the base branch by hand") && adr.contains("`teamwire doctor`"),
        "{adr}"
    );
    // Codex launch refuses either file (teamwire-harness-codex OWN_CONFIG_FILES).
    for needed in [
        "no `.codex/config.toml` or `.codex/hooks.json` exists",
        "Codex launch refuses a repository that holds `.codex/config.toml` or `.codex/hooks.json`",
    ] {
        assert!(rules.contains(needed), "rules lack {needed}: {rules}");
    }
    assert!(
        adr.contains(
            "Codex launch refuses a repository that holds `.codex/config.toml` or `.codex/hooks.json`"
        ),
        "{adr}"
    );
    let skill = flat(&example("skills/teamwire/SKILL.md"));
    assert!(
        skill.contains(
            "It also refuses any other member while a closer runs in the team and has not reported since its latest start or reprompt"
        ) && skill.contains("A team with no closer yet gets no closer refusal"),
        "the skill does not say when close waits for a closer"
    );
    assert!(
        !skill.contains("while the team's closer has not reported"),
        "the skill still says close waits for a closer that never started"
    );
}

/// Each row of the reference's doctor table says what a `warn` on that check
/// means, so a row that says the opposite fails here, not at a launch.
#[test]
fn the_reference_doctor_table_says_what_each_warn_means() {
    let reference = example("skills/teamwire/reference.md");
    let row = |check: &str| {
        let start = format!("| `spec-kit {check}` |");
        reference
            .lines()
            .find(|l| l.starts_with(&start))
            .unwrap_or_else(|| unreachable!("no doctor row for spec-kit {check}"))
            .to_owned()
    };
    for (check, meaning) in [
        (
            "base",
            "when git cannot resolve the `--base` ref to a commit",
        ),
        ("version", "records a version outside the 1.3 line, or none"),
        (
            "version",
            "tested with 1.3.3.dev0 (spec-kit commit c438494)",
        ),
        (
            "version",
            "run `specify integration upgrade <key>` for each integration, then commit on the ref",
        ),
        (
            "committed",
            "A spec-kit file is on disk but not in the commit, so a worktree member does not get it",
        ),
        ("committed", "Remove each ignore rule it names"),
        (
            "committed",
            "which also names each ignore file you edited, such as `git add .claude/skills/speckit-* .gitignore`",
        ),
        (
            "committed",
            "When the ref is not the checked-out branch, run `git switch <ref>` and commit there, or commit here and merge this branch into the ref",
        ),
        (
            "events",
            "only on disk also warns: remove it from the repository. A manifest",
        ),
        ("committed", "Run the `git add` it prints"),
        (
            "constitution",
            "in the commit still holds a template placeholder",
        ),
        (
            "constitution",
            "A written copy on disk that is not committed does not count",
        ),
        (
            "integrations",
            "has no command folder with `speckit-` command files in the commit",
        ),
        ("integrations", "in its current or its older format"),
        (
            "git extension",
            "lists the `git` extension under `installed:`",
        ),
        (
            "git extension",
            "Run `specify extension remove git`, then commit on the ref",
        ),
        ("hooks", "a hook that a member must run (`optional: false`)"),
        (
            "hooks",
            "A hook or extension with `enabled: false` does not count",
        ),
        (
            "hooks",
            "then run `specify extension remove <ext>` or set `optional: true` on the hook",
        ),
        ("events", "A spec-kit runtime event is set up in the commit"),
        (
            "events",
            "`.codex/hooks.json` that is only on disk also warns",
        ),
        (
            "events",
            "`.claude/settings.json` holds a hook tagged `\"__speckit_event__\": true`",
        ),
        ("events", "is `ok`, with a note"),
        (
            "feature pointer",
            "git does not ignore `.specify/feature.json`, so members would commit their pointer",
        ),
        (
            "feature pointer",
            "run `git rm --cached .specify/feature.json`, then commit on the ref",
        ),
        ("workflows", "Never a `warn`"),
        ("workflows", "teamwire starts none of them"),
    ] {
        let text = row(check);
        assert!(
            text.contains(meaning),
            "spec-kit {check} lacks {meaning}: {text}"
        );
    }
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

/// The closer runs only the local gate its task names, so the implementer is
/// not told that the closer runs the full suite.
#[test]
fn the_implementer_task_says_the_closer_runs_only_its_local_gate() {
    let doc = preset_doc("implement-then-review");
    let task = task_of(&doc, "impl");
    assert!(!task.contains("which the closer runs once"), "{task}");
    assert!(
        task.contains(
            "do not run the full suite or the repo-wide lint and type gates. \
             The closer runs the local gate its task names once, and CI runs the rest."
        ),
        "{task}"
    );
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

/// What the review found missing: other extensions' mandatory hooks, the
/// scratch folder, the watch during a question round, and the two flags a
/// per-story split raises by design.
#[test]
fn the_speckit_guidance_covers_hooks_scratch_the_watch_and_split_flags() {
    let rules = flat(&rules_text("speckit"));
    let section = speckit_section();
    let reference = flat(&example("skills/teamwire/reference.md"));
    let skill = flat(&example("skills/teamwire/SKILL.md"));
    for (place, text) in [("rules", &rules), ("reference", &section)] {
        for needed in [
            "`optional: false`",
            "remove the extension or make the hook optional, then commit",
            "`specs/NNN-name/` in the integrator's `owns`",
        ] {
            assert!(text.contains(needed), "{place} lacks {needed}: {text}");
        }
    }
    assert!(
        section.contains("Doctor's `spec-kit hooks` line"),
        "{section}"
    );
    assert!(
        section.contains("both own overlapping paths (specs/NNN-name/tasks.md")
            && section.contains("Expect that warning, which does not refuse the launch"),
        "{section}"
    );
    assert!(
        rules.contains("expect that warning, which refuses nothing"),
        "{rules}"
    );
    for (place, text) in [("skill", &skill), ("reference", &reference)] {
        assert!(
            text.contains("`$TEAMWIRE_SCRATCH`"),
            "{place} lacks the scratch folder"
        );
    }
    assert!(
        skill.contains("never in the repository or in `/tmp`")
            && skill.contains("A question round does not end the watch")
            && skill.contains("That holds for a member of any kind")
            && skill.contains("cannot print until you reprompt the member or remove it"),
        "{skill}"
    );
    assert!(
        reference.contains("A member of any kind waiting in a question round")
            && reference
                .contains("so this line cannot print until you reprompt that member or remove it")
            && section.contains("and the watch keeps running"),
        "{reference}"
    );
}

/// AFK needs a reviewed spec, the reference says which parts no run has
/// tested, the tested build carries the label doctor prints, and a missing
/// token count is not read as zero.
#[test]
fn the_speckit_guidance_states_afk_untested_parts_and_the_tested_build() {
    let rules = flat(&rules_text("speckit"));
    let section = speckit_section();
    let afk = flat(&example("skills/afk/SKILL.md"));
    for (place, text) in [("rules", &rules), ("reference", &section), ("afk", &afk)] {
        assert!(
            text.contains("only from a `spec.md`, `plan.md` and `tasks.md` that the user already reviewed and committed"),
            "{place} lacks the AFK rule: {text}"
        );
    }
    assert!(afk.contains("stop before step 2"), "{afk}");
    for needed in [
        "No run has tested two parts of this section",
        "A split per user story",
        "Codex as a member",
        "spec-kit 1.3.3.dev0 at commit c438494",
    ] {
        assert!(section.contains(needed), "the section lacks {needed}");
    }
    assert!(!section.contains("v1.3.2 and 9 commits"), "{section}");
    let reference = flat(&example("skills/teamwire/reference.md"));
    assert!(
        reference.contains("read a missing count as not recorded, never as zero"),
        "the metrics row reads a missing count as zero"
    );
}

/// ADR 0047 rule 5: archived records name paths but are not live files, so
/// the preset's BUILD rule, the implementer's note and the AFK skill each
/// tell the lead to leave them out of the draft's mention search, as the
/// lead skill does.
#[test]
fn the_speckit_draft_runs_pass_exclude_for_archived_records() {
    let doc = preset_doc("speckit");
    let build = doc["_rules"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|r| r.starts_with("BUILD."))
        .map(flat)
        .unwrap();
    let impl_note = flat(
        doc["children"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["role"] == "impl")
            .and_then(|m| m["_note"].as_str())
            .unwrap(),
    );
    let afk = flat(&example("skills/afk/SKILL.md"));
    let needed = "`--exclude '<folder>/**'` for each folder of archived records";
    for (place, text) in [
        ("BUILD rule", &build),
        ("impl note", &impl_note),
        ("afk", &afk),
    ] {
        assert!(text.contains(needed), "{place} lacks {needed}: {text}");
    }
}

/// ADR 0042 records what the review changed, and what is deferred, inside
/// the repository; ADR 0041 keeps its words and points to it.
#[test]
fn adr_0042_supersedes_the_changed_parts_of_adr_0041() {
    let adr = flat(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../docs/adr/0042-spec-kit-after-the-review.md"),
        )
        .unwrap(),
    );
    for needed in [
        "supersedes ADR 0041 decision 7",
        "`teamwire speckit draft`",
        "spec-kit's `bug` extension",
        "its `assess` extension",
        "`1 question for the parent.`",
        "`^[1-9][0-9]* questions? for the parent\\.`",
        "`git merge --no-ff --no-edit <sha>`",
        "`teamwire doctor --base REF`",
        "`spec-kit version`",
        "`spec-kit hooks`",
        "`TEAMWIRE_SCRATCH`",
        "AFK runs spec-kit only from a reviewed spec",
        "supersedes ADR 0041's \"The pilot is their first test\"",
    ] {
        assert!(adr.contains(needed), "ADR 0042 lacks {needed}");
    }
    let old = adr_0041();
    let status = old.split("## Context").next().unwrap();
    assert!(status.contains("Partly superseded by ADR 0042"), "{status}");
}

/// `specify` can leave a `requirements.md` item unticked on purpose, so the
/// lead reads that checklist before the implementer starts and waives or
/// fixes each open item, instead of meeting implement's stop later.
#[test]
fn the_speckit_preset_has_the_lead_read_the_checklist_before_the_implementer() {
    let rules = rules_text("speckit");
    for wanted in [
        "checklists/requirements.md",
        "before the implementer launches",
        "waive",
    ] {
        assert!(rules.contains(wanted), "{wanted}: {rules}");
    }
}

/// F14: a spec-kit removal on the implementer's own branch fails every later
/// verify, so the preset and the reference say where the removal goes, and
/// that `teamwire amend` swaps the check when it is already there.
#[test]
fn the_speckit_preset_puts_the_removal_on_the_pr_branch_and_names_amend() {
    let rules = rules_text("speckit");
    let reference = speckit_section();
    for wanted in [
        "REMOVE SPEC-KIT",
        "pull request branch",
        "last verified head",
        "never on the implementer's own branch",
        "teamwire amend <team> --role <role> --check",
    ] {
        assert!(rules.contains(wanted), "preset: {wanted}: {rules}");
        assert!(
            reference.contains(wanted),
            "reference: {wanted}: {reference}"
        );
    }
}

/// Some gates run only in CI (a `SonarCloud` scan, a deploy check). The closer
/// task names the gate it runs locally and the gates it leaves to CI, and
/// reports both, so a closer never claims a gate it did not run.
#[test]
fn the_closer_task_has_a_local_gate_and_a_ci_gate_slot_and_reports_both() {
    let doc = preset_doc("closer");
    let task = task_of(&doc, "closer");
    assert!(task.contains("Local gate: CHANGEME"), "{task}");
    assert!(task.contains("Gates left to CI: CHANGEME"), "{task}");
    assert!(
        task.contains("report which gate ran, with its exit code, and which gates were left to CI"),
        "{task}"
    );
}

/// Every text that sends the closer to work names its local gate and the
/// gates left to CI, as the closer preset does, never "the full gate". The
/// AFK skill is the exception: an AFK run pushes nothing, so no CI runs after
/// it, and its branch must pass the full gate (ADR 0025 point 10). So the AFK
/// closer's local gate slot holds the full gate.
#[test]
fn the_closer_runs_the_local_gate_in_every_preset_and_the_afk_skill() {
    for name in ["implement-then-review", "parallel-tickets"] {
        let rules = rules_text(name);
        assert!(!rules.contains("runs the full gate"), "{name}: {rules}");
        assert!(
            rules.contains("local gate its task names"),
            "{name}: {rules}"
        );
        assert!(
            rules.contains("gates the task leaves to CI"),
            "{name}: {rules}"
        );
    }
    let skill = example("skills/afk/SKILL.md");
    assert!(skill.contains("repository's full gate on each branch"));
    assert!(!skill.contains("local gate its task names on each branch"));
    let afk = flat(&skill);
    assert!(
        afk.contains(
            "Its `Local gate:` slot holds the full gate, such as `scripts/gate.sh`, and its \
             `Gates left to CI:` slot holds `none`, because an AFK run pushes nothing."
        ),
        "{afk}"
    );
    assert!(
        afk.contains(
            "Only a gate that needs a service this machine lacks, such as SonarCloud, \
             goes in the CI slot, with a `D` entry and a line under **Risks**."
        ),
        "{afk}"
    );
}
