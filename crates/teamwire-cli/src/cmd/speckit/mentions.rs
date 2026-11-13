//! The tracked files that mention a task path (ADR 0044 decisions 4 to 6):
//! by the exact path, a dotted module name of at least two parts, the full
//! file name, or a workflow's `on.<event>.paths` glob. Never a bare stem.

use std::path::Path;
use std::time::Duration;

use teamwire_core::{Command, Platform};
use teamwire_verify::Owns;

use crate::cmd::doctor_speckit_files::{Yaml, parse_yaml};

const GIT_TIMEOUT: Duration = Duration::from_secs(30);
/// A file larger than this is not read for mentions.
const MAX_TEXT_BYTES: u64 = 2 * 1024 * 1024;
const WORKFLOWS: &str = ".github/workflows/";

/// A tracked file and its text, or `None` when it is not UTF-8 text or is
/// too large to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepoFile {
    pub(crate) path: String,
    pub(crate) text: Option<String>,
}

/// A file a draft holds, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Found {
    pub(crate) path: String,
    pub(crate) reason: String,
}

/// A workflow the YAML reader refused, and its reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Unread {
    pub(crate) path: String,
    pub(crate) why: String,
}

/// The repository's files, ready to be searched.
pub(crate) struct Mentions {
    files: Vec<RepoFile>,
    /// Each readable workflow with its `paths` globs.
    workflows: Vec<(String, Vec<String>)>,
    pub(crate) unread: Vec<Unread>,
}

/// The files git tracks under `root`, from `git ls-files -z`.
pub(crate) fn tracked_files(platform: &dyn Platform, root: &Path) -> Result<Vec<String>, String> {
    let cmd = Command::new("git")
        .args(["ls-files", "-z"])
        .cwd(root.to_path_buf());
    let out = platform
        .run(&cmd, GIT_TIMEOUT)
        .map_err(|e| format!("could not run git ls-files: {e}"))?;
    if out.timed_out {
        return Err(format!(
            "git ls-files gave no answer within {} s",
            GIT_TIMEOUT.as_secs()
        ));
    }
    if out.exit_code != 0 {
        let said = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "git ls-files failed in {}: {}",
            root.display(),
            said.lines().next().unwrap_or("").trim()
        ));
    }
    Ok(out
        .stdout
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| String::from_utf8_lossy(p).into_owned())
        .collect())
}

/// Reads each of `paths` under `root`. A file that cannot be read, is too
/// large or is not text has no text.
pub(crate) fn read_files(root: &Path, paths: &[String]) -> Vec<RepoFile> {
    paths
        .iter()
        .map(|p| {
            let at = root.join(p);
            let small =
                std::fs::metadata(&at).is_ok_and(|m| m.is_file() && m.len() <= MAX_TEXT_BYTES);
            let text = small
                .then(|| std::fs::read(&at).ok())
                .flatten()
                .and_then(|b| String::from_utf8(b).ok());
            RepoFile {
                path: p.clone(),
                text,
            }
        })
        .collect()
}

impl Mentions {
    /// `files` are the files to search, the feature folder already left
    /// out. Each workflow among them is read for its `paths` globs.
    pub(crate) fn new(files: Vec<RepoFile>) -> Self {
        let mut workflows = Vec::new();
        let mut unread = Vec::new();
        for f in files.iter().filter(|f| is_workflow(&f.path)) {
            let Some(text) = &f.text else {
                unread.push(Unread {
                    path: f.path.clone(),
                    why: "not readable as text".to_owned(),
                });
                continue;
            };
            match parse_yaml(text) {
                Ok(doc) => workflows.push((f.path.clone(), path_globs(&doc))),
                Err(why) => unread.push(Unread {
                    path: f.path.clone(),
                    why,
                }),
            }
        }
        Self {
            files,
            workflows,
            unread,
        }
    }

    /// The files that mention any of `paths`, other than `paths`
    /// themselves, each once with its first reason.
    pub(crate) fn of(&self, paths: &[String]) -> Vec<Found> {
        let searched: Vec<&String> = paths
            .iter()
            .filter(|p| !p.contains('*') && !p.ends_with('/'))
            .collect();
        let mut out = Vec::new();
        for file in &self.files {
            if paths.contains(&file.path) {
                continue;
            }
            let by_text = file
                .text
                .as_deref()
                .and_then(|text| searched.iter().find_map(|p| text_reason(text, p)));
            let reason = by_text.or_else(|| self.glob_reason(&file.path, &searched));
            if let Some(reason) = reason {
                out.push(Found {
                    path: file.path.clone(),
                    reason,
                });
            }
        }
        out
    }

