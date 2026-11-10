//! The spec-kit checks of `teamwire doctor`: printed only inside a git
//! repository whose root holds `.specify/`, each `ok` or `warn`, never a
//! failure. The in-process tests ask a real git about a real scratch
//! repository, except where a fake git must time out; the binary tests run
//! `teamwire doctor` itself.

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

/// spec-kit's constitution template, untouched.
const TEMPLATE: &str =
    "# [PROJECT_NAME] Constitution\n\n### [PRINCIPLE_1_NAME]\n[PRINCIPLE_1_DESCRIPTION]\n";

fn exits(code: i32) -> RunOutput {
    RunOutput {
        exit_code: code,
        ..answer("")
    }
}

/// A healthy bed whose repository is a spec-kit project as `specify init`
/// leaves it for Claude Code and pi, all committed: a filled constitution,
/// both command folders and the bundled workflow. spec-kit's own
/// `.specify/.gitignore` makes git ignore the feature pointer.
fn speckit_bed() -> Bed {
    let bed = Bed::healthy();
    let root = bed.home.path();
    git(root, &["init", "-q"]);
    put(root, ".specify/.gitignore", "feature.json\n");
    put(root, ".specify/memory/constitution.md", FILLED);
    put(root, ".specify/integration.json", INTEGRATIONS);
    put(root, ".specify/workflows/workflow-registry.json", WORKFLOWS);
    put(root, ".claude/skills/speckit-plan/SKILL.md", "plan");
    put(root, ".pi/prompts/speckit.plan.md", "plan");
    commit(root);
    bed
}

/// Commits every change in the repository.
fn commit(root: &Path) {
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "--allow-empty", "-m", "change"]);
}

/// Writes `file` and commits it.
fn put_committed(root: &Path, file: &str, text: &str) {
    put(root, file, text);
    commit(root);
}

/// Removes `path` and commits that.
fn remove_committed(root: &Path, path: &str) {
    git(root, &["rm", "-r", "-q", path]);
    commit(root);
}

/// Makes `text` the constitution in `HEAD` and leaves the copy on disk as
/// it was.
fn commit_constitution(bed: &mut Bed, text: &str) {
    let root = bed.home.path();
    let disk = std::fs::read_to_string(root.join(CONSTITUTION_FILE)).unwrap();
    put_committed(root, CONSTITUTION_FILE, text);
    put(root, CONSTITUTION_FILE, &disk);
}

const CONSTITUTION_FILE: &str = ".specify/memory/constitution.md";

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
fn a_git_worktree_whose_git_is_a_file_is_a_spec_kit_root() {
    let bed = speckit_bed();
    let root = bed.home.path();
    // What `git init --separate-git-dir` leaves: `.git` is a file naming the repository.
    let elsewhere = TestHome::new().unwrap();
    let moved = elsewhere.path().join("repo.git");
    std::fs::rename(root.join(".git"), &moved).unwrap();
    put(root, ".git", &format!("gitdir: {}\n", moved.display()));
    let checks = bed.checks();
    assert_eq!(speckit_names(&checks).len(), 8);
    assert_eq!(level_of(&checks, "spec-kit constitution"), Level::Pass);
}

/// A fake git for the spec-kit checks, with `rev-parse` and `ls-tree`
/// answered: the commit is `SHA` and holds `files`.
fn fake_git(bed: &mut Bed, files: &[&str]) -> Arc<FakePlatform> {
    let fake = Arc::new(FakePlatform::new());
    fake.queue_run(Ok(answer(&format!("{SHA}\n"))));
    let listed: String = files.iter().flat_map(|f| [f, "\0"]).collect();
    fake.queue_run(Ok(answer(&listed)));
    // `spec-kit committed`: `diff` and both `ls-files` find nothing on disk.
    for _ in 0..3 {
        fake.queue_run(Ok(answer("")));
    }
    bed.git = Some(fake.clone());
    fake
}

const SHA: &str = "b529e55fe8460706b2943536ecf8d8c17962daf3";

#[test]
fn git_is_asked_about_head_at_the_root() {
    let mut bed = speckit_bed();
    let fake = fake_git(&mut bed, &[CONSTITUTION_FILE, ".specify/integration.json"]);
    // `show` for the constitution, then for `integration.json`; `check-ignore` ignores.
    fake.queue_run(Ok(answer(FILLED)));
    fake.queue_run(Ok(answer(INTEGRATIONS)));
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "spec-kit committed"), Level::Pass);
    assert!(
        detail_of(&checks, "spec-kit committed")
            .contains("every spec-kit file on disk is in HEAD (b529e55)")
    );
    assert_eq!(level_of(&checks, "spec-kit feature pointer"), Level::Pass);
    let git: Vec<(Vec<String>, Option<PathBuf>)> = fake
        .runs()
        .iter()
        .filter(|(c, _)| c.program == "git")
        .map(|(c, _)| (c.args.clone(), c.cwd.clone()))
        .collect();
    let root = Some(bed.home.path().to_path_buf());
    let asked = |args: &[&str]| {
        let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        assert!(git.contains(&(args, root.clone())), "{git:?}");
    };
    asked(&["rev-parse", "--verify", "--end-of-options", "HEAD^{commit}"]);
    asked(&[
        "ls-tree",
        "-r",
        "-z",
        "--name-only",
        SHA,
        "--",
        ".specify",
        ".claude",
        ".agents",
        ".pi",
        ".codex",
    ]);
    asked(&["show", &format!("{SHA}:.specify/memory/constitution.md")]);
    asked(&["check-ignore", "-q", ".specify/feature.json"]);
}

