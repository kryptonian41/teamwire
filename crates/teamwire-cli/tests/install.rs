//! `teamwire install` and `teamwire uninstall` end to end: the binary runs
//! with `fake-claude` as `claude`, and Claude's directory, `HOME` and the
//! state directory are all temporary.
#![expect(
    clippy::disallowed_methods,
    reason = "the test starts the teamwire binary and edits Claude's files as a user would"
)]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

use std::path::PathBuf;

use serde_json::Value;
use teamwire_fakes::{FakeBinDir, FakeBins, TestHome};

struct Bed {
    home: TestHome,
    claude: TestHome,
    user: TestHome,
    bins: FakeBinDir,
    _bin_dir: TestHome,
}

struct Run {
    code: i32,
    out: String,
    err: String,
}

impl Bed {
    fn new() -> Self {
        let bin_dir = TestHome::new().unwrap();
        let bins = FakeBins::ensure()
            .unwrap()
            .install(&bin_dir.path().join("bin"))
            .unwrap();
        Self {
            home: TestHome::new().unwrap(),
            claude: TestHome::new().unwrap(),
            user: TestHome::new().unwrap(),
            bins,
            _bin_dir: bin_dir,
        }
    }

    fn teamwire(&self, args: &[&str], extra_env: &[(&str, &str)]) -> Run {
        let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"));
        cmd.arg("--home")
            .arg(self.home.path())
            .args(args)
            .env("PATH", self.bins.path())
            .env("HOME", self.user.path())
            .env("TEAMWIRE_CLAUDE_DIR", self.claude.path())
            .env_remove("CLAUDE_CONFIG_DIR")
            .env_remove("FAKE_CLAUDE_MCP_FAIL");
        for (k, v) in extra_env {
            cmd.env(k, v);
        }
        let out = cmd.output().unwrap();
        Run {
            code: out.status.code().unwrap(),
            out: String::from_utf8(out.stdout).unwrap(),
            err: String::from_utf8(out.stderr).unwrap(),
        }
    }

    fn skill(&self) -> PathBuf {
        self.claude.path().join("skills/teamwire/SKILL.md")
    }

    /// The user-scope `teamwire` entry, or `Value::Null`.
    fn entry(&self) -> Value {
        std::fs::read_to_string(self.claude.path().join(".claude.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|d| d.pointer("/mcpServers/teamwire").cloned())
            .unwrap_or(Value::Null)
    }

    fn put_entry(&self, entry: &Value) {
        let doc = serde_json::json!({"mcpServers": {"teamwire": entry}});
        std::fs::write(self.claude.path().join(".claude.json"), doc.to_string()).unwrap();
    }
}

fn this_binary() -> String {
    std::fs::canonicalize(env!("CARGO_BIN_EXE_teamwire"))
        .unwrap()
        .display()
        .to_string()
}

#[test]
fn a_build_under_target_is_refused_and_nothing_is_written() {
    let bed = Bed::new();
    let run = bed.teamwire(&["install"], &[]);
    assert_eq!(run.code, 1, "{}", run.err);
    assert!(run.err.contains("target/"), "{}", run.err);
    assert!(!bed.skill().exists());
    assert_eq!(bed.entry(), Value::Null);
}

#[test]
fn install_writes_both_items_and_a_second_run_changes_nothing() {
    let bed = Bed::new();
    let run = bed.teamwire(&["install", "--force"], &[]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(
        run.out.contains("did write the teamwire skill"),
        "{}",
        run.out
    );
    assert!(
        std::fs::read_to_string(bed.skill())
            .unwrap()
            .contains("teamwire-sha256: ")
    );
    let home = bed.home.path().display().to_string();
    assert_eq!(bed.entry()["command"], this_binary().as_str());
    assert_eq!(
        bed.entry()["args"],
        serde_json::json!(["--home", home, "mcp", "--lead"])
    );

    let again = bed.teamwire(&["install", "--force"], &[]);
    assert_eq!(again.code, 0, "{}", again.err);
    assert_eq!(again.out, "the lead setup is already installed\n");

    let doctor = bed.teamwire(&["doctor"], &[]);
    assert!(doctor.out.contains("ok   teamwire skill"), "{}", doctor.out);
    assert!(
        doctor.out.contains("ok   lead MCP server"),
        "{}",
        doctor.out
    );
}

#[test]
fn a_dry_run_prints_the_changes_and_makes_none() {
    let bed = Bed::new();
    let run = bed.teamwire(&["install", "--dry-run", "--force"], &[]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(
        run.out.contains("would write the teamwire skill"),
        "{}",
        run.out
    );
    assert!(
        run.out.contains("would add the user MCP server"),
        "{}",
        run.out
    );
    assert!(!bed.skill().exists());
    assert_eq!(bed.entry(), Value::Null);
}

#[test]
fn a_failed_add_exits_two_says_what_is_left_and_a_rerun_finishes() {
    let bed = Bed::new();
    bed.put_entry(&serde_json::json!({"command": "teamwire", "args": ["mcp", "--lead"]}));
    let run = bed.teamwire(&["install", "--force"], &[("FAKE_CLAUDE_MCP_FAIL", "add")]);
    assert_eq!(run.code, 2, "{}", run.err);
    assert!(
        run.err.contains("already done: write the teamwire skill"),
        "{}",
        run.err
    );
    assert!(
        run.err.contains("the old MCP server entry was removed"),
        "{}",
        run.err
    );
    assert!(
        run.err.contains("run `teamwire install` again"),
        "{}",
        run.err
    );
    assert_eq!(bed.entry(), Value::Null);

    let rerun = bed.teamwire(&["install", "--force"], &[]);
    assert_eq!(rerun.code, 0, "{}", rerun.err);
    assert!(
        !rerun.out.contains("skill"),
        "the skill was already current: {}",
        rerun.out
    );
    assert_eq!(bed.entry()["command"], this_binary().as_str());
}

#[test]
fn uninstall_removes_both_items_and_then_has_nothing_to_do() {
    let bed = Bed::new();
    assert_eq!(bed.teamwire(&["install", "--force"], &[]).code, 0);
    let run = bed.teamwire(&["uninstall"], &[]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(!bed.skill().exists());
    assert_eq!(bed.entry(), Value::Null);
    let again = bed.teamwire(&["uninstall"], &[]);
    assert_eq!(again.out, "no lead setup to remove\n");
}