    fn glob_reason(&self, workflow: &str, paths: &[&String]) -> Option<String> {
        let (_, globs) = self.workflows.iter().find(|(w, _)| w == workflow)?;
        globs.iter().find_map(|g| {
            let owns = Owns::new(std::slice::from_ref(g));
            paths
                .iter()
                .any(|p| owns.matches(p))
                .then(|| format!("CI paths glob {g}"))
        })
    }
}

fn is_workflow(path: &str) -> bool {
    path.strip_prefix(WORKFLOWS).is_some_and(|name| {
        let ext = Path::new(name).extension().and_then(|e| e.to_str());
        !name.contains('/')
            && ext.is_some_and(|e| e.eq_ignore_ascii_case("yml") || e.eq_ignore_ascii_case("yaml"))
    })
}

/// Every `on.<event>.paths` entry of a workflow.
fn path_globs(doc: &Yaml) -> Vec<String> {
    let Some(Yaml::Map(events)) = doc.get("on") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (_, event) in events {
        match event.get("paths") {
            Some(Yaml::List(items)) => {
                for item in items {
                    if let Yaml::Scalar(text, _) = item {
                        out.push(text.clone());
                    }
                }
            }
            Some(Yaml::Scalar(text, _)) if !text.is_empty() => out.push(text.clone()),
            Some(_) | None => {}
        }
    }
    out
}

/// Why `text` mentions `path`: its exact text, a dotted module name, or its
/// full file name, each as a whole token.
fn text_reason(text: &str, path: &str) -> Option<String> {
    if has_token(text, path) {
        return Some(format!("names path {path}"));
    }
    if let Some(module) = dotted_names(path).into_iter().find(|m| has_token(text, m)) {
        return Some(format!("names module {module}"));
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    let has_ext = name
        .rsplit_once('.')
        .is_some_and(|(stem, ext)| !stem.is_empty() && !ext.is_empty());
    (has_ext && name != path && has_token(text, name)).then(|| format!("names file {name}"))
}

/// The dotted module names of `path`: every suffix of its parts, without
/// the extension, of at least two parts, each part a plain identifier.
/// `apps/ai/harness/tools.py` gives `harness.tools`, `ai.harness.tools` and
/// `apps.ai.harness.tools`. A single part, the bare stem, is never one.
fn dotted_names(path: &str) -> Vec<String> {
    let stem = match path.rsplit_once('.') {
        Some((stem, ext)) if !ext.contains('/') => stem,
        _ => path,
    };
    let parts: Vec<&str> = stem.split('/').collect();
    let plain =
        |p: &&str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    let mut out = Vec::new();
    for start in (0..parts.len().saturating_sub(1)).rev() {
        let tail = parts.get(start..).unwrap_or_default();
        if !tail.iter().all(plain) {
            break;
        }
        out.push(tail.join("."));
    }
    out
}

/// A character that continues a name or a path, so a match next to it is
/// part of a longer word.
fn joins(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '/' | '.')
}

