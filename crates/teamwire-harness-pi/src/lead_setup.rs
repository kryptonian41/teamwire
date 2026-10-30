//! The lead setup for pi (ADR 0030): teamwire's lead package, whose files
//! `teamwire install` writes under `$TEAMWIRE_HOME/pi-package/` and which it
//! registers with `pi install <dir>`. pi loads a local package from its own
//! path without copying it, so a changed file needs no second install, only
//! a restart of pi. teamwire never writes pi's directory; it reads
//! `settings.json` there to see whether the package is registered.
//!
//! As for Codex, `plan_*` reads and checks and changes nothing, and
//! `apply_lead_setup` makes the changes in order and stops at the first
//! failure, so a caller that only plans has a dry run.

use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};
use teamwire_core::Command;

use crate::Pi;
use crate::lead_extension::{self, LeadValues};

/// How long one `pi` command may take.
const PI_TIMEOUT: Duration = Duration::from_secs(30);

/// How a pi lead differs from the Claude Code lead the shared skill
/// describes. It goes between the frontmatter and the shared text.
const PI_PART: &str = include_str!("../../../examples/skills/pi-lead.md");
/// The second file of the skill. It names no Claude tool.
const REFERENCE_TEXT: &str = include_str!("../../../examples/skills/teamwire/reference.md");
/// The skill a lead loads when promptwire is installed. It names no Claude
/// tool.
const PROMPTWIRE_TEXT: &str = include_str!("../../../examples/skills/promptwire/SKILL.md");

/// The program the package runs: `<bin> [--home DIR]`, then `mcp --lead`,
/// the lead hook or `watch`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeadCommand {
    pub bin: PathBuf,
    /// Given only when the state directory is not the default, so a default
    /// install keeps following the default.
    pub home: Option<PathBuf>,
}

/// `<home>/pi-package`: the package's directory.
#[must_use]
pub fn package_dir(home: &Path) -> PathBuf {
    home.join("pi-package")
}

/// A check that failed before anything was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeadSetupError(pub String);

impl fmt::Display for LeadSetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for LeadSetupError {}

/// One change install or uninstall makes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Write the package's files: each path is relative to `dir`.
    WritePackage {
        dir: PathBuf,
        files: Vec<(PathBuf, String)>,
    },
    AddPackage(PathBuf),
    RemovePackage(PathBuf),
    /// Delete the package's directory.
    RemovePackageFiles(PathBuf),
}

impl Change {
    /// True for a change made by running `pi`.
    #[must_use]
    pub fn is_pi(&self) -> bool {
        matches!(self, Self::AddPackage(_) | Self::RemovePackage(_))
    }
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WritePackage { dir, .. } => {
                write!(f, "write the teamwire pi package to {}", dir.display())
            }
            Self::AddPackage(dir) => write!(f, "add the pi package {}", dir.display()),
            Self::RemovePackage(dir) => write!(f, "remove the pi package {}", dir.display()),
            Self::RemovePackageFiles(dir) => {
                write!(f, "delete the teamwire pi package at {}", dir.display())
            }
        }
    }
}

/// A change that failed after the checks passed.
#[derive(Debug)]
pub struct ApplyError {
    /// The changes made before the failure, in order.
    pub done: Vec<Change>,
    pub failed: Change,
    pub detail: String,
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} failed: {}", self.failed, self.detail)
    }
}

impl std::error::Error for ApplyError {}

/// What pi has of the package, compared with what this binary would write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageStatus {
    /// pi's settings do not list the package.
    Missing,
    /// Listed, but its files are not this binary's.
    Stale(Vec<String>),
    Current,
}

/// The skill: the shared text's frontmatter, the pi part, then the shared
/// body. `None` when the shared text has no frontmatter.
fn skill(shared: &str) -> Option<String> {
    let rest = shared.strip_prefix("---\n")?;
    let end = "---\n".len() + rest.find("\n---\n")? + "\n---\n".len();
    let (head, body) = shared.split_at(end);
    Some(format!("{head}\n{PI_PART}{body}"))
}

