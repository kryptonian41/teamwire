//! `teamwire install` and `teamwire uninstall` end to end: the binary runs
//! with `fake-claude` as `claude`, and Claude's directory, `HOME` and the
//! state directory are all temporary. The binary under test is linked to a
//! path outside `target/`, as an installed one is, so the refusal of a build
//! does not come first.
#![expect(
    clippy::disallowed_methods,
    reason = "the test starts the teamwire binary and edits Claude's files as a user would"
)]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
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

/// `real` linked (or copied, across volumes) to `<dir>/teamwire`, so the
/// binary does not run from under `target/`.
fn link_binary(real: &Path, dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let to = dir.join("teamwire");
    // Once only: copying onto a hard link of `real` would truncate `real`.
    if !to.exists() {
        std::fs::hard_link(real, &to)
            .or_else(|_| std::fs::copy(real, &to).map(|_| ()))
            .unwrap();
    }
    std::fs::canonicalize(to).unwrap()
}

/// A `pi` that answers `--version` and keeps the package list that
/// `pi install` and `pi remove` change in `$PI_CODING_AGENT_DIR`, as pi
/// does. `FAKE_PI_FAIL=install` makes `install` fail.
fn fake_pi() -> TestHome {
    use std::os::unix::fs::PermissionsExt;
    let dir = TestHome::new().unwrap();
    let script = r#"#!/bin/sh
S="$PI_CODING_AGENT_DIR"
mkdir -p "$S"
echo "$*" >> "$S/calls.log"
case "$1" in
  --version) echo "1.4.0";;
  install)
    if [ "$FAKE_PI_FAIL" = install ]; then echo "boom" >&2; exit 1; fi
    printf '{"packages":["%s"]}\n' "$2" > "$S/settings.json";;
  remove) printf '{"packages":[]}\n' > "$S/settings.json";;
  *) exit 2;;
esac
"#;
    let path = dir.path().join("pi");
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    dir
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

    /// An installed copy of the binary under test, named by `tag`.
    fn binary(&self, tag: &str) -> PathBuf {
        link_binary(
            Path::new(env!("CARGO_BIN_EXE_teamwire")),
            &self.user.path().join(tag),
        )
    }

    fn teamwire_with(&self, bin: &Path, args: &[&str], extra_env: &[(&str, &str)]) -> Run {
        let mut cmd = std::process::Command::new(bin);
        cmd.arg("--home")
            .arg(self.home.path())
            .args(args)
            .env("PATH", self.bins.path())
            .env("HOME", self.user.path())
            .env("TEAMWIRE_CLAUDE_DIR", self.claude.path())
            .env_remove("CLAUDE_CONFIG_DIR")
            .env_remove("FAKE_CLAUDE_MCP_FAIL")
            .env_remove("FAKE_CLAUDE_PLUGIN_FAIL");
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

    fn teamwire(&self, args: &[&str], extra_env: &[(&str, &str)]) -> Run {
        self.teamwire_with(&self.binary("bin"), args, extra_env)
    }

    /// `teamwire` with the fake `pi` on `PATH` and pi's directory in `dir`.
    fn teamwire_pi(&self, pi: &TestHome, dir: &Path, args: &[&str], extra: &[(&str, &str)]) -> Run {
        let path = format!("{}:{}", pi.path().display(), self.bins.path().display());
        let dir = dir.display().to_string();
        let mut env = vec![("PATH", path.as_str()), ("PI_CODING_AGENT_DIR", &dir)];
        env.extend_from_slice(extra);
        self.teamwire(args, &env)
    }

    fn plugin_dir(&self) -> PathBuf {
        self.home.path().join("claude-plugin")
    }

    fn old_skill(&self) -> PathBuf {
        self.claude.path().join("skills/teamwire/SKILL.md")
    }

    fn claude_json(&self, rel: &str) -> Value {
        std::fs::read_to_string(self.claude.path().join(rel))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Value::Null)
    }

    /// Where Claude Code loaded the user-scope plugin from, if it did.
    fn installed_root(&self) -> Option<PathBuf> {
        self.claude_json("plugins/installed_plugins.json")["plugins"]["teamwire@teamwire"][0]
            ["installPath"]
            .as_str()
            .map(PathBuf::from)
    }

    fn marketplace_added(&self) -> bool {
        self.claude_json("plugins/known_marketplaces.json")["teamwire"].is_object()
    }

    /// The user-scope `teamwire` MCP entry that ADR 0012 wrote, or `Value::Null`.
    fn old_entry(&self) -> Value {
        self.claude_json(".claude.json")
            .pointer("/mcpServers/teamwire")
            .cloned()
            .unwrap_or(Value::Null)
    }

    fn put_old_entry(&self, entry: &Value) {
        let doc = json!({"mcpServers": {"teamwire": entry}});
        std::fs::write(self.claude.path().join(".claude.json"), doc.to_string()).unwrap();
    }

    fn put_old_skill(&self, text: &str) {
        std::fs::create_dir_all(self.old_skill().parent().unwrap()).unwrap();
        std::fs::write(self.old_skill(), text).unwrap();
    }
}