#[test]
fn a_constitution_not_in_head_and_a_pointer_git_does_not_ignore_warn_and_name_the_file() {
    let bed = speckit_bed();
    let root = bed.home.path();
    git(
        root,
        &[
            "rm",
            "-q",
            "--cached",
            CONSTITUTION_FILE,
            ".specify/.gitignore",
        ],
    );
    std::fs::remove_file(root.join(".specify/.gitignore")).unwrap();
    git(root, &["commit", "-q", "-m", "drop"]);
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "spec-kit committed"), Level::Warn);
    let detail = detail_of(&checks, "spec-kit committed");
    assert!(
        detail.contains(".specify/memory/constitution.md"),
        "{detail}"
    );
    assert!(detail.contains("not in HEAD ("), "{detail}");
    assert!(detail.contains("constitution.md (untracked)"), "{detail}");
    assert!(detail.contains("`git add .specify`"), "{detail}");
    assert_eq!(level_of(&checks, "spec-kit constitution"), Level::Warn);
    let detail = detail_of(&checks, "spec-kit constitution");
    assert!(detail.contains("/speckit-constitution"), "{detail}");
    assert!(detail.contains("commit it"), "{detail}");
    assert_eq!(level_of(&checks, "spec-kit feature pointer"), Level::Warn);
    assert!(
        detail_of(&checks, "spec-kit feature pointer").contains(".specify/feature.json"),
        "{checks:?}"
    );
}

#[test]
fn a_git_that_cannot_answer_warns_and_names_the_file_and_git_status() {
    let timed_out = RunOutput {
        timed_out: true,
        ..answer("")
    };
    // No answer to `rev-parse`: one line, and no other spec-kit line.
    let mut bed = speckit_bed();
    let fake = Arc::new(FakePlatform::new());
    fake.queue_run(Ok(timed_out.clone()));
    bed.git = Some(fake);
    let checks = bed.checks();
    assert_eq!(speckit_names(&checks), vec!["spec-kit base"]);
    let detail = detail_of(&checks, "spec-kit base");
    assert!(detail.contains("no answer within 10 s"), "{detail}");

    // No answer to `show`, and `check-ignore` exits 128.
    let fake = fake_git(&mut bed, &[CONSTITUTION_FILE]);
    fake.queue_run(Ok(timed_out));
    fake.queue_run(Ok(exits(128)));
    let after = bed.checks();
    for (checks, name, file) in [
        (&checks, "spec-kit base", "`HEAD`"),
        (
            &after,
            "spec-kit constitution",
            ".specify/memory/constitution.md",
        ),
        (&after, "spec-kit feature pointer", ".specify/feature.json"),
    ] {
        assert_eq!(level_of(checks, name), Level::Warn);
        let detail = detail_of(checks, name);
        assert!(detail.contains(file), "{name}: {detail}");
        assert!(detail.contains("run `git status` in"), "{name}: {detail}");
    }
}

#[test]
fn a_constitution_with_a_template_placeholder_left_warns_and_names_it() {
    let mut bed = speckit_bed();
    assert_eq!(
        level_of(&bed.checks(), "spec-kit constitution"),
        Level::Pass
    );

    put(bed.home.path(), ".specify/memory/constitution.md", TEMPLATE);
    commit_constitution(&mut bed, TEMPLATE);
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "spec-kit constitution"), Level::Warn);
    let detail = detail_of(&checks, "spec-kit constitution");
    assert!(detail.contains("[PROJECT_NAME]"), "{detail}");
    assert!(
        detail.contains(".specify/memory/constitution.md"),
        "{detail}"
    );
    assert!(!detail.contains("differs"), "{detail}");
}

#[test]
fn a_constitution_filled_only_on_disk_warns_because_members_get_the_committed_template() {
    let mut bed = speckit_bed();
    commit_constitution(&mut bed, TEMPLATE);
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "spec-kit constitution"), Level::Warn);
    let detail = detail_of(&checks, "spec-kit constitution");
    assert!(detail.contains("[PROJECT_NAME]"), "{detail}");
    assert!(
        detail.contains("the copy on disk differs from HEAD"),
        "{detail}"
    );
}

