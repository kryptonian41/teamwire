//! The spec-kit checks of `teamwire doctor`: printed only inside a git
//! repository whose root holds `.specify/`, each `ok` or `warn`, never a
//! failure. The in-process tests give git's answers through the fake
//! platform; the binary test asks a real git.

use super::*;

/// A constitution `/speckit-constitution` filled in: no placeholder left.
const FILLED: &str = "# wordtool Constitution\n\n## Core Principles\n\n### I. Small CLI\n\
                      Every feature is a subcommand.\n\n**Version**: 1.0.0 | **Ratified**: 2026-11-10\n";

/// The pilot repository's `.specify/integration.json`, as `specify init`
/// and `specify integration install pi` wrote it.
const INTEGRATIONS: &str = r#"{
  "version": "1.3.3.dev0",
  "integration_state_schema": 1,
  "installed_integrations": [
    "claude",
    "pi"
  ],
  "integration_settings": {
    "claude": {"script": "sh", "invoke_separator": "-"},
    "pi": {"script": "sh", "invoke_separator": "."}
  },
  "integration": "claude",
  "default_integration": "claude"
}"#;

/// The pilot repository's `.specify/workflows/workflow-registry.json`.
const WORKFLOWS: &str = r#"{
  "schema_version": "1.0",
  "workflows": {
    "speckit": {
      "name": "Full SDD Cycle",
      "version": "1.0.1",
      "source": "bundled"
    }
  }
}"#;

/// A healthy bed whose repository is a spec-kit project as `specify init`
/// leaves it for Claude Code and pi: a `.git`, a filled constitution, both
/// command folders and the bundled workflow. git answers yes to both
/// questions.
fn speckit_bed() -> Bed {
    let mut bed = Bed::healthy();
    let root = bed.home.path();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    put(root, ".specify/memory/constitution.md", FILLED);
    put(root, ".specify/integration.json", INTEGRATIONS);
    put(root, ".specify/workflows/workflow-registry.json", WORKFLOWS);
    put(root, ".claude/skills/speckit-plan/SKILL.md", "plan");
    put(root, ".pi/prompts/speckit.plan.md", "plan");
    bed.speckit_git = vec![0, 0];
    bed
}

fn put(root: &Path, file: &str, text: &str) {
    let path = root.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn speckit_names(checks: &[Check]) -> Vec<&str> {
    names(checks)
        .into_iter()
        .filter(|n| n.starts_with("spec-kit"))
        .collect()
}

#[test]
fn outside_a_spec_kit_repository_doctor_prints_no_spec_kit_line() {
    // A git repository without `.specify/`.
    let bed = Bed::healthy();
    std::fs::create_dir_all(bed.home.path().join(".git")).unwrap();
    assert_eq!(speckit_names(&bed.checks()), Vec::<&str>::new());

    // A `.specify/` outside any git repository.
    let bed = Bed::healthy();
    put(bed.home.path(), ".specify/memory/constitution.md", FILLED);
    assert_eq!(speckit_names(&bed.checks()), Vec::<&str>::new());
}

#[test]
fn a_committed_constitution_and_an_ignored_pointer_are_ok_and_git_is_asked_at_the_root() {
    let bed = speckit_bed();
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "spec-kit committed"), Level::Pass);
    assert_eq!(level_of(&checks, "spec-kit feature pointer"), Level::Pass);
    let runs = bed.platform.runs();
    let git: Vec<(Vec<String>, Option<PathBuf>)> = runs
        .iter()
        .filter(|(c, _)| c.program == "git")
        .map(|(c, _)| (c.args.clone(), c.cwd.clone()))
        .collect();
    let root = Some(bed.home.path().to_path_buf());
    assert!(
        git.contains(&(
            vec![
                "ls-files".to_owned(),
                "--error-unmatch".to_owned(),
                "--".to_owned(),
                ".specify/memory/constitution.md".to_owned()
            ],
            root.clone()
        )),
        "{git:?}"
    );
    assert!(
        git.contains(&(
            vec![
                "check-ignore".to_owned(),
                "-q".to_owned(),
                ".specify/feature.json".to_owned()
            ],
            root
        )),
        "{git:?}"
    );
}

