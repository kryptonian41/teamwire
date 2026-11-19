//! Finding and installing the fake binaries, finding the real `teamwire`
//! binary for tests that run it from outside `teamwire-cli`, and building a
//! `PATH` on which the real hosts and harnesses cannot be found.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The directories a test's `PATH` keeps after the fake binaries: enough for
/// `sh`, `git`, `cat` and `sleep`. One that holds a real program of
/// `REAL_BINS` is replaced by a shadow of it (`shadow`).
const SYSTEM_DIRS: [&str; 4] = ["/usr/bin", "/bin", "/usr/sbin", "/sbin"];

/// The real programs a test must never reach by name: the hosts, which
/// type into the user's panes, and the harnesses, which start real sessions.
/// A Linux package puts `tmux` and `herdr` in /usr/bin; macOS has none there.
const REAL_BINS: [&str; 3] = ["herdr", "claude", "tmux"];

/// The directory that holds the built `fake-herdr` and `fake-claude`.
#[derive(Debug, Clone)]
pub struct FakeBins {
    dir: PathBuf,
}

static BUILT: OnceLock<Result<PathBuf, String>> = OnceLock::new();
static CLI: OnceLock<Result<PathBuf, String>> = OnceLock::new();

/// Set to `1` by a runner that built the whole workspace before any test
/// started, so the fake binaries and `teamwire` beside the test executable
/// are current. Cargo replaces those files on every build, even one with
/// nothing to compile, so a build from one test process makes another test's
/// start of `fake-herdr` fail with "No such file or directory". A runner with
/// one process per test, such as nextest, starts hundreds of those builds at
/// once. A binary older than its sources is refused (`check_current`), so a
/// run that sets the variable without building first fails instead of
/// testing old code.
const PREBUILT: &str = "TEAMWIRE_FAKES_PREBUILT";

/// The workspace manifest, for building `teamwire-cli`.
const WORKSPACE_MANIFEST: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml");

/// The real `teamwire` binary, for tests that run it as a child process.
/// Cargo sets `CARGO_BIN_EXE_teamwire` only for `teamwire-cli`'s own tests,
/// and the end-to-end and crash tests do not depend on that crate, so the
/// binary is built here once per process (`cargo build -p teamwire-cli --bin
/// teamwire`), into the same directory as the fakes. With
/// `TEAMWIRE_FAKES_PREBUILT=1` the build is skipped and the binary already
/// there is used, once `check_current` has found it current.
pub fn teamwire_cli() -> Result<&'static Path, String> {
    CLI.get_or_init(build_cli).as_deref().map_err(String::clone)
}

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
    let profile_dir = profile_dir()?;
    let prebuilt = prebuilt();
    if !prebuilt {
        let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        cargo_build(
            &profile_dir,
            &["--bins", "--manifest-path", manifest],
            "the fake binaries",
        )?;
    }
    for name in ["fake-herdr", "fake-claude"] {
        let bin = profile_dir.join(name);
        if !bin.is_file() {
            return Err(format!("{name} is not in {}", profile_dir.display()));
        }
        if prebuilt {
            check_current(&bin)?;
        }
    }
    Ok(profile_dir)
}

fn build_cli() -> Result<PathBuf, String> {
    let profile_dir = profile_dir()?;
    let bin = profile_dir.join("teamwire");
    if prebuilt() {
        if !bin.is_file() {
            return Err(format!(
                "{} is missing; run `cargo build --workspace` first",
                bin.display()
            ));
        }
        check_current(&bin)?;
        return Ok(bin);
    }
    cargo_build(
        &profile_dir,
        &[
            "-p",
            "teamwire-cli",
            "--bin",
            "teamwire",
            "--manifest-path",
            WORKSPACE_MANIFEST,
        ],
        "teamwire",
    )?;
    if bin.is_file() {
        Ok(bin)
    } else {
        Err(format!("{} was not built", bin.display()))
    }
}