#[test]
fn a_filled_constitution_in_head_passes_and_says_when_the_disk_copy_differs() {
    let bed = speckit_bed();
    let detail = detail_of(&bed.checks(), "spec-kit constitution").to_owned();
    assert!(!detail.contains("differs"), "{detail}");

    put(
        bed.home.path(),
        ".specify/memory/constitution.md",
        &format!("{FILLED}\n### II. Tests first\n"),
    );
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "spec-kit constitution"), Level::Pass);
    let detail = detail_of(&checks, "spec-kit constitution");
    assert!(
        detail.contains("the copy on disk differs from HEAD"),
        "{detail}"
    );
}

#[test]
fn brackets_that_are_not_template_placeholders_pass() {
    let mut bed = speckit_bed();
    commit_constitution(
        &mut bed,
        "# wordtool Constitution\n- [x] done\n- [ ] open\nT001 [P] [US1] task\n\
         [NEEDS CLARIFICATION] and a [link](https://example.com) and [V1]\n",
    );
    assert_eq!(
        level_of(&bed.checks(), "spec-kit constitution"),
        Level::Pass
    );
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

    remove_committed(bed.home.path(), ".pi");
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "spec-kit integrations"), Level::Warn);
    let detail = detail_of(&checks, "spec-kit integrations");
    assert!(detail.contains(".pi/prompts/speckit.*.md"), "{detail}");
    assert!(!detail.contains(".claude/skills"), "{detail}");

    remove_committed(bed.home.path(), ".claude/skills/speckit-plan");
    let detail = detail_of(&bed.checks(), "spec-kit integrations").to_owned();
    assert!(detail.contains(".claude/skills/speckit-*"), "{detail}");
}

#[test]
fn codex_needs_its_skills_folder() {
    let bed = speckit_bed();
    put_committed(
        bed.home.path(),
        ".specify/integration.json",
        r#"{"installed_integrations": ["codex"]}"#,
    );
    assert_eq!(
        level_of(&bed.checks(), "spec-kit integrations"),
        Level::Warn
    );
    put_committed(
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
    remove_committed(bed.home.path(), ".specify/integration.json");
    assert_eq!(
        level_of(&bed.checks(), "spec-kit integrations"),
        Level::Warn
    );
    put_committed(bed.home.path(), ".specify/integration.json", "{not json");
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "spec-kit integrations"), Level::Warn);
    let detail = detail_of(&checks, "spec-kit integrations");
    assert!(detail.contains(".specify/integration.json"), "{detail}");
    assert!(
        detail.contains("run `specify integration status`"),
        "{detail}"
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

    remove_committed(bed.home.path(), ".specify/workflows/workflow-registry.json");
    assert_eq!(level_of(&bed.checks(), "spec-kit workflows"), Level::Pass);
    put_committed(
        bed.home.path(),
        ".specify/workflows/workflow-registry.json",
        "[",
    );
    assert_eq!(level_of(&bed.checks(), "spec-kit workflows"), Level::Pass);
}

fn git_extension(bed: &Bed, extensions_yml: &str) -> (Level, String) {
    put_committed(bed.home.path(), ".specify/extensions.yml", extensions_yml);
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
    // `installed:` nested under another key is not the top-level list.
    let yml = "settings:\n  installed: [git]\n  other:\n  - git\n";
    assert_eq!(git_extension(&bed, yml).0, Level::Pass);
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
    put_committed(
        root,
        ".specify/extensions/bug/extension.yml",
        "extension:\n  id: bug\n  events: none\nprovides:\n  commands: []\n",
    );
    assert_eq!(events(&bed).0, Level::Pass);

    put_committed(
        root,
        ".specify/extensions/tw-ev/extension.yml",
        "extension:\n  id: tw-ev\nevents:\n  pre_tool_use:\n    command: speckit.tw-ev.guard\n",
    );
    // Declared, but spec-kit wrote no hook: nothing runs, so a note and no warning.
    let (level, detail) = events(&bed);
    assert_eq!(level, Level::Pass);
    assert!(
        detail.contains("tw-ev declares events:, but spec-kit wrote no hook"),
        "{detail}"
    );
    assert!(!detail.contains("bug"), "{detail}");

    // Once spec-kit wrote its hook, the action names the extension.
    put_committed(root, ".claude/settings.json", EVX_CLAUDE_SETTINGS);
    let (level, detail) = events(&bed);
    assert_eq!(level, Level::Warn);
    assert!(
        detail.contains("specify extension remove tw-ev"),
        "{detail}"
    );
}

