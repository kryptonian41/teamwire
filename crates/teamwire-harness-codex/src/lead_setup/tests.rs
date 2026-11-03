#![expect(
    clippy::indexing_slicing,
    reason = "the tests index lists whose length they just asserted"
)]

use std::sync::Arc;

use teamwire_core::{PlatformError, RunOutput};
use teamwire_fakes::FakePlatform;

use super::*;
use crate::CodexConfig;

#[expect(
    clippy::disallowed_methods,
    reason = "the tests write Codex's config and the plugin files as Codex and install would"
)]
mod bed {
    use super::*;

    pub(super) const SHARED: &str = "---\nname: teamwire\ndescription: d\n---\n\n# Body\n";

    pub(super) struct Bed {
        pub(super) tmp: tempfile::TempDir,
        pub(super) platform: Arc<FakePlatform>,
        pub(super) cli: CodexCli,
    }

    pub(super) fn bed() -> Bed {
        let tmp = tempfile::tempdir().unwrap();
        let platform = Arc::new(FakePlatform::new());
        let cli = CodexCli::new(
            platform.clone(),
            CodexConfig {
                codex_home: tmp.path().join("codex"),
                codex_bin: "codex".to_owned(),
            },
        );
        Bed { tmp, platform, cli }
    }

    pub(super) fn want() -> LeadCommand {
        LeadCommand {
            bin: PathBuf::from("/opt/bin/teamwire"),
            home: None,
        }
    }

    impl Bed {
        pub(super) fn home(&self) -> PathBuf {
            self.tmp.path().join("tw")
        }

        /// What `codex plugin list --json` prints next.
        pub(super) fn list(&self, installed: &[(&str, bool)]) {
            let installed: Vec<_> = installed
                .iter()
                .map(|(v, on)| serde_json::json!({"pluginId": "teamwire@teamwire", "version": v, "enabled": on}))
                .collect();
            self.reply(&serde_json::json!({"installed": installed, "available": []}).to_string());
        }

        pub(super) fn reply(&self, stdout: &str) {
            self.platform.queue_run(Ok(RunOutput {
                exit_code: 0,
                stdout: stdout.as_bytes().to_vec(),
                stderr: Vec::new(),
                timed_out: false,
            }));
        }

        pub(super) fn marketplace_from(&self, source_type: &str, source: &Path) {
            let dir = self.tmp.path().join("codex");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("config.toml"),
                format!(
                    "[marketplaces.teamwire]\nsource_type = \"{source_type}\"\nsource = \"{}\"\n",
                    source.display()
                ),
            )
            .unwrap();
        }

        /// The plugin files on disk, as install wrote them; returns their version.
        pub(super) fn write_files(&self) -> String {
            let files =
                plugin::PluginFiles::render(&plugin_dir(&self.home()), &want(), SHARED).unwrap();
            for (rel, text) in &files.files {
                let to = files.dir.join(rel);
                std::fs::create_dir_all(to.parent().unwrap()).unwrap();
                std::fs::write(to, text).unwrap();
            }
            files.version
        }

        pub(super) fn args_run(&self) -> Vec<Vec<String>> {
            self.platform
                .runs()
                .into_iter()
                .map(|(c, _)| c.args)
                .collect()
        }
    }
}
use bed::{Bed, SHARED, bed, want};

fn words(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| (*a).to_owned()).collect()
}

fn dir_of(b: &Bed) -> PathBuf {
    plugin_dir(&b.home())
}

#[test]
fn a_fresh_install_writes_the_files_then_adds_the_marketplace_and_the_plugin() {
    let b = bed();
    b.list(&[]);
    b.reply("");
    let changes = b
        .cli
        .plan_install(&want(), SHARED, &b.home(), false)
        .unwrap();
    assert!(matches!(&changes[0], Change::WritePlugin { dir, .. } if *dir == dir_of(&b)));
    assert_eq!(
        changes[1..],
        [Change::AddMarketplace(dir_of(&b)), Change::AddPlugin]
    );
    // Planning reads the list and probes `plugin`, and runs no change.
    assert_eq!(
        b.args_run(),
        [
            words(&["plugin", "list", "--json"]),
            words(&["plugin", "--help"])
        ]
    );
}

#[test]
fn an_installed_current_plugin_needs_no_change_and_no_probe() {
    let b = bed();
    let version = b.write_files();
    b.marketplace_from("local", &dir_of(&b));
    b.list(&[(&version, true)]);
    assert_eq!(
        b.cli
            .plan_install(&want(), SHARED, &b.home(), false)
            .unwrap(),
        []
    );
    assert_eq!(b.args_run().len(), 1);
}

#[test]
fn another_version_is_removed_and_added_again() {
    let b = bed();
    b.write_files();
    b.marketplace_from("local", &dir_of(&b));
    b.list(&[("old", true)]);
    b.reply("");
    assert_eq!(
        b.cli
            .plan_install(&want(), SHARED, &b.home(), false)
            .unwrap(),
        [Change::RemovePlugin, Change::AddPlugin]
    );
}

