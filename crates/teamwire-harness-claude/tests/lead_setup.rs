//! The lead setup: what install and uninstall plan for each state of the
//! teamwire plugin (ADR 0018) and of the skill and MCP entry that ADR 0012
//! wrote, the refusals that come before any write, and what a failure partway
//! through reports. `claude` is never started: the fake platform records each
//! command and answers from a queue, and a test writes the files Claude Code
//! would have written.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests build fixtures from literals; a failure should fail the test"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "the test writes Claude's files as a user or Claude Code would, not through the platform"
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Value, json};
use teamwire_core::{PlatformError, PlatformErrorCode, RunOutput};
use teamwire_fakes::{FakePlatform, TestHome};
use teamwire_harness_claude::{
    Change, ClaudeCode, ClaudeConfig, LeadSetupError, McpEntry, McpState, SKILL_TEXT, SkillState,
    lead_setup_state,
};

struct Bed {
    /// Claude's `CLAUDE_CONFIG_DIR` layout.
    dir: TestHome,
    /// The teamwire state directory.
    home: TestHome,
    platform: Arc<FakePlatform>,
    claude: ClaudeCode,
}

impl Bed {
    /// Nothing installed.
    fn new() -> Self {
        let dir = TestHome::new().unwrap();
        let platform = Arc::new(FakePlatform::new());
        let claude = ClaudeCode::new(
            Arc::clone(&platform) as Arc<dyn teamwire_core::Platform>,
            ClaudeConfig {
                claude_dir: dir.path().to_path_buf(),
                claude_json: dir.path().join(".claude.json"),
                claude_bin: "claude".to_owned(),
            },
        );
        Self {
            dir,
            home: TestHome::new().unwrap(),
            platform,
            claude,
        }
    }

    fn plugin_dir(&self) -> PathBuf {
        self.home.path().join("claude-plugin")
    }

    fn skill_path(&self) -> PathBuf {
        self.dir.path().join("skills/teamwire/SKILL.md")
    }

    fn put_skill(&self, text: &str) {
        std::fs::create_dir_all(self.skill_path().parent().unwrap()).unwrap();
        std::fs::write(self.skill_path(), text).unwrap();
    }

    fn put_entry(&self, entry: &Value) {
        let doc = json!({"numStartups": 3, "mcpServers": {"teamwire": entry}});
        std::fs::write(self.dir.path().join(".claude.json"), doc.to_string()).unwrap();
    }

    fn put_claude_file(&self, rel: &str, doc: &Value) {
        let path = self.dir.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, doc.to_string()).unwrap();
    }

    fn plan(&self, want: &McpEntry) -> Vec<Change> {
        self.claude
            .plan_install(want, self.home.path(), false)
            .unwrap()
    }

    /// Writes the plugin files for `want` where `claude plugin marketplace add`
    /// would read them.
    fn write_files(&self, want: &McpEntry) {
        let write: Vec<Change> = self
            .plan(want)
            .into_iter()
            .filter(|c| matches!(c, Change::WritePlugin { .. }))
            .collect();
        self.claude.apply_lead_setup(&write).unwrap();
    }

    /// Claude Code's registration, as `claude plugin install` leaves it: the
    /// marketplace added from this bed's directory, and the plugin loaded from
    /// `root`.
    fn register(&self, root: &Path, enabled: bool) {
        self.put_claude_file(
            "plugins/known_marketplaces.json",
            &json!({"teamwire": {
                "source": {"source": "directory", "path": self.plugin_dir()},
                "installLocation": self.plugin_dir(),
            }}),
        );
        self.put_claude_file(
            "plugins/installed_plugins.json",
            &json!({"version": 2, "plugins": {"teamwire@teamwire": [
                {"scope": "user", "installPath": root, "version": "x"}]}}),
        );
        self.put_claude_file(
            "settings.json",
            &json!({"enabledPlugins": {"teamwire@teamwire": enabled}}),
        );
    }

    /// A fully installed setup for `want`, loading the plugin from the
    /// marketplace directory itself.
    fn install(&self, want: &McpEntry) {
        self.write_files(want);
        self.register(&self.plugin_dir().join("teamwire"), true);
    }

    /// The argv of each `claude` command run, after the program name.
    fn claude_runs(&self) -> Vec<Vec<String>> {
        self.platform
            .runs()
            .into_iter()
            .map(|(cmd, _)| {
                assert_eq!(cmd.program, "claude");
                assert_eq!(
                    cmd.env,
                    [(
                        "CLAUDE_CONFIG_DIR".to_owned(),
                        self.dir.path().display().to_string()
                    )],
                    "claude must write where teamwire reads"
                );
                cmd.args
            })
            .collect()
    }
}

fn want() -> McpEntry {
    McpEntry::lead(Path::new("/opt/bin/teamwire"), None)
}