#[test]
fn an_integration_events_file_or_a_codex_config_warns() {
    let bed = speckit_bed();
    // An override file sets up no hook on its own.
    put_committed(bed.home.path(), ".specify/integration-events.yml", "{}\n");
    let (level, detail) = events(&bed);
    assert_eq!(level, Level::Pass);
    assert!(
        !detail.contains(".specify/integration-events.yml"),
        "{detail}"
    );

    let bed = speckit_bed();
    put_committed(
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
fn a_codex_hooks_file_warns_because_codex_launch_refuses_it() {
    let bed = speckit_bed();
    put_committed(bed.home.path(), ".codex/hooks.json", "{}\n");
    let (level, detail) = events(&bed);
    assert_eq!(level, Level::Warn);
    assert!(detail.contains(".codex/hooks.json"), "{detail}");
    assert!(detail.contains("Codex member"), "{detail}");
    assert!(!detail.contains(".codex/config.toml"), "{detail}");
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
    put_committed(bed.home.path(), ".claude/settings.json", &settings(false));
    assert_eq!(events(&bed).0, Level::Pass);
    put_committed(bed.home.path(), ".claude/settings.json", &settings(true));
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
        "spec-kit hooks",
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

    // No commit yet: one line says so, and no other line guesses.
    let out = teamwire(&bed, &path, &sub).out;
    let line = line_of(&out, "spec-kit base");
    assert!(line.starts_with("warn"), "{out}");
    assert!(line.contains("Needed a single revision"), "{out}");
    assert!(!out.contains("spec-kit feature pointer"), "{out}");
    assert!(!out.contains("FAIL spec-kit"), "{out}");

    // spec-kit's own `.specify/.gitignore` is not there yet.
    git(repo.path(), &["add", ".gitignore"]);
    git(repo.path(), &["commit", "-q", "-m", "first"]);
    let out = teamwire(&bed, &path, &sub).out;
    assert!(
        line_of(&out, "spec-kit committed").starts_with("warn"),
        "{out}"
    );
    assert!(
        line_of(&out, "spec-kit feature pointer").starts_with("warn"),
        "{out}"
    );

    // Staged but not committed: a worktree cut from HEAD gets none of it.
    put(repo.path(), ".specify/.gitignore", "feature.json\n");
    put(repo.path(), ".specify/memory/constitution.md", TEMPLATE);
    git(repo.path(), &["add", "."]);
    let out = teamwire(&bed, &path, &sub).out;
    assert!(
        line_of(&out, "spec-kit committed").starts_with("warn"),
        "{out}"
    );
    assert!(
        line_of(&out, "spec-kit feature pointer").starts_with("ok"),
        "{out}"
    );

    // The template committed, then filled in only on disk.
    git(repo.path(), &["commit", "-q", "-m", "spec-kit"]);
    put(repo.path(), ".specify/memory/constitution.md", FILLED);
    let out = teamwire(&bed, &path, &sub).out;
    let line = line_of(&out, "spec-kit committed");
    assert!(line.starts_with("warn"), "{out}");
    assert!(line.contains("constitution.md (changed)"), "{out}");
    let line = line_of(&out, "spec-kit constitution");
    assert!(line.starts_with("warn"), "{out}");
    assert!(line.contains("[PROJECT_NAME]"), "{out}");
    assert!(line.contains("the copy on disk differs from HEAD"), "{out}");

    git(repo.path(), &["commit", "-q", "-am", "constitution"]);
    let out = teamwire(&bed, &path, &sub).out;
    assert!(
        line_of(&out, "spec-kit constitution").starts_with("ok"),
        "{out}"
    );
}

/// The start of the `.specify/extensions.yml` that `specify extension add
/// git` (spec-kit 1.3.3.dev0) writes: its two hooks with `optional: false`
/// and one optional hook. The real file goes on with 15 more optional hooks.
const GIT_EXTENSIONS: &str = "installed:
- git
settings:
  auto_execute_hooks: true
hooks:
  before_constitution:
  - extension: git
    command: speckit.git.initialize
    enabled: true
    optional: false
    priority: 10
    prompt: Execute speckit.git.initialize?
    description: Initialize Git repository before constitution setup
    condition: null
  before_specify:
  - extension: git
    command: speckit.git.feature
    enabled: true
    optional: false
    priority: 10
    prompt: Execute speckit.git.feature?
    description: Create feature branch before specification
    condition: null
  before_clarify:
  - extension: git
    command: speckit.git.commit
    enabled: true
    optional: true
    priority: 10
    prompt: Commit outstanding changes before clarification?
    description: Auto-commit before spec clarification
    condition: null
";

/// What `specify extension remove <id>` leaves in `.specify/extensions.yml`
/// when it removed the last extension.
const NO_EXTENSIONS: &str = "installed: []\nsettings:\n  auto_execute_hooks: true\nhooks: {}\n";

/// The manifest of a scratch extension `evx` that declares one event.
const EVX_MANIFEST: &str = "schema_version: \"1.0\"
extension:
  id: evx
  name: \"Events demo\"
  version: \"1.0.0\"
provides:
  commands:
    - name: speckit.evx.guard
      file: commands/speckit.evx.guard.md
events:
  session_start:
    command: speckit.evx.guard
";

/// The `.claude/settings.json` that `specify extension add --dev evx` wrote.
const EVX_CLAUDE_SETTINGS: &str = r#"{
  "hooks": {
    "SessionStart": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "python3 \"${CLAUDE_PROJECT_DIR}/.specify/events.py\" speckit.evx.guard session_start 60",
            "timeout": 65,
            "__speckit_event__": true
          }
        ]
      }
    ]
  }
}
"#;