fn mcp_command(root: &Path) -> String {
    let doc: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join(".mcp.json")).unwrap()).unwrap();
    doc["mcpServers"]["teamwire"]["command"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn a_build_under_target_is_refused_and_nothing_is_written() {
    let bed = Bed::new();
    let run = bed.teamwire_with(Path::new(env!("CARGO_BIN_EXE_teamwire")), &["install"], &[]);
    assert_eq!(run.code, 1, "{}", run.err);
    assert!(run.err.contains("target/"), "{}", run.err);
    assert!(!bed.plugin_dir().exists());
    assert!(!bed.marketplace_added());
}

#[test]
fn install_registers_the_plugin_and_a_second_run_changes_nothing() {
    let bed = Bed::new();
    let run = bed.teamwire(&["install"], &[]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    for line in [
        "did write the teamwire plugin files",
        "did add the Claude Code marketplace `teamwire`",
        "did install the plugin `teamwire@teamwire`",
    ] {
        assert!(run.out.contains(line), "{line}: {}", run.out);
    }
    assert!(bed.marketplace_added());
    assert_eq!(
        bed.claude_json("settings.json")["enabledPlugins"]["teamwire@teamwire"],
        true
    );
    let root = bed.installed_root().unwrap();
    assert_eq!(mcp_command(&root), bed.binary("bin").display().to_string());
    let afk = std::fs::read_to_string(root.join("skills/afk/SKILL.md")).unwrap();
    assert!(afk.contains("\nname: afk\n"), "{afk}");
    let hooks = std::fs::read_to_string(root.join("hooks/hooks.json")).unwrap();
    let home = bed.home.path().display().to_string();
    assert!(
        hooks.contains(&format!(
            "{} --home {home} hook claude-lead-stop",
            bed.binary("bin").display()
        )),
        "{hooks}"
    );

    let again = bed.teamwire(&["install"], &[]);
    assert_eq!(again.code, 0, "{}", again.err);
    assert_eq!(again.out, "the lead setup is already installed\n");
}

#[test]
fn a_new_binary_refreshes_the_copy_claude_keeps() {
    let bed = Bed::new();
    assert_eq!(bed.teamwire(&["install"], &[]).code, 0);
    let first = bed.installed_root().unwrap();

    let moved = bed.binary("other");
    let run = bed.teamwire_with(&moved, &["install"], &[]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    for line in [
        "did write the teamwire plugin files",
        "did update the Claude Code marketplace `teamwire`",
        "did update the plugin `teamwire@teamwire`",
    ] {
        assert!(run.out.contains(line), "{line}: {}", run.out);
    }
    let second = bed.installed_root().unwrap();
    assert_ne!(first, second, "the plugin version moved with the binary");
    assert_eq!(mcp_command(&second), moved.display().to_string());
    assert_eq!(
        bed.teamwire_with(&moved, &["install"], &[]).out,
        "the lead setup is already installed\n"
    );
}

#[test]
fn a_dry_run_prints_the_changes_and_makes_none() {
    let bed = Bed::new();
    bed.put_old_entry(&json!({"command": "teamwire", "args": ["mcp", "--lead"]}));
    bed.put_old_skill(teamwire_harness_claude::SKILL_TEXT);
    let run = bed.teamwire(&["install", "--dry-run"], &[]);
    assert_eq!(run.code, 0, "{}", run.err);
    for line in [
        "would write the teamwire plugin files",
        "would add the Claude Code marketplace",
        "would install the plugin",
        "would remove the old teamwire skill",
        "would remove the old user MCP server `teamwire` (teamwire mcp --lead)",
    ] {
        assert!(run.out.contains(line), "{line}: {}", run.out);
    }
    assert!(!bed.plugin_dir().exists());
    assert!(bed.old_skill().exists());
    assert_ne!(bed.old_entry(), Value::Null);
}

#[test]
fn a_failed_install_exits_two_says_what_is_left_and_a_rerun_finishes() {
    let bed = Bed::new();
    let run = bed.teamwire(&["install"], &[("FAKE_CLAUDE_PLUGIN_FAIL", "install")]);
    assert_eq!(run.code, 2, "{}", run.err);
    assert!(
        run.err
            .contains("already done: write the teamwire plugin files"),
        "{}",
        run.err
    );
    assert!(
        run.err
            .contains("already done: add the Claude Code marketplace"),
        "{}",
        run.err
    );
    assert!(
        run.err.contains("run `teamwire install` again"),
        "{}",
        run.err
    );
    assert!(bed.installed_root().is_none());

    let rerun = bed.teamwire(&["install"], &[]);
    assert_eq!(rerun.code, 0, "{}", rerun.err);
    assert!(
        !rerun.out.contains("write the teamwire plugin files"),
        "the files were already current: {}",
        rerun.out
    );
    assert!(bed.installed_root().is_some());
}

#[test]
fn a_failure_that_changed_nothing_exits_one_not_two() {
    let bed = Bed::new();
    let fail = [("FAKE_CLAUDE_PLUGIN_FAIL", "marketplace add")];
    // The first run writes the files and then fails to add the marketplace.
    assert_eq!(bed.teamwire(&["install"], &fail).code, 2);
    // The files are current now, so the second run fails with nothing changed.
    let run = bed.teamwire(&["install"], &fail);
    assert_eq!(run.code, 1, "{}", run.err);
    assert!(!run.err.contains("already done:"), "{}", run.err);
}

#[test]
fn install_removes_the_old_skill_and_entry_after_the_plugin_is_in() {
    let bed = Bed::new();
    bed.put_old_skill(teamwire_harness_claude::SKILL_TEXT);
    bed.put_old_entry(
        &json!({"type": "stdio", "command": "/old/teamwire", "args": ["mcp", "--lead"], "env": {}}),
    );
    let run = bed.teamwire(&["install"], &[]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(!bed.old_skill().exists());
    assert_eq!(bed.old_entry(), Value::Null);
    assert!(bed.installed_root().is_some());
    let plugin_at = run.out.find("install the plugin").unwrap();
    let skill_at = run.out.find("remove the old teamwire skill").unwrap();
    assert!(plugin_at < skill_at, "{}", run.out);
}

#[test]
fn an_edited_old_skill_or_a_foreign_entry_is_refused_and_nothing_is_changed() {
    let bed = Bed::new();
    bed.put_old_skill("---\nname: teamwire\n---\nmy own notes\n");
    let run = bed.teamwire(&["install"], &[]);
    assert_eq!(run.code, 1, "{}", run.err);
    assert!(run.err.contains("--force"), "{}", run.err);
    assert!(!bed.plugin_dir().exists());
    assert!(!bed.marketplace_added());

    std::fs::remove_file(bed.old_skill()).unwrap();
    bed.put_old_entry(&json!({"command": "/usr/local/bin/other-tool", "args": ["serve"]}));
    let run = bed.teamwire(&["install"], &[]);
    assert_eq!(run.code, 1, "{}", run.err);
    assert!(run.err.contains("other-tool"), "{}", run.err);
    assert!(!bed.plugin_dir().exists());

    let forced = bed.teamwire(&["install", "--force"], &[]);
    assert_eq!(forced.code, 0, "{}", forced.err);
    assert_eq!(bed.old_entry(), Value::Null);
}

#[test]
fn uninstall_removes_the_plugin_the_marketplace_and_the_files_and_then_has_nothing_to_do() {
    let bed = Bed::new();
    assert_eq!(bed.teamwire(&["install"], &[]).code, 0);
    let run = bed.teamwire(&["uninstall"], &[]);
    assert_eq!(run.code, 0, "{}", run.err);
    assert!(bed.installed_root().is_none());
    assert!(!bed.marketplace_added());
    assert!(!bed.plugin_dir().exists());
    assert_eq!(
        bed.claude_json("settings.json")["enabledPlugins"]["teamwire@teamwire"],
        Value::Null
    );
    let again = bed.teamwire(&["uninstall"], &[]);
    assert_eq!(again.out, "no lead setup to remove\n");
}

#[test]
fn install_covers_both_harnesses_when_both_are_named() {
    let bed = Bed::new();
    let pi = fake_pi();
    let agent = TestHome::new().unwrap();
    let args = ["install", "--harness", "claude-code,pi"];
    let run = bed.teamwire_pi(&pi, agent.path(), &args, &[]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(bed.home.path().join("pi-package").exists());
    assert!(bed.installed_root().is_some());
}

#[test]
fn without_a_terminal_or_a_flag_install_keeps_to_claude_code() {
    let bed = Bed::new();
    let pi = fake_pi();
    let agent = TestHome::new().unwrap();
    let run = bed.teamwire_pi(&pi, agent.path(), &["install"], &[]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(bed.installed_root().is_some());
    let calls = std::fs::read_to_string(agent.path().join("calls.log")).unwrap_or_default();
    assert_eq!(calls, "", "pi was not run");
}

#[test]
fn an_unknown_harness_is_a_usage_error_and_one_not_on_path_a_refusal_and_both_name_it() {
    let bed = Bed::new();
    let run = bed.teamwire(&["install", "--harness", "gemini"], &[]);
    assert_eq!(run.code, 3, "{}", run.err);
    assert!(run.err.contains("`gemini`"), "{}", run.err);
    // `teamwire` without the fake `pi` on PATH.
    let run = bed.teamwire(&["install", "--harness", "pi"], &[]);
    assert_eq!(run.code, 1, "a refusal, not a usage error: {}", run.err);
    assert!(run.err.contains("`pi` is not on PATH"), "{}", run.err);
    assert!(!bed.home.path().join("pi-package").exists());
    assert!(bed.installed_root().is_none(), "nothing was changed");
}

#[test]
fn the_help_of_install_and_uninstall_names_the_plugin() {
    let bed = Bed::new();
    for command in ["install", "uninstall"] {
        let run = bed.teamwire(&[command, "--help"], &[]);
        assert_eq!(run.code, 0, "{}", run.err);
        assert!(run.out.contains("`teamwire@teamwire`"), "{}", run.out);
    }
}

#[test]
fn uninstall_ignores_the_pi_setup_without_pi_and_removes_it_with_pi() {
    let bed = Bed::new();
    let pi = fake_pi();
    let agent = TestHome::new().unwrap();
    let install = bed.teamwire_pi(&pi, agent.path(), &["install", "--harness", "pi"], &[]);
    assert_eq!(install.code, 0, "{}", install.err);
    // Without `pi` on PATH there is nothing to run, so nothing is removed.
    let dir = agent.path().display().to_string();
    let none = bed.teamwire(&["uninstall"], &[("PI_CODING_AGENT_DIR", &dir)]);
    assert_eq!(none.out, "no lead setup to remove\n", "{}", none.err);
    assert!(bed.home.path().join("pi-package").exists());

    let run = bed.teamwire_pi(&pi, agent.path(), &["uninstall"], &[]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    for line in [
        "did remove the pi package",
        "did delete the teamwire pi package",
    ] {
        assert!(run.out.contains(line), "{line}: {}", run.out);
    }
    assert!(!bed.home.path().join("pi-package").exists());
    let again = bed.teamwire_pi(&pi, agent.path(), &["uninstall"], &[]);
    assert_eq!(again.out, "no lead setup to remove\n");
}

#[test]
fn a_broken_pi_setup_does_not_block_the_claude_uninstall() {
    let bed = Bed::new();
    assert_eq!(bed.teamwire(&["install"], &[]).code, 0);
    // A pi that runs `--version`, beside a settings file it cannot read.
    let pi = fake_pi();
    let agent = TestHome::new().unwrap();
    std::fs::write(agent.path().join("settings.json"), "not json").unwrap();
    for dry in [true, false] {
        let args: &[&str] = if dry {
            &["uninstall", "--dry-run"]
        } else {
            &["uninstall"]
        };
        let run = bed.teamwire_pi(&pi, agent.path(), args, &[]);
        assert_eq!(run.code, 0, "{}", run.err);
        assert!(run.err.contains("skipped the pi part"), "{}", run.err);
        assert!(
            run.out.contains("the plugin `teamwire@teamwire`"),
            "{}",
            run.out
        );
    }
    assert!(bed.installed_root().is_none());
    // Asking for pi by name still fails clearly.
    let run = bed.teamwire_pi(&pi, agent.path(), &["install", "--harness", "pi"], &[]);
    assert_eq!(run.code, 1, "{}", run.err);
    assert!(run.err.contains("settings.json"), "{}", run.err);
}

#[test]
fn install_adds_the_pi_package_and_a_second_run_changes_nothing() {
    let bed = Bed::new();
    let pi = fake_pi();
    let agent = TestHome::new().unwrap();
    let run = bed.teamwire_pi(&pi, agent.path(), &["install", "--harness", "pi"], &[]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    let dir = bed.home.path().join("pi-package");
    for line in [
        format!("did write the teamwire pi package to {}", dir.display()),
        format!("did add the pi package {}", dir.display()),
        "pi loads packages at start".to_owned(),
    ] {
        assert!(run.out.contains(&line), "{line}: {}", run.out);
    }
    assert!(!bed.marketplace_added(), "Claude Code was not asked for");
    let extension = std::fs::read_to_string(dir.join("extensions/teamwire-lead.ts")).unwrap();
    let home = bed.home.path().display().to_string();
    assert!(
        extension.contains(&format!(r#""homeArgs":["--home","{home}"]"#)),
        "{extension}"
    );
    assert!(
        extension.contains(&format!(r#""bin":"{}""#, bed.binary("bin").display())),
        "{extension}"
    );
    let calls = std::fs::read_to_string(agent.path().join("calls.log")).unwrap();
    let installs: Vec<_> = calls.lines().filter(|l| l.starts_with("install")).collect();
    assert_eq!(installs, [format!("install {}", dir.display())], "{calls}");

    let again = bed.teamwire_pi(&pi, agent.path(), &["install", "--harness", "pi"], &[]);
    assert_eq!(again.code, 0, "{}", again.err);
    assert_eq!(again.out, "the lead setup is already installed\n");
}

#[test]
fn a_failed_pi_install_exits_two_and_a_rerun_finishes() {
    let bed = Bed::new();
    let pi = fake_pi();
    let agent = TestHome::new().unwrap();
    let args = ["install", "--harness", "pi"];
    let run = bed.teamwire_pi(&pi, agent.path(), &args, &[("FAKE_PI_FAIL", "install")]);
    assert_eq!(run.code, 2, "{}", run.err);
    assert!(run.err.contains("boom"), "{}", run.err);
    assert!(
        run.err
            .contains("already done: write the teamwire pi package"),
        "{}",
        run.err
    );
    let rerun = bed.teamwire_pi(&pi, agent.path(), &args, &[]);
    assert_eq!(rerun.code, 0, "{}", rerun.err);
    assert!(
        rerun.out.contains("did add the pi package"),
        "{}",
        rerun.out
    );
    assert!(!rerun.out.contains("did write"), "{}", rerun.out);
}

#[test]
fn uninstall_removes_the_pi_package_and_its_files() {
    let bed = Bed::new();
    let pi = fake_pi();
    let agent = TestHome::new().unwrap();
    let install = bed.teamwire_pi(&pi, agent.path(), &["install", "--harness", "pi"], &[]);
    assert_eq!(install.code, 0, "{}", install.err);
    let run = bed.teamwire_pi(&pi, agent.path(), &["uninstall"], &[]);
    assert_eq!(run.code, 0, "{}{}", run.out, run.err);
    assert!(run.out.contains("did remove the pi package"), "{}", run.out);
    assert!(
        run.out.contains("did delete the teamwire pi package"),
        "{}",
        run.out
    );
    assert!(!bed.home.path().join("pi-package").exists());
    let again = bed.teamwire_pi(&pi, agent.path(), &["uninstall"], &[]);
    assert_eq!(again.out, "no lead setup to remove\n", "{}", again.err);
}