#[test]
fn an_untracked_constitution_and_a_pointer_git_does_not_ignore_warn_and_name_the_file() {
    let mut bed = speckit_bed();
    bed.speckit_git = vec![1, 1];
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "spec-kit committed"), Level::Warn);
    assert!(
        detail_of(&checks, "spec-kit committed").contains(".specify/memory/constitution.md"),
        "{checks:?}"
    );
    assert_eq!(level_of(&checks, "spec-kit feature pointer"), Level::Warn);
    assert!(
        detail_of(&checks, "spec-kit feature pointer").contains(".specify/feature.json"),
        "{checks:?}"
    );
}

#[test]
fn a_git_that_cannot_answer_is_a_warning() {
    let mut bed = speckit_bed();
    bed.speckit_git = vec![128, 128];
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "spec-kit committed"), Level::Warn);
    assert_eq!(level_of(&checks, "spec-kit feature pointer"), Level::Warn);
}

#[test]
fn a_constitution_with_a_template_placeholder_left_warns_and_names_it() {
    let bed = speckit_bed();
    assert_eq!(
        level_of(&bed.checks(), "spec-kit constitution"),
        Level::Pass
    );

    // The untouched template, as `specify init` writes it.
    put(
        bed.home.path(),
        ".specify/memory/constitution.md",
        "# [PROJECT_NAME] Constitution\n\n### [PRINCIPLE_1_NAME]\n[PRINCIPLE_1_DESCRIPTION]\n",
    );
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "spec-kit constitution"), Level::Warn);
    let detail = detail_of(&checks, "spec-kit constitution");
    assert!(detail.contains("[PROJECT_NAME]"), "{detail}");
    assert!(
        detail.contains(".specify/memory/constitution.md"),
        "{detail}"
    );
}

#[test]
fn brackets_that_are_not_template_placeholders_pass() {
    let bed = speckit_bed();
    put(
        bed.home.path(),
        ".specify/memory/constitution.md",
        "# wordtool Constitution\n- [x] done\n- [ ] open\nT001 [P] [US1] task\n\
         [NEEDS CLARIFICATION] and a [link](https://example.com) and [V1]\n",
    );
    assert_eq!(
        level_of(&bed.checks(), "spec-kit constitution"),
        Level::Pass
    );
}

#[test]
fn a_missing_constitution_warns() {
    let bed = speckit_bed();
    std::fs::remove_file(bed.home.path().join(".specify/memory/constitution.md")).unwrap();
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "spec-kit constitution"), Level::Warn);
    assert!(detail_of(&checks, "spec-kit constitution").contains("/speckit-constitution"));
}

#[test]
fn each_installed_integration_needs_its_command_folder() {
    let bed = speckit_bed();
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "spec-kit integrations"), Level::Pass);
    assert!(
        detail_of(&checks, "spec-kit integrations").contains("claude, pi"),
        "{checks:?}"
    );

    std::fs::remove_dir_all(bed.home.path().join(".pi")).unwrap();
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "spec-kit integrations"), Level::Warn);
    let detail = detail_of(&checks, "spec-kit integrations");
    assert!(detail.contains(".pi/prompts/speckit.*.md"), "{detail}");
    assert!(!detail.contains(".claude/skills"), "{detail}");

    std::fs::remove_dir_all(bed.home.path().join(".claude/skills/speckit-plan")).unwrap();
    let detail = detail_of(&bed.checks(), "spec-kit integrations").to_owned();
    assert!(detail.contains(".claude/skills/speckit-*"), "{detail}");
}

#[test]
fn codex_needs_its_skills_folder() {
    let bed = speckit_bed();
    put(
        bed.home.path(),
        ".specify/integration.json",
        r#"{"installed_integrations": ["codex"]}"#,
    );
    assert_eq!(
        level_of(&bed.checks(), "spec-kit integrations"),
        Level::Warn
    );
    put(
        bed.home.path(),
        ".agents/skills/speckit-plan/SKILL.md",
        "plan",
    );
    assert_eq!(
        level_of(&bed.checks(), "spec-kit integrations"),
        Level::Pass
    );
}