/// The `.codex/config.toml` that `specify extension add --dev evx` wrote.
const EVX_CODEX_CONFIG: &str = "

[[hooks.SessionStart]]
matcher = \"*\"

[[hooks.SessionStart.hooks]]
type = \"command\"
command = \"python3 .specify/events.py speckit.evx.guard session_start 60\"
timeout = 65
speckit_marker = true
";

/// Every file `specify extension add --dev evx` wrote for Claude Code and Codex.
fn put_evx(root: &Path) {
    put(root, ".specify/extensions/evx/extension.yml", EVX_MANIFEST);
    put(root, ".claude/settings.json", EVX_CLAUDE_SETTINGS);
    put(root, ".codex/config.toml", EVX_CODEX_CONFIG);
    put(root, ".specify/events.py", "#!/usr/bin/env python3\n");
}

#[test]
fn every_line_reads_the_commit_and_a_change_only_on_disk_does_not_clear_a_warning() {
    let bed = speckit_bed();
    let root = bed.home.path();
    put(root, ".specify/extensions.yml", GIT_EXTENSIONS);
    put_evx(root);
    commit(root);
    // pi's prompts on disk, but not in the commit.
    git(root, &["rm", "-r", "-q", "--cached", ".pi"]);
    git(root, &["commit", "-q", "-m", "drop pi"]);

    // What `specify extension remove git` and `specify extension remove evx`
    // change, not committed.
    put(root, ".specify/extensions.yml", NO_EXTENSIONS);
    for file in [
        ".specify/extensions/evx/extension.yml",
        ".claude/settings.json",
        ".codex/config.toml",
        ".specify/events.py",
    ] {
        std::fs::remove_file(root.join(file)).unwrap();
    }
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "spec-kit git extension"), Level::Warn);
    assert_eq!(level_of(&checks, "spec-kit events"), Level::Warn);
    let detail = detail_of(&checks, "spec-kit events");
    assert!(detail.contains(".claude/settings.json"), "{detail}");
    assert!(detail.contains(".codex/config.toml"), "{detail}");
    assert_eq!(level_of(&checks, "spec-kit integrations"), Level::Warn);
    let detail = detail_of(&checks, "spec-kit integrations");
    assert!(
        detail.contains(".pi/prompts/speckit.*.md in HEAD ("),
        "{detail}"
    );

    // Committed, each line turns ok.
    commit(root);
    let checks = bed.checks();
    for name in [
        "spec-kit git extension",
        "spec-kit events",
        "spec-kit integrations",
    ] {
        assert_eq!(level_of(&checks, name), Level::Pass, "{name}: {checks:?}");
    }
}

#[test]
fn base_names_the_commit_the_lines_read_and_every_line_names_it() {
    let bed = Bed::healthy();
    let bins = FakeBins::ensure().unwrap();
    let fake_dir = bed.bin.path().join("fakes");
    bins.install(&fake_dir).unwrap();
    let path = format!("{}:/usr/bin:/bin", fake_dir.display());
    let repo = ignoring_repo();
    let root = repo.path();
    git(root, &["checkout", "-q", "-b", "main"]);
    git(root, &["add", ".gitignore"]);
    git(root, &["commit", "-q", "-m", "first"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
    put(root, ".specify/.gitignore", "feature.json\n");
    put(root, ".specify/memory/constitution.md", FILLED);
    put(root, ".specify/integration.json", INTEGRATIONS);
    put(root, ".claude/skills/speckit-plan/SKILL.md", "plan");
    put(root, ".pi/prompts/speckit.plan.md", "plan");
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "spec-kit"]);
    let short = |name: &str| {
        let out = std::process::Command::new("git")
            .args(["rev-parse", "--short=7", name])
            .current_dir(root)
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    };

    // The default is HEAD, here develop, which holds spec-kit.
    let out = teamwire(&bed, &path, root).out;
    let line = line_of(&out, "spec-kit constitution");
    assert!(line.starts_with("ok"), "{out}");
    assert!(
        line.contains(&format!("in HEAD ({})", short("develop"))),
        "{out}"
    );

    // Members cut from main get no spec-kit.
    let out = teamwire_with(&bed, &path, root, &["--base", "main"]).out;
    let at = format!("main ({})", short("main"));
    for name in [
        "spec-kit committed",
        "spec-kit constitution",
        "spec-kit integrations",
        "spec-kit git extension",
        "spec-kit events",
        "spec-kit workflows",
    ] {
        let line = line_of(&out, name);
        assert!(line.contains(&at), "{name}: {out}");
    }
    assert!(
        line_of(&out, "spec-kit constitution").starts_with("warn"),
        "{out}"
    );
    assert!(
        line_of(&out, "spec-kit committed").starts_with("warn"),
        "{out}"
    );

    // A ref git cannot resolve: one line, and it names the ref.
    let out = teamwire_with(&bed, &path, root, &["--base", "mian"]).out;
    let line = line_of(&out, "spec-kit base");
    assert!(line.starts_with("warn"), "{out}");
    assert!(line.contains("`mian`"), "{out}");
    assert!(line.contains("pass `--base` a branch or commit"), "{out}");
    assert!(!out.contains("spec-kit committed"), "{out}");
}

