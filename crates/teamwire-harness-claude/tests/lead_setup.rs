//! The lead setup: what install and uninstall plan for each state of the
//! skill file and the MCP entry, the refusals that come before any write,
//! and what a failure partway through reports. `claude` is never started:
//! the fake platform records each command and answers from a queue.

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

use serde_json::json;
use teamwire_core::{PlatformError, PlatformErrorCode, RunOutput};
use teamwire_fakes::{FakePlatform, TestHome};
use teamwire_harness_claude::{
    Change, ClaudeCode, ClaudeConfig, LeadSetupError, McpEntry, McpState, SKILL_TEXT, SkillState,
    lead_setup_state,
};

struct Bed {
    dir: TestHome,
    platform: Arc<FakePlatform>,
    claude: ClaudeCode,
}

impl Bed {
    /// Claude's `CLAUDE_CONFIG_DIR` layout in a temporary directory, with
    /// nothing installed.
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
            platform,
            claude,
        }
    }

    fn skill_path(&self) -> PathBuf {
        self.dir.path().join("skills/teamwire/SKILL.md")
    }

    fn put_skill(&self, text: &str) {
        std::fs::create_dir_all(self.skill_path().parent().unwrap()).unwrap();
        std::fs::write(self.skill_path(), text).unwrap();
    }

    fn put_entry(&self, entry: &serde_json::Value) {
        let doc = json!({"numStartups": 3, "mcpServers": {"teamwire": entry}});
        std::fs::write(self.dir.path().join(".claude.json"), doc.to_string()).unwrap();
    }

    /// Installs into the bed for real, so a test starts from "current".
    fn install(&self) {
        let changes = self.claude.plan_install(&want(), false).unwrap();
        self.claude.apply_lead_setup(&changes).unwrap();
        self.put_entry(&json!({"type": "stdio", "command": "/opt/bin/teamwire", "args": ["mcp", "--lead"], "env": {}}));
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

#[test]
fn a_fresh_install_writes_the_stamped_skill_and_adds_the_entry() {
    let bed = Bed::new();
    let changes = bed.claude.plan_install(&want(), false).unwrap();
    assert_eq!(
        changes,
        [Change::WriteSkill(bed.skill_path()), Change::AddMcp(want())]
    );
    assert_eq!(
        bed.claude_runs(),
        [args("--version")],
        "planning only checks claude runs"
    );
    assert!(!bed.skill_path().exists(), "planning writes nothing");

    bed.claude.apply_lead_setup(&changes).unwrap();
    let text = std::fs::read_to_string(bed.skill_path()).unwrap();
    assert!(text.contains("\nmetadata:\n  teamwire-sha256: "));
    assert_eq!(
        bed.claude_runs()[1],
        args("mcp add --scope user teamwire -- /opt/bin/teamwire mcp --lead")
    );
    assert_eq!(
        lead_setup_state(bed.claude.config(), &want())
            .unwrap()
            .skill,
        SkillState::Current
    );
}

#[test]
fn a_current_setup_plans_nothing_and_does_not_start_claude() {
    let bed = Bed::new();
    bed.install();
    let before = bed.platform.runs().len();
    assert_eq!(bed.claude.plan_install(&want(), false).unwrap(), []);
    assert_eq!(bed.platform.runs().len(), before);
}

#[test]
fn an_edited_skill_is_refused_before_anything_is_written_unless_forced() {
    let bed = Bed::new();
    bed.put_skill("---\nname: teamwire\n---\nmy own notes\n");
    let err = bed.claude.plan_install(&want(), false).unwrap_err();
    assert!(matches!(err, LeadSetupError::Refused(_)));
    assert!(err.to_string().contains("--force"), "{err}");
    assert!(
        bed.platform.runs().is_empty(),
        "the refusal comes before claude runs"
    );

    let forced = bed.claude.plan_install(&want(), true).unwrap();
    assert_eq!(forced[0], Change::WriteSkill(bed.skill_path()));
}

#[test]
fn our_unstamped_skill_is_taken_over_and_stamped() {
    let bed = Bed::new();
    bed.put_skill(SKILL_TEXT);
    let changes = bed.claude.plan_install(&want(), false).unwrap();
    assert_eq!(changes[0], Change::WriteSkill(bed.skill_path()));
}

#[test]
fn an_entry_on_path_is_replaced_by_remove_then_add() {
    let bed = Bed::new();
    bed.put_entry(
        &json!({"type": "stdio", "command": "teamwire", "args": ["mcp", "--lead"], "env": {}}),
    );
    let state = lead_setup_state(bed.claude.config(), &want()).unwrap();
    assert!(matches!(state.mcp, McpState::Outdated(_)));
    let changes = bed.claude.plan_install(&want(), false).unwrap();
    assert_eq!(
        changes[1],
        Change::ReplaceMcp {
            old: "teamwire mcp --lead".to_owned(),
            new: want()
        }
    );
    bed.claude.apply_lead_setup(&changes).unwrap();
    let runs = bed.claude_runs();
    assert_eq!(runs[1], args("mcp remove --scope user teamwire"));
    assert_eq!(
        runs[2],
        args("mcp add --scope user teamwire -- /opt/bin/teamwire mcp --lead")
    );
}

#[test]
fn a_foreign_entry_is_refused_unless_forced() {
    let bed = Bed::new();
    bed.put_entry(&json!({"command": "/usr/local/bin/other-tool", "args": ["serve"]}));
    let err = bed.claude.plan_install(&want(), false).unwrap_err();
    assert!(err.to_string().contains("other-tool"), "{err}");
    assert!(!bed.skill_path().exists());
    let forced = bed.claude.plan_install(&want(), true).unwrap();
    assert!(matches!(forced[1], Change::ReplaceMcp { .. }));
}

#[test]
fn a_refusal_names_a_foreign_entry_without_its_secrets() {
    let bed = Bed::new();
    bed.put_entry(&json!({"command": "teamwire", "args": ["mcp", "--lead"], "env": {"TEAMWIRE_TOKEN": "s3cret"}}));
    let err = bed
        .claude
        .plan_install(&want(), false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("with 1 environment variable"), "{err}");
    assert!(!err.contains("s3cret"), "{err}");
}

#[test]
fn uninstall_with_force_removes_a_foreign_entry() {
    let bed = Bed::new();
    bed.put_entry(&json!({"command": "/bin/other", "args": ["serve"]}));
    assert!(bed.claude.plan_uninstall(false).is_err());
    let changes = bed.claude.plan_uninstall(true).unwrap();
    assert_eq!(changes, [Change::RemoveMcp("/bin/other serve".to_owned())]);
    bed.claude.apply_lead_setup(&changes).unwrap();
    assert_eq!(
        bed.claude_runs().last().unwrap(),
        &args("mcp remove --scope user teamwire")
    );
}

#[test]
fn a_missing_claude_is_refused_before_the_skill_is_written() {
    let bed = Bed::new();
    bed.platform.queue_run(Err(PlatformError::new(
        PlatformErrorCode::NotFound,
        "claude: not found",
    )));
    let err = bed.claude.plan_install(&want(), false).unwrap_err();
    assert!(matches!(err, LeadSetupError::Refused(_)));
    assert!(err.to_string().contains("cannot run `claude`"), "{err}");
    assert!(!bed.skill_path().exists());
}

#[test]
fn an_add_that_fails_after_the_remove_says_the_old_entry_is_gone() {
    let bed = Bed::new();
    bed.put_entry(&json!({"command": "teamwire", "args": ["mcp", "--lead"]}));
    let changes = bed.claude.plan_install(&want(), false).unwrap();
    bed.platform.queue_run(Ok(exited(0, ""))); // remove
    bed.platform.queue_run(Ok(exited(1, "disk full"))); // add
    let err = bed.claude.apply_lead_setup(&changes).unwrap_err();
    assert_eq!(err.done, [Change::WriteSkill(bed.skill_path())]);
    assert!(err.entry_removed);
    assert!(err.detail.contains("disk full"), "{}", err.detail);
}

#[test]
fn a_config_file_that_is_not_json_is_an_error_not_an_empty_config() {
    let bed = Bed::new();
    std::fs::write(bed.dir.path().join(".claude.json"), "{ not json").unwrap();
    let err = bed.claude.plan_install(&want(), false).unwrap_err();
    assert!(matches!(err, LeadSetupError::Read { .. }), "{err}");
}

#[test]
fn uninstall_removes_both_and_keeps_the_users_other_files() {
    let bed = Bed::new();
    bed.install();
    let notes = bed.skill_path().with_file_name("notes.md");
    std::fs::write(&notes, "mine").unwrap();
    let changes = bed.claude.plan_uninstall(false).unwrap();
    assert_eq!(
        changes,
        [
            Change::RemoveSkill(bed.skill_path()),
            Change::RemoveMcp("/opt/bin/teamwire mcp --lead".to_owned())
        ]
    );
    bed.claude.apply_lead_setup(&changes).unwrap();
    assert!(!bed.skill_path().exists());
    assert!(notes.exists(), "a file beside the skill stays");
    assert_eq!(
        bed.claude_runs().last().unwrap(),
        &args("mcp remove --scope user teamwire")
    );
}

#[test]
fn uninstall_refuses_an_edited_skill_and_plans_nothing_when_nothing_is_there() {
    let bed = Bed::new();
    assert_eq!(bed.claude.plan_uninstall(false).unwrap(), []);
    bed.put_skill("---\nname: teamwire\n---\nmy own notes\n");
    let err = bed.claude.plan_uninstall(false).unwrap_err();
    assert!(err.to_string().contains("--force to remove"), "{err}");
    assert_eq!(
        bed.claude.plan_uninstall(true).unwrap(),
        [Change::RemoveSkill(bed.skill_path())]
    );
}