/// True when `needle` occurs in `text` as a whole token. A `.` after it ends
/// a sentence, so it counts as a break unless a name character follows.
fn has_token(text: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    text.match_indices(needle).any(|(at, _)| {
        let before = text.get(..at).and_then(|s| s.chars().next_back());
        let rest = text.get(at + needle.len()..).unwrap_or("");
        let mut after = rest.chars();
        let next = after.next();
        let free_before = before.is_none_or(|c| !joins(c));
        let free_after = match next {
            None => true,
            Some('.') => after.next().is_none_or(|c| !c.is_ascii_alphanumeric()),
            Some(c) => !joins(c),
        };
        free_before && free_after
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, text: &str) -> RepoFile {
        RepoFile {
            path: path.to_owned(),
            text: Some(text.to_owned()),
        }
    }

    fn found(m: &Mentions, paths: &[&str]) -> Vec<(String, String)> {
        let paths: Vec<String> = paths.iter().map(|p| (*p).to_owned()).collect();
        m.of(&paths)
            .into_iter()
            .map(|f| (f.path, f.reason))
            .collect()
    }

    fn pair(path: &str, reason: &str) -> (String, String) {
        (path.to_owned(), reason.to_owned())
    }

    #[test]
    fn the_exact_path_a_dotted_module_and_the_full_file_name_are_mentions() {
        let m = Mentions::new(vec![
            file("apps/ai/harness/tools.py", "def tool(): pass\n"),
            file("docs/map.md", "See `apps/ai/harness/tools.py`.\n"),
            file("apps/ai/.importlinter", "layers = harness.tools\n"),
            file("apps/ai/tests/t.py", "TARGETS = [\"tools.py\"]\n"),
            file("apps/ai/notes.txt", "the tools module, see tools\n"),
            file(
                "apps/other/tools.pyc.md",
                "other/tools.py and harness.toolsmith\n",
            ),
        ]);
        assert_eq!(
            found(&m, &["apps/ai/harness/tools.py"]),
            [
                pair("docs/map.md", "names path apps/ai/harness/tools.py"),
                pair("apps/ai/.importlinter", "names module harness.tools"),
                pair("apps/ai/tests/t.py", "names file tools.py"),
            ]
        );
    }

    /// FR-014: the exact path counts after a `./` or any `/`, and the file
    /// name counts after a `/` when the words before it are the end of the
    /// task path. `other/tools.py` names some other `tools.py`.
    #[test]
    fn a_path_or_file_name_after_a_slash_prefix_is_a_mention() {
        let m = Mentions::new(vec![
            file("docs/a.md", "run ./apps/ai/harness/tools.py\n"),
            file("docs/b.md", "cp $ROOT/apps/ai/harness/tools.py .\n"),
            file("docs/c.md", "see harness/tools.py\n"),
            file("docs/d.md", "see `ai/harness/tools.py`\n"),
            file("docs/e.md", "see other/tools.py\n"),
        ]);
        assert_eq!(
            found(&m, &["apps/ai/harness/tools.py"]),
            [
                pair("docs/a.md", "names path apps/ai/harness/tools.py"),
                pair("docs/b.md", "names path apps/ai/harness/tools.py"),
                pair("docs/c.md", "names file tools.py"),
                pair("docs/d.md", "names file tools.py"),
            ]
        );
    }

    #[test]
    fn a_workflow_paths_glob_is_a_mention_and_an_anchor_is_unread() {
        let replay = "on:\n  push:\n    paths:\n      - \"apps/ai/harness/**\"\n  pull_request:\n    paths: [\"docs/**\"]\n";
        let odd = "on:\n  push:\n    paths: &p\n      - \"src/**\"\n";
        let m = Mentions::new(vec![
            file(".github/workflows/replay.yml", replay),
            file(".github/workflows/odd.yaml", odd),
            file(".github/workflows/nested/skip.yml", odd),
        ]);
        assert_eq!(
            found(&m, &["apps/ai/harness/tools.py"]),
            [pair(
                ".github/workflows/replay.yml",
                "CI paths glob apps/ai/harness/**"
            )]
        );
        assert_eq!(
            found(&m, &["docs/a/b.md"]),
            [pair(
                ".github/workflows/replay.yml",
                "CI paths glob docs/**"
            )]
        );
        assert_eq!(
            m.unread.iter().map(|u| u.path.as_str()).collect::<Vec<_>>(),
            [".github/workflows/odd.yaml"]
        );
    }

    /// Guard: passes on a stub that finds nothing.
    #[test]
    fn a_bare_stem_a_task_path_itself_and_a_glob_are_never_mentions() {
        let m = Mentions::new(vec![
            file("src/tools.rs", "mod tools;\n"),
            file("src/lib.rs", "mod tools; // the tools\n"),
            file("src/x/one.rs", "// one\n"),
        ]);
        assert!(found(&m, &["src/tools.rs", "src/x/*.rs"]).is_empty());
    }

    #[test]
    fn a_dotted_name_needs_two_plain_parts() {
        assert_eq!(
            dotted_names("apps/ai/harness/tools.py"),
            ["harness.tools", "ai.harness.tools", "apps.ai.harness.tools"]
        );
        assert_eq!(
            dotted_names("crates/teamwire-cli/src/main.rs"),
            ["src.main"]
        );
        assert!(dotted_names("tools.py").is_empty());
        assert!(has_token("uses harness.tools.", "harness.tools"));
        assert!(!has_token("uses harness.tools.x", "harness.tools"));
    }
}