#[test]
fn doctor_help_shows_base() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .args(["doctor", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8(out.stdout).unwrap();
    assert!(help.contains("--base <REF>"), "{help}");
    assert!(help.contains("[default: HEAD]"), "{help}");
}

/// Runs the `git add ...` that a `spec-kit committed` warning names, as the
/// lead would, then commits.
fn follow_git_add(root: &Path, detail: &str) {
    let add = detail
        .split("run `git add ")
        .nth(1)
        .and_then(|rest| rest.split('`').next())
        .unwrap_or_else(|| unreachable!("no git add in {detail}"));
    let mut args = vec!["add"];
    args.extend(add.split(' '));
    git(root, &args);
    git(root, &["commit", "-q", "-m", "follow doctor"]);
}

#[test]
fn a_file_on_disk_but_not_in_the_commit_is_named_untracked_changed_new_or_git_ignored() {
    let bed = speckit_bed();
    let root = bed.home.path();
    put_committed(
        root,
        ".specify/.gitignore",
        "feature.json\nextensions/*/local-config.yml\n",
    );
    put(
        root,
        ".specify/integration.json",
        &INTEGRATIONS.replace("pi", "codex"),
    );
    put(root, ".agents/skills/speckit-plan/SKILL.md", "plan");
    put(root, ".agents/skills/speckit-tasks/SKILL.md", "tasks");
    put(root, ".specify/scripts/bash/common.sh", "#!/bin/sh\n");
    git(root, &["add", ".specify/scripts"]);
    put(root, ".gitignore", ".pi/prompts/speckit.tasks.md\n");
    put(root, ".pi/prompts/speckit.tasks.md", "tasks");
    // Not spec-kit's to commit: the pointer, local config spec-kit ignores, a user's own skill.
    put(root, ".specify/feature.json", "{}");
    put(root, ".specify/extensions/bug/local-config.yml", "a: 1\n");
    put(root, ".claude/skills/mine/SKILL.md", "mine");

    let checks = bed.checks();
    assert_eq!(level_of(&checks, "spec-kit committed"), Level::Warn);
    let detail = detail_of(&checks, "spec-kit committed");
    assert!(detail.contains("not in HEAD ("), "{detail}");
    assert!(
        detail.contains(".specify/integration.json (changed)"),
        "{detail}"
    );
    assert!(
        detail.contains(".specify/scripts/bash/common.sh (new)"),
        "{detail}"
    );
    assert!(
        detail.contains(".agents/skills/speckit-* (untracked, 2 files)"),
        "{detail}"
    );
    assert!(
        detail.contains(
            ".pi/prompts/speckit.*.md (git-ignored by `.pi/prompts/speckit.tasks.md` in .gitignore:1)"
        ),
        "{detail}"
    );
    assert!(
        detail.contains("remove the rule `.pi/prompts/speckit.tasks.md` from .gitignore (line 1)"),
        "{detail}"
    );
    for skipped in ["feature.json", "local-config.yml", "mine"] {
        assert!(!detail.contains(skipped), "{skipped}: {detail}");
    }
    assert!(detail.ends_with("then commit on HEAD"), "{detail}");
}

#[test]
fn codex_installed_but_not_committed_warns_until_the_named_git_add_is_committed() {
    let bed = speckit_bed();
    let root = bed.home.path();
    // What `specify integration install codex` changes on a committed claude setup.
    put(
        root,
        ".specify/integration.json",
        &INTEGRATIONS.replace("\"pi\"", "\"codex\""),
    );
    put(root, ".specify/integrations/codex.manifest.json", "{}\n");
    for cmd in ["plan", "tasks", "implement"] {
        put(root, &format!(".agents/skills/speckit-{cmd}/SKILL.md"), cmd);
    }
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "spec-kit committed"), Level::Warn);
    let detail = detail_of(&checks, "spec-kit committed").to_owned();
    assert!(
        detail.contains(".agents/skills/speckit-* (untracked, 3 files)"),
        "{detail}"
    );
    assert!(
        detail.contains(".specify/integrations/codex.manifest.json (untracked)"),
        "{detail}"
    );
    assert!(
        detail.contains("run `git add .agents/skills/speckit-* .specify`"),
        "{detail}"
    );

    follow_git_add(root, &detail);
    let checks = bed.checks();
    assert_eq!(
        level_of(&checks, "spec-kit committed"),
        Level::Pass,
        "{checks:?}"
    );
    assert_eq!(
        level_of(&checks, "spec-kit integrations"),
        Level::Pass,
        "{checks:?}"
    );
}