impl Pi {
    /// The package's files as this binary writes them, each path relative
    /// to `package_dir(home)`. Fails when the shared skill has no
    /// frontmatter.
    pub fn package_files(
        &self,
        want: &LeadCommand,
        shared_skill: &str,
    ) -> Result<Vec<(PathBuf, String)>, LeadSetupError> {
        let skill = skill(shared_skill)
            .ok_or_else(|| LeadSetupError("the compiled-in skill has no frontmatter".to_owned()))?;
        let home = want.home.as_ref().map(|h| {
            // Rebuilt from its components so `/x/` and `/x` give one text.
            h.components().collect::<PathBuf>().display().to_string()
        });
        let extension = lead_extension::render(&LeadValues {
            teamwire_bin: &want.bin.display().to_string(),
            home: home.as_deref(),
            state_dir: &self.config.sessions_dir().display().to_string(),
        });
        let manifest = json!({
            "name": "teamwire-lead",
            "private": true,
            "description": "teamwire's lead tools, skill, watch and Stop hook for pi",
            "keywords": ["pi-package"],
            "pi": {
                "extensions": ["./extensions/teamwire-lead.ts"],
                "skills": ["./skills"],
            },
            "peerDependencies": {
                "@earendil-works/pi-ai": "*",
                "@earendil-works/pi-coding-agent": "*",
            },
        });
        let manifest = serde_json::to_string_pretty(&manifest)
            .map_or_else(|_| manifest.to_string(), |t| t + "\n");
        Ok([
            ("package.json", manifest),
            ("extensions/teamwire-lead.ts", extension),
            ("skills/teamwire/SKILL.md", skill),
            ("skills/teamwire/reference.md", REFERENCE_TEXT.to_owned()),
            ("skills/promptwire/SKILL.md", PROMPTWIRE_TEXT.to_owned()),
        ]
        .into_iter()
        .map(|(path, text)| (PathBuf::from(path), text))
        .collect())
    }

    /// Whether pi's `settings.json` lists the package at `dir`. pi keeps a
    /// local package's path relative to the settings file's folder.
    fn registered(&self, dir: &Path) -> Result<bool, LeadSetupError> {
        let file = self.config.agent_dir.join("settings.json");
        let text = match std::fs::read_to_string(&file) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(read_error(&file, &e.to_string())),
        };
        let doc: Value =
            serde_json::from_str(&text).map_err(|e| read_error(&file, &e.to_string()))?;
        Ok(doc
            .get("packages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|p| p.as_str().or_else(|| p.get("source")?.as_str()))
            .any(|source| same_dir(&self.config.agent_dir.join(source), dir)))
    }

    /// What pi has of the package. `compare_files` is false for a cargo
    /// build, whose paths differ from the installed binary's by design.
    /// Reads only, so `doctor` calls it.
    pub fn package_status(
        &self,
        want: &LeadCommand,
        shared_skill: &str,
        home: &Path,
        compare_files: bool,
    ) -> Result<PackageStatus, LeadSetupError> {
        let dir = package_dir(home);
        if !self.registered(&dir)? {
            return Ok(PackageStatus::Missing);
        }
        let files = self.package_files(want, shared_skill)?;
        let problem = if compare_files && !is_written(&dir, &files) {
            Some(format!(
                "the package files at {} are not this binary's",
                dir.display()
            ))
        } else if !dir.join("package.json").is_file() {
            Some(format!(
                "the package files at {} are missing",
                dir.display()
            ))
        } else {
            None
        };
        Ok(problem.map_or(PackageStatus::Current, |p| PackageStatus::Stale(vec![p])))
    }

    /// The changes that make the package this binary's, after every check.
    pub fn plan_install(
        &self,
        want: &LeadCommand,
        shared_skill: &str,
        home: &Path,
    ) -> Result<Vec<Change>, LeadSetupError> {
        let dir = package_dir(home);
        let files = self.package_files(want, shared_skill)?;
        let mut changes = Vec::new();
        if !is_written(&dir, &files) {
            changes.push(Change::WritePackage {
                dir: dir.clone(),
                files,
            });
        }
        if !self.registered(&dir)? {
            changes.push(Change::AddPackage(dir));
        }
        self.check_pi_runs(&changes)?;
        Ok(changes)
    }