fn args(line: &str) -> Vec<String> {
    line.split_whitespace().map(str::to_owned).collect()
}

fn exited(code: i32, stderr: &str) -> RunOutput {
    RunOutput {
        exit_code: code,
        stdout: Vec::new(),
        stderr: stderr.as_bytes().to_vec(),
        timed_out: false,
    }
}

/// The text of one planned plugin file.
fn planned_file(changes: &[Change], path: &str) -> Value {
    let Some(Change::WritePlugin { files, .. }) = changes.first() else {
        unreachable!("no WritePlugin first: {changes:?}")
    };
    let (_, text) = files.iter().find(|(p, _)| p == Path::new(path)).unwrap();
    serde_json::from_str(text).unwrap()
}

#[test]
fn a_fresh_install_writes_the_files_adds_the_marketplace_and_installs_the_plugin() {
    let bed = Bed::new();
    let changes = bed.plan(&want());
    let [Change::WritePlugin { dir, files }, rest @ ..] = changes.as_slice() else {
        unreachable!("{changes:?}")
    };
    assert_eq!(dir, &bed.plugin_dir());
    assert_eq!(files.len(), 7);
    assert_eq!(
        rest,
        [
            Change::AddMarketplace(bed.plugin_dir()),
            Change::InstallPlugin
        ]
    );
    assert_eq!(
        bed.claude_runs(),
        [args("plugin --help")],
        "planning only checks that claude has a plugin command"
    );
    assert!(!bed.plugin_dir().exists(), "planning writes nothing");

    bed.claude.apply_lead_setup(&changes).unwrap();
    let skill = std::fs::read_to_string(bed.plugin_dir().join("teamwire/skills/teamwire/SKILL.md"))
        .unwrap();
    assert!(skill.contains("\nmetadata:\n  teamwire-sha256: "));
    assert!(
        std::fs::read_to_string(
            bed.plugin_dir()
                .join("teamwire/skills/teamwire/reference.md")
        )
        .unwrap()
        .contains("## Watch lines")
    );
    let afk =
        std::fs::read_to_string(bed.plugin_dir().join("teamwire/skills/afk/SKILL.md")).unwrap();
    assert!(afk.contains("\nname: afk\n"));
    assert!(afk.contains("\nmetadata:\n  teamwire-sha256: "));
    let hooks: Value = serde_json::from_str(
        &std::fs::read_to_string(bed.plugin_dir().join("teamwire/hooks/hooks.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        hooks["hooks"]["Stop"][0]["hooks"][0]["command"],
        "/opt/bin/teamwire hook claude-lead-stop"
    );
    let runs = bed.claude_runs();
    assert_eq!(
        runs[1],
        args(&format!(
            "plugin marketplace add --scope user {}",
            bed.plugin_dir().display()
        ))
    );
    assert_eq!(
        runs[2],
        args("plugin install teamwire@teamwire --scope user")
    );
}

#[test]
fn a_home_that_is_not_the_default_reaches_the_server_and_the_hook() {
    let bed = Bed::new();
    let entry = McpEntry::lead(Path::new("/opt/bin/teamwire"), Some(bed.home.path()));
    let changes = bed.plan(&entry);
    let home = bed.home.path().display().to_string();
    assert_eq!(
        planned_file(&changes, "teamwire/.mcp.json")["mcpServers"]["teamwire"]["args"],
        json!(["--home", home, "mcp", "--lead"])
    );
    assert_eq!(
        planned_file(&changes, "teamwire/hooks/hooks.json")["hooks"]["Stop"][0]["hooks"][0]["command"],
        format!("/opt/bin/teamwire --home {home} hook claude-lead-stop")
    );
}

#[test]
fn a_current_setup_plans_nothing_and_does_not_start_claude() {
    let bed = Bed::new();
    bed.install(&want());
    let before = bed.platform.runs().len();
    assert_eq!(bed.plan(&want()), []);
    assert_eq!(bed.platform.runs().len(), before);
}

#[test]
fn a_new_binary_rewrites_the_files_and_updates_the_marketplace_and_the_plugin() {
    let bed = Bed::new();
    let old = McpEntry::lead(Path::new("/old/bin/teamwire"), None);
    bed.install(&old);
    // Claude Code keeps its own copy of the plugin, made at install time.
    let copy = bed.dir.path().join("plugins/cache/teamwire");
    std::fs::create_dir_all(&copy).unwrap();
    for rel in [
        ".mcp.json",
        "hooks/hooks.json",
        ".claude-plugin/plugin.json",
    ] {
        let to = copy.join(rel);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(bed.plugin_dir().join("teamwire").join(rel), to).unwrap();
    }
    bed.register(&copy, true);

    let changes = bed.plan(&want());
    assert!(
        matches!(changes[0], Change::WritePlugin { .. }),
        "{changes:?}"
    );
    assert_eq!(
        changes[1..],
        [Change::UpdateMarketplace, Change::UpdatePlugin]
    );
    bed.claude.apply_lead_setup(&changes).unwrap();
    let runs = bed.claude_runs();
    assert_eq!(
        runs[runs.len() - 2..],
        [
            args("plugin marketplace update teamwire"),
            args("plugin update teamwire@teamwire --scope user")
        ]
    );
}

#[test]
fn files_that_are_current_but_not_loaded_by_claude_still_update_the_plugin() {
    let bed = Bed::new();
    bed.install(&want());
    let stale = bed.dir.path().join("plugins/cache/stale");
    std::fs::create_dir_all(&stale).unwrap();
    bed.register(&stale, true);
    assert_eq!(
        bed.plan(&want()),
        [Change::UpdateMarketplace, Change::UpdatePlugin]
    );
}

#[test]
fn a_disabled_plugin_is_enabled() {
    let bed = Bed::new();
    bed.install(&want());
    bed.register(&bed.plugin_dir().join("teamwire"), false);
    assert_eq!(bed.plan(&want()), [Change::EnablePlugin]);
}

#[test]
fn a_marketplace_from_another_source_is_refused_unless_forced() {
    let bed = Bed::new();
    bed.install(&want());
    bed.put_claude_file(
        "plugins/known_marketplaces.json",
        &json!({"teamwire": {"source": {"source": "github", "repo": "someone/else"}}}),
    );
    let err = bed
        .claude
        .plan_install(&want(), bed.home.path(), false)
        .unwrap_err();
    assert!(err.to_string().contains("pass --force to replace"), "{err}");
    let forced = bed
        .claude
        .plan_install(&want(), bed.home.path(), true)
        .unwrap();
    assert_eq!(
        forced,
        [
            Change::RemoveMarketplace,
            Change::AddMarketplace(bed.plugin_dir())
        ]
    );
}

#[test]
fn the_old_skill_and_entry_are_removed_after_the_plugin_is_in_place() {
    let bed = Bed::new();
    bed.put_skill(SKILL_TEXT);
    bed.put_entry(
        &json!({"type": "stdio", "command": "teamwire", "args": ["mcp", "--lead"], "env": {}}),
    );
    let state = lead_setup_state(bed.claude.config(), &want()).unwrap();
    assert_eq!(state.skill, SkillState::Outdated);
    assert!(matches!(state.mcp, McpState::Outdated(_)));
    let changes = bed.plan(&want());
    assert_eq!(
        changes[changes.len() - 2..],
        [
            Change::RemoveSkill(bed.skill_path()),
            Change::RemoveMcp("teamwire mcp --lead".to_owned())
        ],
        "{changes:?}"
    );
    assert_eq!(changes[changes.len() - 3], Change::InstallPlugin);
    bed.claude.apply_lead_setup(&changes).unwrap();
    assert!(!bed.skill_path().exists());
    assert_eq!(
        bed.claude_runs().last().unwrap(),
        &args("mcp remove --scope user teamwire")
    );
}

#[test]
fn an_edited_old_skill_is_refused_before_anything_is_written_unless_forced() {
    let bed = Bed::new();
    bed.put_skill("---\nname: teamwire\n---\nmy own notes\n");
    let err = bed
        .claude
        .plan_install(&want(), bed.home.path(), false)
        .unwrap_err();
    assert!(matches!(err, LeadSetupError::Refused(_)));
    assert!(err.to_string().contains("--force to remove"), "{err}");
    assert!(
        bed.platform.runs().is_empty(),
        "the refusal comes before claude runs"
    );
    assert!(!bed.plugin_dir().exists());
    let forced = bed
        .claude
        .plan_install(&want(), bed.home.path(), true)
        .unwrap();
    assert_eq!(
        forced.last().unwrap(),
        &Change::RemoveSkill(bed.skill_path())
    );
}

#[test]
fn a_foreign_old_entry_is_refused_without_its_secrets_unless_forced() {
    let bed = Bed::new();
    bed.put_entry(&json!({"command": "teamwire", "args": ["mcp", "--lead"], "env": {"TEAMWIRE_TOKEN": "s3cret"}}));
    let err = bed
        .claude
        .plan_install(&want(), bed.home.path(), false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("with 1 environment variable"), "{err}");
    assert!(!err.contains("s3cret"), "{err}");
    let forced = bed
        .claude
        .plan_install(&want(), bed.home.path(), true)
        .unwrap();
    assert!(matches!(forced.last(), Some(Change::RemoveMcp(_))));
}

#[test]
fn a_missing_claude_is_refused_before_the_files_are_written() {
    let bed = Bed::new();
    bed.platform.queue_run(Err(PlatformError::new(
        PlatformErrorCode::NotFound,
        "claude: not found",
    )));
    let err = bed
        .claude
        .plan_install(&want(), bed.home.path(), false)
        .unwrap_err();
    assert!(matches!(err, LeadSetupError::Refused(_)));
    assert!(
        err.to_string().contains("cannot run `claude plugin`"),
        "{err}"
    );
    assert!(!bed.plugin_dir().exists());
}

#[test]
fn a_failure_partway_names_what_was_done_and_what_failed() {
    let bed = Bed::new();
    let changes = bed.plan(&want());
    bed.platform.queue_run(Ok(exited(1, "no such directory")));
    let err = bed.claude.apply_lead_setup(&changes).unwrap_err();
    assert_eq!(err.done.len(), 1);
    assert!(matches!(err.done[0], Change::WritePlugin { .. }));
    assert_eq!(err.failed, Change::AddMarketplace(bed.plugin_dir()));
    assert!(err.detail.contains("no such directory"), "{}", err.detail);
}

#[test]
fn claude_files_that_are_not_json_are_an_error_not_an_empty_config() {
    for rel in [
        ".claude.json",
        "plugins/installed_plugins.json",
        "settings.json",
    ] {
        let bed = Bed::new();
        let path = bed.dir.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "{ not json").unwrap();
        let err = bed
            .claude
            .plan_install(&want(), bed.home.path(), false)
            .unwrap_err();
        assert!(matches!(err, LeadSetupError::Read { .. }), "{rel}: {err}");
    }
}

#[test]
fn uninstall_removes_the_plugin_the_marketplace_and_the_files() {
    let bed = Bed::new();
    bed.install(&want());
    assert_eq!(
        bed.claude.plan_uninstall(bed.home.path(), false).unwrap(),
        [
            Change::UninstallPlugin,
            Change::RemoveMarketplace,
            Change::RemovePluginFiles(bed.plugin_dir())
        ]
    );
    let changes = bed.claude.plan_uninstall(bed.home.path(), false).unwrap();
    bed.claude.apply_lead_setup(&changes).unwrap();
    assert!(!bed.plugin_dir().exists());
    let runs = bed.claude_runs();
    assert_eq!(
        runs[runs.len() - 2..],
        [
            args("plugin uninstall teamwire@teamwire --scope user"),
            args("plugin marketplace remove --scope user teamwire")
        ]
    );
}

#[test]
fn uninstall_also_removes_the_old_setup_and_keeps_the_users_other_files() {
    let bed = Bed::new();
    bed.put_skill(SKILL_TEXT);
    bed.put_entry(&json!({"command": "teamwire", "args": ["mcp", "--lead"]}));
    let notes = bed.skill_path().with_file_name("notes.md");
    std::fs::write(&notes, "mine").unwrap();
    let changes = bed.claude.plan_uninstall(bed.home.path(), false).unwrap();
    assert_eq!(
        changes,
        [
            Change::RemoveSkill(bed.skill_path()),
            Change::RemoveMcp("teamwire mcp --lead".to_owned())
        ]
    );
    bed.claude.apply_lead_setup(&changes).unwrap();
    assert!(!bed.skill_path().exists());
    assert!(notes.exists(), "a file beside the skill stays");
}

#[test]
fn uninstall_with_nothing_installed_plans_nothing_and_refuses_what_is_not_ours() {
    let bed = Bed::new();
    assert_eq!(
        bed.claude.plan_uninstall(bed.home.path(), false).unwrap(),
        []
    );
    bed.put_entry(&json!({"command": "/bin/other", "args": ["serve"]}));
    let err = bed
        .claude
        .plan_uninstall(bed.home.path(), false)
        .unwrap_err();
    assert!(err.to_string().contains("--force to remove"), "{err}");
    assert_eq!(
        bed.claude.plan_uninstall(bed.home.path(), true).unwrap(),
        [Change::RemoveMcp("/bin/other serve".to_owned())]
    );
}

#[test]
fn plugin_problems_name_what_is_missing_stale_or_off() {
    let bed = Bed::new();
    let config = bed.claude.config();
    let problems = |files: bool| config.plugin_problems(&want(), files).unwrap();
    assert_eq!(problems(true).len(), 1);
    assert!(problems(true)[0].contains("not installed"));

    bed.install(&want());
    assert_eq!(problems(true), Vec::<String>::new());

    bed.register(&bed.plugin_dir().join("teamwire"), false);
    assert!(problems(true)[0].contains("disabled"));

    bed.register(&bed.plugin_dir().join("teamwire"), true);
    let other = McpEntry::lead(Path::new("/new/bin/teamwire"), None);
    let stale = config.plugin_problems(&other, true).unwrap();
    assert!(stale[0].contains("another version"), "{stale:?}");
    assert_eq!(
        config.plugin_problems(&other, false).unwrap(),
        Vec::<String>::new(),
        "a build under target/ is not compared"
    );
}
