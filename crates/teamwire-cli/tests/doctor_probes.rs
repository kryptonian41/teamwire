//! `teamwire doctor` run as the real binary on a `PATH` of fake `claude` and
//! `herdr`: the probes of the harness and the host read their answers.
#![expect(
    clippy::disallowed_methods,
    reason = "the test starts the teamwire binary"
)]
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

use teamwire_fakes::{FakeBins, TestHome};

fn doctor(extra_env: &[(&str, &str)]) -> String {
    let bins = FakeBins::ensure().unwrap();
    let bin = TestHome::new().unwrap();
    bins.install(bin.path()).unwrap();
    let home = TestHome::new().unwrap();
    let claude = TestHome::new().unwrap();
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"));
    cmd.args([
        "--home".as_ref(),
        home.path().as_os_str(),
        "doctor".as_ref(),
    ])
    .env("PATH", bin.path())
    .env("HERDR_ENV", "1")
    .env_remove("TMUX")
    .env("TEAMWIRE_HOME", home.path())
    .env("TEAMWIRE_CLAUDE_DIR", claude.path())
    .env("FAKE_CLAUDE_DIR", claude.path());
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    String::from_utf8(cmd.output().unwrap().stdout).unwrap()
}

#[test]
fn versions_and_flags_come_from_the_binaries_on_path() {
    let out = doctor(&[]);
    assert!(
        out.contains("ok   claude version: 0.0.0 (fake-claude)"),
        "{out}"
    );
    assert!(out.contains("ok   claude flags:"), "{out}");
    assert!(
        out.contains("ok   herdr version: herdr 0.0.0 (fake-herdr)"),
        "{out}"
    );
}

#[test]
fn a_claude_without_a_launch_flag_is_warned_with_the_flag_named() {
    let out = doctor(&[("FAKE_CLAUDE_HELP_WITHOUT", "--effort")]);
    let line = out.lines().find(|l| l.contains("claude flags")).unwrap();
    assert!(line.starts_with("warn"), "{out}");
    assert!(line.contains("--effort"), "{line}");
}

#[test]
fn a_credential_variable_is_named_and_its_value_is_never_printed() {
    let out = doctor(&[("ANTHROPIC_API_KEY", "sk-secret-value")]);
    let line = out
        .lines()
        .find(|l| l.contains("credential variables"))
        .unwrap();
    assert!(
        line.starts_with("warn") && line.contains("ANTHROPIC_API_KEY"),
        "{line}"
    );
    assert!(!out.contains("sk-secret-value"), "{out}");
}