#[test]
fn an_unreadable_integration_file_warns() {
    let bed = speckit_bed();
    std::fs::remove_file(bed.home.path().join(".specify/integration.json")).unwrap();
    assert_eq!(
        level_of(&bed.checks(), "spec-kit integrations"),
        Level::Warn
    );
    put(bed.home.path(), ".specify/integration.json", "{not json");
    assert_eq!(
        level_of(&bed.checks(), "spec-kit integrations"),
        Level::Warn
    );
}

#[test]
fn the_workflows_line_lists_the_ids_and_is_always_ok() {
    let bed = speckit_bed();
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "spec-kit workflows"), Level::Pass);
    let detail = detail_of(&checks, "spec-kit workflows");
    assert!(detail.contains("speckit"), "{detail}");
    assert!(detail.contains("teamwire starts none"), "{detail}");

    std::fs::remove_file(
        bed.home
            .path()
            .join(".specify/workflows/workflow-registry.json"),
    )
    .unwrap();
    assert_eq!(level_of(&bed.checks(), "spec-kit workflows"), Level::Pass);
    put(
        bed.home.path(),
        ".specify/workflows/workflow-registry.json",
        "[",
    );
    assert_eq!(level_of(&bed.checks(), "spec-kit workflows"), Level::Pass);
}

fn git_extension(bed: &Bed, extensions_yml: &str) -> (Level, String) {
    put(bed.home.path(), ".specify/extensions.yml", extensions_yml);
    let checks = bed.checks();
    let name = "spec-kit git extension";
    (level_of(&checks, name), detail_of(&checks, name).to_owned())
}

#[test]
fn the_git_extension_listed_under_installed_warns() {
    let bed = speckit_bed();
    assert_eq!(
        level_of(&bed.checks(), "spec-kit git extension"),
        Level::Pass
    );

    // The inline list `specify extension add` writes, and a block list.
    let (level, detail) = git_extension(&bed, "installed: [tw-demo, git]\nhooks: {}\n");
    assert_eq!(level, Level::Warn);
    assert!(detail.contains(".specify/extensions.yml"), "{detail}");
    assert!(detail.contains("specify extension remove git"), "{detail}");
    let block = "installed:\n- bug\n- 'git'\nsettings: {auto_execute_hooks: true}\n";
    assert_eq!(git_extension(&bed, block).0, Level::Warn);
    let indented = "installed:\n  - \"git\"  # branches\n";
    assert_eq!(git_extension(&bed, indented).0, Level::Warn);
}

#[test]
fn other_extensions_and_git_outside_the_installed_list_pass() {
    let bed = speckit_bed();
    let yml = "installed: [gitlab, bug]\nhooks:\n  after_tasks:\n  - {extension: git, command: speckit.git.commit}\n";
    assert_eq!(git_extension(&bed, yml).0, Level::Pass);
    let yml = "installed:\n- bug\nhooks:\n- git\n";
    assert_eq!(git_extension(&bed, yml).0, Level::Pass);
    assert_eq!(git_extension(&bed, "installed: []\n").0, Level::Pass);
}

fn events(bed: &Bed) -> (Level, String) {
    let checks = bed.checks();
    let name = "spec-kit events";
    (level_of(&checks, name), detail_of(&checks, name).to_owned())
}

#[test]
fn an_extension_that_declares_events_warns_and_one_that_does_not_passes() {
    let bed = speckit_bed();
    let root = bed.home.path();
    // A nested `events:` key is not the top-level one spec-kit reads.
    put(
        root,
        ".specify/extensions/bug/extension.yml",
        "extension:\n  id: bug\n  events: none\nprovides:\n  commands: []\n",
    );
    assert_eq!(events(&bed).0, Level::Pass);

    put(
        root,
        ".specify/extensions/tw-ev/extension.yml",
        "extension:\n  id: tw-ev\nevents:\n  pre_tool_use:\n    command: speckit.tw-ev.guard\n",
    );
    let (level, detail) = events(&bed);
    assert_eq!(level, Level::Warn);
    assert!(
        detail.contains(".specify/extensions/tw-ev/extension.yml"),
        "{detail}"
    );
    assert!(!detail.contains("extensions/bug"), "{detail}");
}

