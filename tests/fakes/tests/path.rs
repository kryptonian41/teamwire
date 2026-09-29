//! The `PATH` a test builds reaches the fake `herdr` and `claude` and never
//! the real ones.

#![expect(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    reason = "test helpers build fixtures from literals; one test asks sh(1) which program a name resolves to"
)]

use teamwire_fakes::FakeBins;

fn resolve(path: &std::ffi::OsStr, name: &str) -> String {
    let out = std::process::Command::new("/bin/sh")
        .args(["-c", &format!("command -v {name}")])
        .env_clear()
        .env("PATH", path)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

#[test]
fn herdr_and_claude_resolve_to_the_fakes_inside_the_test_directory() {
    let dir = tempfile::tempdir().unwrap();
    let bin = FakeBins::ensure()
        .unwrap()
        .install(&dir.path().join("bin"))
        .unwrap();
    for name in ["herdr", "claude", "fake-herdr", "fake-claude"] {
        let found = resolve(bin.path(), name);
        assert!(
            found.starts_with(&dir.path().join("bin").display().to_string()),
            "{name} resolved to {found}"
        );
    }
    assert!(!resolve(bin.path(), "sh").is_empty());
}

#[test]
fn the_test_path_holds_no_directory_with_a_real_herdr_or_claude() {
    let dir = tempfile::tempdir().unwrap();
    let bin = FakeBins::ensure()
        .unwrap()
        .install(&dir.path().join("bin"))
        .unwrap();
    for d in std::env::split_paths(bin.path()).skip(1) {
        assert!(!d.join("herdr").exists(), "{}", d.display());
        assert!(!d.join("claude").exists(), "{}", d.display());
    }
}

#[test]
fn installing_twice_into_one_directory_is_harmless() {
    let dir = tempfile::tempdir().unwrap();
    let bins = FakeBins::ensure().unwrap();
    bins.install(&dir.path().join("bin")).unwrap();
    bins.install(&dir.path().join("bin")).unwrap();
}
