//! `teamwire --version` prints the same build identity as `teamwire version`.
#![expect(
    clippy::disallowed_methods,
    reason = "the test starts the teamwire binary as a user would"
)]
#![expect(
    clippy::unwrap_used,
    reason = "a failure to run the binary fails the test"
)]

use std::process::Command;

#[test]
fn the_flag_prints_the_writer_version_with_the_commit() {
    let out = Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        format!("teamwire {}\n", teamwire_store::WRITER_VERSION)
    );
}
