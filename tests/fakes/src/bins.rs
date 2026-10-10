//! Finding and installing the fake binaries, and building a `PATH` on which
//! the real `herdr` and `claude` cannot be found.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The directories a test's `PATH` keeps after the fake binaries: enough for
/// `sh`, `git`, `cat` and `sleep`, and nowhere the real `herdr` or `claude`
/// lives.
const SYSTEM_DIRS: [&str; 4] = ["/usr/bin", "/bin", "/usr/sbin", "/sbin"];

/// The directory that holds the built `fake-herdr` and `fake-claude`.
#[derive(Debug, Clone)]
pub struct FakeBins {
    dir: PathBuf,
}

static BUILT: OnceLock<Result<PathBuf, String>> = OnceLock::new();

/// Set to `1` by a runner that built the whole workspace before any test
/// started, so the fake binaries beside the test executable are current.
/// Cargo replaces those files on every build, even one with nothing to
/// compile, so a build from one test process makes another test's start of
/// `fake-herdr` fail with "No such file or directory". A runner with one
/// process per test, such as nextest, starts hundreds of those builds at once.
const PREBUILT: &str = "TEAMWIRE_FAKES_PREBUILT";

impl FakeBins {
    /// Finds the fake binaries beside the running test executable, building
    /// them first (`cargo build -p teamwire-fakes --bins`) once per process,
    /// unless `TEAMWIRE_FAKES_PREBUILT=1` says they are built already.
    /// `cargo test -p teamwire-fakes` builds them already; another crate's
    /// tests need the build.
    pub fn ensure() -> Result<Self, String> {
        BUILT.get_or_init(build).clone().map(|dir| Self { dir })
    }

    pub fn herdr(&self) -> PathBuf {
        self.dir.join("fake-herdr")
    }

    pub fn claude(&self) -> PathBuf {
        self.dir.join("fake-claude")
    }

    /// Makes `into` a bin directory holding `herdr`, `claude`, `fake-herdr`
    /// and `fake-claude`, all pointing at the fake binaries, so a program that
    /// starts `herdr` or `claude` by name gets a fake.
    pub fn install(&self, into: &Path) -> std::io::Result<FakeBinDir> {
        std::fs::create_dir_all(into)?;
        for (link, target) in [
            ("herdr", self.herdr()),
            ("fake-herdr", self.herdr()),
            ("claude", self.claude()),
            ("fake-claude", self.claude()),
        ] {
            let path = into.join(link);
            if path.symlink_metadata().is_err() {
                std::os::unix::fs::symlink(&target, &path)?;
            }
        }
        FakeBinDir::new(into.to_path_buf())
    }
}

fn build() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // <target>/<profile>/deps/<test binary>
    let profile_dir = exe
        .parent()
        .and_then(Path::parent)
        .ok_or("cannot find the target directory from the test executable")?
        .to_path_buf();
    if std::env::var_os(PREBUILT).is_none_or(|v| v != "1") {
        cargo_build(&profile_dir)?;
    }
    for name in ["fake-herdr", "fake-claude"] {
        if !profile_dir.join(name).is_file() {
            return Err(format!("{name} is not in {}", profile_dir.display()));
        }
    }
    Ok(profile_dir)
}

/// `cargo build` of this crate's binaries into the target directory that
/// holds `profile_dir`.
fn cargo_build(profile_dir: &Path) -> Result<(), String> {
    let target_dir = profile_dir
        .parent()
        .ok_or("cannot find the target directory")?
        .to_path_buf();
    let release = profile_dir.file_name().is_some_and(|n| n == "release");
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
    let mut args = vec![
        "build".to_owned(),
        "--locked".to_owned(),
        "--bins".to_owned(),
        "--manifest-path".to_owned(),
        manifest.to_owned(),
        "--target-dir".to_owned(),
        target_dir.display().to_string(),
    ];
    if release {
        args.push("--release".to_owned());
    }
    let out = cargo(&args)?;
    if !out.status.success() {
        return Err(format!(
            "building the fake binaries failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

#[expect(
    clippy::disallowed_methods,
    reason = "building the fake binaries needs cargo; this crate may start processes"
)]
fn cargo(args: &[String]) -> Result<std::process::Output, String> {
    let program = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("cannot run cargo: {e}"))
}

/// A bin directory with the fakes installed, and the `PATH` built from it.
#[derive(Debug, Clone)]
pub struct FakeBinDir {
    dir: PathBuf,
    path: OsString,
}

impl FakeBinDir {
    fn new(dir: PathBuf) -> std::io::Result<Self> {
        let system: Vec<PathBuf> = SYSTEM_DIRS.iter().map(PathBuf::from).collect();
        for d in &system {
            for real in ["herdr", "claude"] {
                if d.join(real).exists() {
                    return Err(std::io::Error::other(format!(
                        "the real {real} is on the test PATH in {}",
                        d.display()
                    )));
                }
            }
        }
        let path = std::env::join_paths(std::iter::once(&dir).chain(&system))
            .map_err(std::io::Error::other)?;
        Ok(Self { dir, path })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The value for `PATH`: the fake directory first, then system
    /// directories that hold no real `herdr` or `claude`.
    pub fn path(&self) -> &OsString {
        &self.path
    }
}