    /// The changes that remove the package, after every check.
    pub fn plan_uninstall(&self, home: &Path) -> Result<Vec<Change>, LeadSetupError> {
        let dir = package_dir(home);
        let mut changes = Vec::new();
        if self.registered(&dir)? {
            changes.push(Change::RemovePackage(dir.clone()));
        }
        if dir.exists() {
            changes.push(Change::RemovePackageFiles(dir));
        }
        self.check_pi_runs(&changes)?;
        Ok(changes)
    }

    /// Makes `changes` in order, stopping at the first failure.
    pub fn apply_lead_setup(&self, changes: &[Change]) -> Result<(), Box<ApplyError>> {
        let mut done = Vec::new();
        for change in changes {
            if let Err(detail) = self.apply_one(change) {
                return Err(Box::new(ApplyError {
                    done,
                    failed: change.clone(),
                    detail,
                }));
            }
            done.push(change.clone());
        }
        Ok(())
    }

    fn apply_one(&self, change: &Change) -> Result<(), String> {
        match change {
            Change::WritePackage { dir, files } => self.write_package(dir, files),
            Change::RemovePackageFiles(dir) => std::fs::remove_dir_all(dir)
                .map_err(|e| format!("cannot remove {}: {e}", dir.display())),
            Change::AddPackage(dir) => self.run_pi(&["install", &dir.display().to_string()]),
            Change::RemovePackage(dir) => self.run_pi(&["remove", &dir.display().to_string()]),
        }
    }

    /// Runs `pi --version` when a change needs `pi`, so a missing `pi` is
    /// refused before the first write.
    fn check_pi_runs(&self, changes: &[Change]) -> Result<(), LeadSetupError> {
        if !changes.iter().any(Change::is_pi) {
            return Ok(());
        }
        self.run_pi(&["--version"]).map_err(|detail| {
            LeadSetupError(format!(
                "cannot run `{}`, which registers the teamwire package: {detail}",
                self.config.pi_bin
            ))
        })
    }

    /// Runs `pi <args>` against this config's directory, with no network
    /// call of pi's own.
    fn run_pi(&self, args: &[&str]) -> Result<(), String> {
        let cmd = Command::new(self.config.pi_bin.clone())
            .args(args.iter().map(|a| (*a).to_owned()))
            .env(
                "PI_CODING_AGENT_DIR",
                self.config.agent_dir.display().to_string(),
            )
            .env("PI_OFFLINE", "1")
            .env("PI_SKIP_VERSION_CHECK", "1");
        let out = self
            .platform
            .run(&cmd, PI_TIMEOUT)
            .map_err(|e| e.to_string())?;
        if out.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        let said = if stderr.is_empty() {
            String::from_utf8_lossy(&out.stdout).trim().to_owned()
        } else {
            stderr
        };
        Err(if out.timed_out {
            format!("timed out after {}s", PI_TIMEOUT.as_secs())
        } else {
            format!("exit {}: {said}", out.exit_code)
        })
    }

    fn write_package(&self, dir: &Path, files: &[(PathBuf, String)]) -> Result<(), String> {
        for (rel, text) in files {
            let path = dir.join(rel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
            }
            self.platform
                .write_atomic(&path, text.as_bytes())
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

fn read_error(file: &Path, detail: &str) -> LeadSetupError {
    LeadSetupError(format!("cannot read {}: {detail}", file.display()))
}

/// The same folder, as the file system resolves both or, for a folder that
/// is gone, by its path with `.` and `..` worked out.
fn same_dir(a: &Path, b: &Path) -> bool {
    let lexical = |p: &Path| {
        let mut out = PathBuf::new();
        for part in p.components() {
            match part {
                Component::CurDir => {}
                Component::ParentDir => {
                    out.pop();
                }
                Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                    out.push(part);
                }
            }
        }
        out
    };
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => lexical(a) == lexical(b),
    }
}

/// True when every file is on disk at `dir` with this text.
fn is_written(dir: &Path, files: &[(PathBuf, String)]) -> bool {
    files
        .iter()
        .all(|(path, text)| std::fs::read_to_string(dir.join(path)).is_ok_and(|t| &t == text))
}