#[test]
fn an_integration_events_file_or_a_codex_config_warns() {
    let bed = speckit_bed();
    put(bed.home.path(), ".specify/integration-events.yml", "{}\n");
    let (level, detail) = events(&bed);
    assert_eq!(level, Level::Warn);
    assert!(
        detail.contains(".specify/integration-events.yml"),
        "{detail}"
    );

    let bed = speckit_bed();
    put(
        bed.home.path(),
        ".codex/config.toml",
        "[[hooks.PreToolUse]]\n",
    );
    let (level, detail) = events(&bed);
    assert_eq!(level, Level::Warn);
    assert!(detail.contains(".codex/config.toml"), "{detail}");
    assert!(detail.contains("Codex member"), "{detail}");
}

#[test]
fn a_claude_settings_hook_tagged_as_a_spec_kit_event_warns() {
    let bed = speckit_bed();
    let settings = |marker: bool| {
        serde_json::json!({"hooks": {"PreToolUse": [{"hooks": [{
            "type": "command",
            "command": "python3 \"${CLAUDE_PROJECT_DIR}/.specify/events.py\" speckit.tw-ev.guard pre_tool_use 60",
            "__speckit_event__": marker
        }]}]}})
        .to_string()
    };
    put(bed.home.path(), ".claude/settings.json", &settings(false));
    assert_eq!(events(&bed).0, Level::Pass);
    put(bed.home.path(), ".claude/settings.json", &settings(true));
    let (level, detail) = events(&bed);
    assert_eq!(level, Level::Warn);
    assert!(detail.contains(".claude/settings.json"), "{detail}");
}

#[test]
fn a_healthy_spec_kit_repository_prints_seven_ok_lines_in_order() {
    let checks = speckit_bed().checks();
    let lines: Vec<(&str, Level)> = checks
        .iter()
        .filter(|c| c.name.starts_with("spec-kit"))
        .map(|c| (c.name.as_str(), c.level))
        .collect();
    let want: Vec<(&str, Level)> = [
        "spec-kit committed",
        "spec-kit constitution",
        "spec-kit integrations",
        "spec-kit git extension",
        "spec-kit events",
        "spec-kit feature pointer",
        "spec-kit workflows",
    ]
    .into_iter()
    .map(|n| (n, Level::Pass))
    .collect();
    assert_eq!(lines, want);
    // Right after the `.worktrees` check.
    let at = names(&checks)
        .iter()
        .position(|n| *n == ".worktrees ignored")
        .unwrap();
    assert_eq!(checks[at + 1].name, "spec-kit committed");
}

/// `git` with a fixed author, so a commit works on any machine.
fn git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn line_of<'a>(out: &'a str, name: &str) -> &'a str {
    out.lines()
        .find(|l| {
            l.get(5..)
                .is_some_and(|rest| rest.starts_with(&format!("{name}:")))
        })
        .unwrap_or_else(|| unreachable!("no line {name}: {out}"))
}

#[test]
fn the_binary_asks_a_real_git_from_a_subdirectory_of_the_repository() {
    let bed = Bed::healthy();
    let bins = FakeBins::ensure().unwrap();
    let fake_dir = bed.bin.path().join("fakes");
    bins.install(&fake_dir).unwrap();
    let path = format!("{}:/usr/bin:/bin", fake_dir.display());
    let repo = ignoring_repo();
    let sub = repo.path().join("src");
    std::fs::create_dir_all(&sub).unwrap();
    put(repo.path(), ".specify/memory/constitution.md", FILLED);

    // spec-kit's own `.specify/.gitignore` is not there yet, and nothing is committed.
    let out = teamwire(&bed, &path, &sub).out;
    assert!(
        line_of(&out, "spec-kit committed").starts_with("warn"),
        "{out}"
    );
    assert!(
        line_of(&out, "spec-kit feature pointer").starts_with("warn"),
        "{out}"
    );
    assert!(!out.contains("FAIL spec-kit"), "{out}");

    put(repo.path(), ".specify/.gitignore", "feature.json\n");
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-q", "-m", "spec-kit"]);
    let out = teamwire(&bed, &path, &sub).out;
    assert!(
        line_of(&out, "spec-kit committed").starts_with("ok"),
        "{out}"
    );
    assert!(
        line_of(&out, "spec-kit feature pointer").starts_with("ok"),
        "{out}"
    );
}