fn prebuilt() -> bool {
    std::env::var_os(PREBUILT).is_some_and(|v| v == "1")
}

/// `<target>/<profile>/`, from the running test executable at
/// `<target>/<profile>/deps/<test binary>`.
fn profile_dir() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    exe.parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .ok_or_else(|| "cannot find the target directory from the test executable".to_owned())
}

/// Fails when a source file `bin` was built from is newer than `bin`, or is
/// gone. The list is the dep-info file cargo writes beside the binary
/// (`<bin>.d`): exactly the files compiled into it. A test file is not in
/// it, and cargo does not relink the binary when only a test file changes,
/// so a test edit never makes a fresh binary look old. A changed
/// `Cargo.toml` or `Cargo.lock` is not in the list either.
fn check_current(bin: &Path) -> Result<(), String> {
    let built = modified(bin).map_err(|e| format!("cannot read {}: {e}", bin.display()))?;
    let dep_info = bin.with_extension("d");
    let text = std::fs::read_to_string(&dep_info).map_err(|e| {
        format!(
            "cannot check that {} is current: {}: {e}; unset {PREBUILT} to build it",
            bin.display(),
            dep_info.display()
        )
    })?;
    for source in dep_info_sources(&text) {
        let why = match modified(Path::new(&source)) {
            Ok(changed) if changed <= built => continue,
            Ok(_) => format!("{source} changed after it was built"),
            Err(_) => format!("{source} is gone"),
        };
        return Err(format!(
            "{} is out of date: {why}. Run `cargo build --workspace`, or unset {PREBUILT}",
            bin.display()
        ));
    }
    Ok(())
}

fn modified(path: &Path) -> std::io::Result<std::time::SystemTime> {
    path.metadata()?.modified()
}

/// The prerequisites in a make-style dep-info file: what follows `: ` on
/// each `target: prerequisite ...` line, split at spaces that are not
/// escaped with `\`.
fn dep_info_sources(text: &str) -> Vec<String> {
    let mut sources = Vec::new();
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let Some((_, prerequisites)) = line.split_once(": ") else {
            continue;
        };
        let mut current = String::new();
        let mut chars = prerequisites.chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => current.extend(chars.next()),
                ' ' => {
                    if !current.is_empty() {
                        sources.push(std::mem::take(&mut current));
                    }
                }
                _ => current.push(c),
            }
        }
        if !current.is_empty() {
            sources.push(current);
        }
    }
    sources
}