#[test]
fn a_command_folder_ignored_as_spec_kit_advises_warns_until_the_rule_goes_and_it_is_committed() {
    let bed = speckit_bed();
    let root = bed.home.path();
    // `specify init` advises adding `.claude/` to `.gitignore`.
    put(root, ".gitignore", ".claude/\n");
    git(root, &["rm", "-r", "-q", "--cached", ".claude"]);
    git(root, &["add", ".gitignore"]);
    git(
        root,
        &["commit", "-q", "-m", "ignore .claude as spec-kit advises"],
    );

    let checks = bed.checks();
    assert_eq!(level_of(&checks, "spec-kit committed"), Level::Warn);
    let detail = detail_of(&checks, "spec-kit committed").to_owned();
    assert!(
        detail.contains(".claude/skills/speckit-* (git-ignored by `.claude/` in .gitignore:1)"),
        "{detail}"
    );
    assert!(
        detail.contains("remove the rule `.claude/` from .gitignore (line 1), run `git add .claude/skills/speckit-*`"),
        "{detail}"
    );
    assert_eq!(level_of(&checks, "spec-kit integrations"), Level::Warn);

    put(root, ".gitignore", "");
    follow_git_add(root, &detail);
    let checks = bed.checks();
    assert_eq!(
        level_of(&checks, "spec-kit committed"),
        Level::Pass,
        "{checks:?}"
    );
    assert_eq!(
        level_of(&checks, "spec-kit integrations"),
        Level::Pass,
        "{checks:?}"
    );
}

#[test]
fn an_older_integration_file_still_names_its_integration() {
    let bed = speckit_bed();
    let root = bed.home.path();
    remove_committed(root, ".claude");
    // The form spec-kit before `installed_integrations` wrote.
    put_committed(
        root,
        ".specify/integration.json",
        r#"{"integration": "claude", "version": "0.8.0"}"#,
    );
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "spec-kit integrations"), Level::Warn);
    let detail = detail_of(&checks, "spec-kit integrations");
    assert!(detail.contains("lists claude"), "{detail}");
    assert!(detail.contains(".claude/skills/speckit-*"), "{detail}");

    // `default_integration` counts too, ahead of the list.
    put_committed(
        root,
        ".specify/integration.json",
        r#"{"installed_integrations": ["pi"], "default_integration": "claude"}"#,
    );
    let detail = detail_of(&bed.checks(), "spec-kit integrations").to_owned();
    assert!(detail.contains("lists claude, pi"), "{detail}");
}

#[test]
fn a_skills_folder_with_only_the_users_own_skills_has_no_spec_kit_commands() {
    let bed = speckit_bed();
    let root = bed.home.path();
    remove_committed(root, ".claude/skills/speckit-plan");
    put_committed(root, ".claude/skills/mine/SKILL.md", "mine");
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "spec-kit integrations"), Level::Warn);
    assert!(
        detail_of(&checks, "spec-kit integrations").contains(".claude/skills/speckit-*"),
        "{checks:?}"
    );
}

#[test]
fn every_form_spec_kit_reads_as_listing_git_warns_and_an_unreadable_file_never_passes() {
    let bed = speckit_bed();
    for form in [
        "installed: [git] # note\n",
        "\"installed\":\n- git\n",
        "\u{feff}installed:\n- git\n",
        "installed: [bug,\n  git]\n",
        "installed:\r\n- git\r\n",
        "installed:\n- id: git\n",
    ] {
        let (level, detail) = git_extension(&bed, form);
        assert_eq!(level, Level::Warn, "{form:?}: {detail}");
        assert!(detail.contains("specify extension remove git"), "{detail}");
    }
    let (level, detail) = git_extension(&bed, "installed: &ids [git]\n");
    assert_eq!(level, Level::Warn, "{detail}");
    assert!(detail.contains("check the file by hand"), "{detail}");
    assert!(
        detail.contains(".specify/extensions.yml in HEAD ("),
        "{detail}"
    );
}

/// The `.specify/extensions.yml` that `specify extension add --dev gate`
/// (spec-kit 1.3.3.dev0) wrote for a scratch extension `gate` with one
/// required and one optional hook.
const GATE_EXTENSIONS: &str = "installed:
- gate
settings:
  auto_execute_hooks: true
hooks:
  before_tasks:
  - extension: gate
    command: speckit.gate.check
    enabled: true
    optional: false
    priority: 10
    prompt: Execute speckit.gate.check?
    description: Require a merge request
    condition: null
  after_tasks:
  - extension: gate
    command: speckit.gate.check
    enabled: true
    optional: true
    priority: 10
    prompt: Execute speckit.gate.check?
    description: ''
    condition: null
