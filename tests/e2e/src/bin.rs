//! Building the `teamwire` binary once per test process. Cargo sets
//! `CARGO_BIN_EXE_teamwire` only for a package that owns the binary, and this
//! crate does not depend on `teamwire-cli`, so the build is run here, next to
//! the fakes (as `teamwire_fakes::FakeBins` does for them).
//!
//! With `TEAMWIRE_FAKES_PREBUILT=1` the build is skipped and the binary beside
//! the fakes is used as it is. `cargo test --workspace` builds
//! `target/<profile>/teamwire` before any test starts, because the
//! `teamwire-cli` integration tests need it. The risk: someone who sets the
//! variable for a `cargo test -p teamwire-e2e` run without building the
//! workspace first tests whatever binary is already there, which may be out
//! of date. Run `cargo build --workspace` first. The crash rig has a copy of
//! this logic in `tests/crash/tests/support/rig.rs`; keep the two in step.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::OnceLock;

use teamwire_fakes::FakeBins;

/// Same variable as `teamwire_fakes::FakeBins` reads.
const PREBUILT: &str = "TEAMWIRE_FAKES_PREBUILT";

static BUILT: OnceLock<Result<PathBuf, String>> = OnceLock::new();

/// The path of the built `teamwire` binary.
pub(crate) fn teamwire() -> PathBuf {
    BUILT
        .get_or_init(build)
        .clone()
        .unwrap_or_else(|e| panic!("cannot build the teamwire binary: {e}"))
}

fn build() -> Result<PathBuf, String> {
    // The fakes are built into <target>/<profile>/, and so is the binary.
    let fake = FakeBins::ensure()?.herdr();
    let profile_dir = fake
        .parent()
        .ok_or("the fake binaries have no directory")?
        .to_path_buf();
    let target_dir = profile_dir
        .parent()
        .ok_or("cannot find the target directory")?
        .to_path_buf();
    if std::env::var_os(PREBUILT).is_some_and(|v| v == "1") {
        let bin = profile_dir.join("teamwire");
        return if bin.is_file() {
            Ok(bin)
        } else {
            Err(format!(
                "{} is missing; run `cargo build --workspace` first",
                bin.display()
            ))
        };
    }
    let release = profile_dir.file_name().is_some_and(|n| n == "release");
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml");
    let mut args = vec![
        "build".to_owned(),
        "--locked".to_owned(),
        "-p".to_owned(),
        "teamwire-cli".to_owned(),
        "--bin".to_owned(),
        "teamwire".to_owned(),
        "--manifest-path".to_owned(),
        manifest.to_owned(),
        "--target-dir".to_owned(),
        target_dir.display().to_string(),
    ];
    if release {
        args.push("--release".to_owned());
    }
    let program = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let out = std::process::Command::new(program)
        .args(&args)
        .output()
        .map_err(|e| format!("cannot run cargo: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    let bin = profile_dir.join("teamwire");
    if bin.is_file() {
        Ok(bin)
    } else {
        Err(format!("{} was not built", bin.display()))
    }
}