/// `cargo build --locked <what>` into the target directory that holds
/// `profile_dir`, in the same profile.
fn cargo_build(profile_dir: &Path, what: &[&str], label: &str) -> Result<(), String> {
    let target_dir = profile_dir
        .parent()
        .ok_or("cannot find the target directory")?
        .to_path_buf();
    let release = profile_dir.file_name().is_some_and(|n| n == "release");
    let mut args = vec!["build".to_owned(), "--locked".to_owned()];
    args.extend(what.iter().map(|a| (*a).to_owned()));
    args.push("--target-dir".to_owned());
    args.push(target_dir.display().to_string());
    if release {
        args.push("--release".to_owned());
    }
    let out = cargo(&args)?;
    if !out.status.success() {
        return Err(format!(
            "building {label} failed: {}",
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
        let mut system: Vec<PathBuf> = Vec::new();
        let mut seen: Vec<PathBuf> = Vec::new();
        for (i, d) in SYSTEM_DIRS.iter().map(Path::new).enumerate() {
            // On a merged-/usr system such as Arch, /bin, /sbin and /usr/sbin
            // all lead to /usr/bin, and one shadow of it is enough.
            let real = d.canonicalize().unwrap_or_else(|_| d.to_path_buf());
            if seen.contains(&real) {
                continue;
            }
            seen.push(real);
            if REAL_BINS.iter().any(|b| d.join(b).exists()) {
                system.push(shadow(d, &dir.join(format!(".system{i}")))?);
            } else {
                system.push(d.to_path_buf());
            }
        }
        for d in &system {
            for real in REAL_BINS {
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
    /// directories that hold no program of `REAL_BINS`.
    pub fn path(&self) -> &OsString {
        &self.path
    }
}

/// Makes `into` a directory with a link to every entry of `system` except the
/// programs of `REAL_BINS`. A Linux package installs `herdr` and `tmux` into
/// /usr/bin, beside the `sh` and `git` the tests need, so that directory
/// cannot go on the test `PATH` as it is.
fn shadow(system: &Path, into: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(into)?;
    for entry in std::fs::read_dir(system)? {
        let name = entry?.file_name();
        if REAL_BINS.iter().any(|b| name == *b) {
            continue;
        }
        let link = into.join(&name);
        if link.symlink_metadata().is_err() {
            std::os::unix::fs::symlink(system.join(&name), &link)?;
        }
    }
    Ok(into.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::time::{Duration, SystemTime};

    use super::*;

    #[test]
    fn dep_info_sources_split_at_unescaped_spaces() {
        let text = "/t/teamwire: /w/a.rs /w/my\\ dir/b.rs\n\n/w/a.rs:\n# env-dep:X=1\n";
        assert_eq!(dep_info_sources(text), ["/w/a.rs", "/w/my dir/b.rs"]);
    }

    /// A binary at `bin_time` built from one source at `source_time`, with
    /// the dep-info file cargo would write.
    fn built(dir: &Path, source_time: SystemTime, bin_time: SystemTime) -> (PathBuf, PathBuf) {
        let source = dir.join("a.rs");
        let bin = dir.join("teamwire");
        crate::fsutil::write_atomic(&source, b"fn main() {}\n").unwrap();
        crate::fsutil::write_atomic(&bin, b"binary").unwrap();
        crate::fsutil::write_atomic(
            &dir.join("teamwire.d"),
            format!("{}: {}\n", bin.display(), source.display()).as_bytes(),
        )
        .unwrap();
        #[expect(
            clippy::disallowed_methods,
            reason = "the test sets the file's mtime directly to build a stale or fresh binary; it writes no content"
        )]
        File::options()
            .write(true)
            .open(&source)
            .unwrap()
            .set_modified(source_time)
            .unwrap();
        #[expect(
            clippy::disallowed_methods,
            reason = "the test sets the file's mtime directly to build a stale or fresh binary; it writes no content"
        )]
        File::options()
            .write(true)
            .open(&bin)
            .unwrap()
            .set_modified(bin_time)
            .unwrap();
        (bin, source)
    }

    #[test]
    fn a_binary_newer_than_its_sources_is_current() {
        let dir = tempfile::tempdir().unwrap();
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_790_000_000);
        let (bin, _) = built(dir.path(), t, t + Duration::from_secs(1));
        assert_eq!(check_current(&bin), Ok(()));
    }

    #[test]
    fn a_source_changed_after_the_build_makes_the_binary_out_of_date() {
        let dir = tempfile::tempdir().unwrap();
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_790_000_000);
        let (bin, source) = built(dir.path(), t + Duration::from_secs(1), t);
        let err = check_current(&bin).unwrap_err();
        assert!(err.contains("is out of date"), "{err}");
        assert!(err.contains(&source.display().to_string()), "{err}");
        assert!(err.contains("cargo build --workspace"), "{err}");
    }

    #[test]
    fn a_deleted_source_or_a_missing_dep_info_file_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_790_000_000);
        let (bin, source) = built(dir.path(), t, t + Duration::from_secs(1));
        std::fs::remove_file(&source).unwrap();
        let err = check_current(&bin).unwrap_err();
        assert!(err.contains("is gone"), "{err}");
        std::fs::remove_file(dir.path().join("teamwire.d")).unwrap();
        let err = check_current(&bin).unwrap_err();
        assert!(err.contains("cannot check"), "{err}");
    }
}