#[test]
fn a_disabled_plugin_is_added_again() {
    let b = bed();
    let version = b.write_files();
    b.marketplace_from("local", &dir_of(&b));
    b.list(&[(&version, false)]);
    b.reply("");
    assert_eq!(
        b.cli
            .plan_install(&want(), SHARED, &b.home(), false)
            .unwrap(),
        [Change::RemovePlugin, Change::AddPlugin]
    );
}

#[test]
fn a_marketplace_from_another_source_is_refused_unless_forced() {
    let b = bed();
    b.write_files();
    b.marketplace_from("git", Path::new("https://example.com/x.git"));
    b.list(&[]);
    let e = b
        .cli
        .plan_install(&want(), SHARED, &b.home(), false)
        .unwrap_err();
    assert!(e.to_string().contains("another source") && e.to_string().contains("--force"));
    b.list(&[]);
    b.reply("");
    assert_eq!(
        b.cli
            .plan_install(&want(), SHARED, &b.home(), true)
            .unwrap(),
        [
            Change::RemoveMarketplace,
            Change::AddMarketplace(dir_of(&b)),
            Change::AddPlugin
        ]
    );
}

#[test]
fn a_codex_without_the_plugin_command_is_refused_before_anything_is_written() {
    let b = bed();
    b.list(&[]);
    b.platform.queue_run(Ok(RunOutput {
        exit_code: 2,
        stdout: Vec::new(),
        stderr: b"unrecognized subcommand".to_vec(),
        timed_out: false,
    }));
    let e = b
        .cli
        .plan_install(&want(), SHARED, &b.home(), false)
        .unwrap_err();
    assert!(e.to_string().contains("cannot run `codex plugin`"), "{e}");
    assert!(b.platform.written().is_empty());
}

#[test]
fn uninstall_removes_the_plugin_the_marketplace_and_the_files() {
    let b = bed();
    b.write_files();
    b.marketplace_from("local", &dir_of(&b));
    b.list(&[("v", true)]);
    b.reply("");
    assert_eq!(
        b.cli.plan_uninstall(&b.home(), false).unwrap(),
        [
            Change::RemovePlugin,
            Change::RemoveMarketplace,
            Change::RemovePluginFiles(dir_of(&b))
        ]
    );
}

#[test]
fn uninstall_with_nothing_installed_plans_nothing() {
    let b = bed();
    b.list(&[]);
    assert_eq!(b.cli.plan_uninstall(&b.home(), false).unwrap(), []);
}

#[test]
fn apply_runs_the_codex_commands_in_order_and_stops_at_the_first_failure() {
    let b = bed();
    b.reply("");
    b.platform.queue_run(Ok(RunOutput {
        exit_code: 1,
        stdout: Vec::new(),
        stderr: b"no such plugin".to_vec(),
        timed_out: false,
    }));
    let changes = [
        Change::AddMarketplace(PathBuf::from("/h/codex-plugin")),
        Change::AddPlugin,
        Change::RemovePlugin,
    ];
    let e = b.cli.apply_lead_setup(&changes).unwrap_err();
    assert_eq!(
        e.done,
        [Change::AddMarketplace(PathBuf::from("/h/codex-plugin"))]
    );
    assert_eq!(e.failed, Change::AddPlugin);
    assert!(e.detail.contains("no such plugin"));
    assert_eq!(
        b.args_run(),
        [
            words(&["plugin", "marketplace", "add", "/h/codex-plugin"]),
            words(&["plugin", "add", "teamwire@teamwire"]),
        ]
    );
}

#[test]
fn apply_writes_the_files_through_the_platform() {
    let b = bed();
    b.list(&[]);
    b.reply("");
    let changes = b
        .cli
        .plan_install(&want(), SHARED, &b.home(), false)
        .unwrap();
    b.cli.apply_lead_setup(&changes[..1]).unwrap();
    assert_eq!(b.platform.written().len(), 7);
}

#[test]
fn the_status_is_missing_stale_or_current() {
    let b = bed();
    let status = |compare| {
        b.cli
            .plugin_status(&want(), SHARED, &b.home(), compare)
            .unwrap()
    };
    b.list(&[]);
    assert_eq!(status(true), LeadStatus::Missing);
    let version = b.write_files();
    b.marketplace_from("local", &dir_of(&b));
    b.list(&[(&version, true)]);
    assert_eq!(status(true), LeadStatus::Current);
    b.list(&[("old", true)]);
    assert!(matches!(status(true), LeadStatus::Stale(p) if p[0].contains("old")));
    // A cargo build is not compared by version.
    b.list(&[("old", true)]);
    assert_eq!(status(false), LeadStatus::Current);
    b.list(&[(&version, false)]);
    assert!(matches!(status(true), LeadStatus::Stale(p) if p[0].contains("disabled")));
}

#[test]
fn a_codex_that_cannot_run_is_a_refusal_that_names_it() {
    let b = bed();
    b.platform.queue_run(Err(PlatformError::unsupported()));
    let e = b.cli.plan_uninstall(&b.home(), false).unwrap_err();
    assert!(e.to_string().contains("codex plugin list --json"), "{e}");
}