";

fn hooks(bed: &Bed) -> (Level, String) {
    let checks = bed.checks();
    let name = "spec-kit hooks";
    (level_of(&checks, name), detail_of(&checks, name).to_owned())
}

#[test]
fn a_required_hook_of_another_extension_warns_until_it_is_removed_disabled_or_optional() {
    let bed = speckit_bed();
    let root = bed.home.path();
    assert_eq!(hooks(&bed).0, Level::Pass);
    // The git extension's required hooks are its own line's.
    put_committed(root, ".specify/extensions.yml", GIT_EXTENSIONS);
    assert_eq!(hooks(&bed).0, Level::Pass);

    put_committed(root, ".specify/extensions.yml", GATE_EXTENSIONS);
    let (level, detail) = hooks(&bed);
    assert_eq!(level, Level::Warn);
    assert!(
        detail.contains("gate runs speckit.gate.check at before_tasks"),
        "{detail}"
    );
    assert!(!detail.contains("after_tasks"), "{detail}");
    assert!(detail.contains("specify extension remove gate"), "{detail}");
    assert!(detail.ends_with("then commit on HEAD"), "{detail}");

    // Not committed yet: members still get the hook.
    put(root, ".specify/extensions.yml", NO_EXTENSIONS);
    assert_eq!(hooks(&bed).0, Level::Warn);
    // What `specify extension remove gate` leaves, committed.
    commit(root);
    assert_eq!(hooks(&bed).0, Level::Pass);

    // Made optional by hand, or disabled (`specify extension disable` sets `enabled: false`).
    for fixed in [
        GATE_EXTENSIONS.replace("optional: false", "optional: true"),
        GATE_EXTENSIONS.replace("enabled: true", "enabled: false"),
    ] {
        put_committed(root, ".specify/extensions.yml", GATE_EXTENSIONS);
        assert_eq!(hooks(&bed).0, Level::Warn);
        put_committed(root, ".specify/extensions.yml", &fixed);
        assert_eq!(hooks(&bed).0, Level::Pass, "{fixed}");
    }

    put_committed(root, ".specify/extensions.yml", "hooks: [unclosed\n");
    let (level, detail) = hooks(&bed);
    assert_eq!(level, Level::Warn);
    assert!(detail.contains("check the file by hand"), "{detail}");
}

#[test]
fn events_warn_only_on_hooks_spec_kit_wrote_and_each_named_remedy_clears_them() {
    // The three commands that remove spec-kit's event hooks, each as the
    // files it leaves (proven with spec-kit 1.3.3.dev0 in a scratch repository).
    // `remove` also deletes the manifest; `disable` and the upgrade keep it.
    let remedies = [
        ("specify extension remove evx", false),
        ("specify extension disable evx", true),
        (
            "specify integration upgrade claude --force --integration-options=\"--events false\"",
            true,
        ),
    ];
    for (named, keeps_manifest) in remedies {
        let bed = speckit_bed();
        let root = bed.home.path();
        put(
            root,
            ".specify/integration.json",
            &INTEGRATIONS.replace("\"pi\"", "\"codex\""),
        );
        put_evx(root);
        commit(root);
        let (level, detail) = events(&bed);
        assert_eq!(level, Level::Warn);
        assert!(detail.contains(named), "{named}: {detail}");
        assert!(
            detail.contains("and `specify integration upgrade codex --force"),
            "{detail}"
        );
        assert!(detail.contains(".claude/settings.json"), "{detail}");
        assert!(detail.contains(".codex/config.toml"), "{detail}");
        assert!(detail.contains(".specify/events.py"), "{detail}");
        assert!(detail.ends_with("then commit on HEAD"), "{detail}");

        for file in [
            ".claude/settings.json",
            ".codex/config.toml",
            ".specify/events.py",
        ] {
            std::fs::remove_file(root.join(file)).unwrap();
        }
        if !keeps_manifest {
            std::fs::remove_file(root.join(".specify/extensions/evx/extension.yml")).unwrap();
        }
        commit(root);
        let (level, detail) = events(&bed);
        assert_eq!(level, Level::Pass, "{named}: {detail}");
    }
}

#[test]
fn a_codex_file_only_on_disk_warns_because_codex_launch_refuses_it_there() {
    let bed = speckit_bed();
    let root = bed.home.path();
    put(root, ".codex/hooks.json", "{}\n");
    let (level, detail) = events(&bed);
    assert_eq!(level, Level::Warn);
    assert!(detail.contains(".codex/hooks.json is on disk"), "{detail}");
    assert!(detail.contains("Codex member"), "{detail}");
    assert!(detail.contains("remove it from the repository"), "{detail}");
    std::fs::remove_file(root.join(".codex/hooks.json")).unwrap();
    assert_eq!(events(&bed).0, Level::Pass);
}
