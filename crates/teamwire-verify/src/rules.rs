//! The per-language rules of VER-4, read from `rules/*.toml`. The files are
//! part of the binary (`include_str!`), so verify never depends on a file
//! that may be missing where it runs.

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Deserialize;

use crate::VerifyError;

const FILES: [(&str, &str); 6] = [
    ("rust", include_str!("../rules/rust.toml")),
    ("python", include_str!("../rules/python.toml")),
    ("javascript", include_str!("../rules/javascript.toml")),
    ("go", include_str!("../rules/go.toml")),
    ("java", include_str!("../rules/java.toml")),
    ("common", include_str!("../rules/common.toml")),
];

/// One added line that turns a check off instead of satisfying it.
#[derive(Debug, Deserialize)]
struct Weakening {
    id: String,
    contains: String,
    /// Every one of these must be on the line too: `cfg_attr(` with `ignore`.
    #[serde(default)]
    and_contains: Vec<String>,
    /// The text must not continue an identifier, so `xit(` does not match
    /// inside `exit(`.
    #[serde(default)]
    word_start: bool,
}

/// The sections of a manifest that set lint levels.
#[derive(Debug, Deserialize)]
struct LintTables {
    files: Vec<String>,
    sections: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RuleFile {
    language: String,
    #[serde(default)]
    extensions: Vec<String>,
    #[serde(default)]
    comment_prefixes: Vec<String>,
    /// Like `comment_prefixes`, but only for counting test markers. A
    /// weakening can live in a comment (`//nolint`, `# noqa`), so these
    /// languages cannot skip comment lines when they look for one.
    #[serde(default)]
    marker_comment_prefixes: Vec<String>,
    #[serde(default)]
    test_paths: Vec<String>,
    #[serde(default)]
    test_markers: Vec<String>,
    #[serde(default)]
    config_files: Vec<String>,
    lint_tables: Option<LintTables>,
    #[serde(default)]
    weakening: Vec<Weakening>,
}

/// Every language's rules, compiled.
#[derive(Debug)]
pub(crate) struct Ruleset {
    languages: Vec<RuleFile>,
    test_paths: GlobSet,
    config_files: GlobSet,
}

fn globs<'a>(patterns: impl Iterator<Item = &'a String>) -> Result<GlobSet, VerifyError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern)
            .map_err(|e| VerifyError::Internal(format!("rule glob {pattern}: {e}")))?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|e| VerifyError::Internal(format!("rule globs: {e}")))
}

impl Ruleset {
    pub(crate) fn load() -> Result<Self, VerifyError> {
        let mut languages = Vec::new();
        for (name, text) in FILES {
            let file: RuleFile = toml::from_str(text)
                .map_err(|e| VerifyError::Internal(format!("rules/{name}.toml: {e}")))?;
            tracing::debug!(language = %file.language, "loaded verify rules");
            languages.push(file);
        }
        let test_paths = globs(languages.iter().flat_map(|l| &l.test_paths))?;
        let config_files = globs(languages.iter().flat_map(|l| &l.config_files))?;
        Ok(Self {
            languages,
            test_paths,
            config_files,
        })
    }

    /// True for a path that holds tests, in any language.
    pub(crate) fn is_test_file(&self, path: &str) -> bool {
        self.test_paths.is_match(path)
    }

    /// True for a path that decides what a check checks, in any language.
    pub(crate) fn is_check_config(&self, path: &str) -> bool {
        self.config_files.is_match(path)
    }

    fn language_of(&self, path: &str) -> Option<&RuleFile> {
        let ext = path.rsplit_once('.')?.1;
        self.languages
            .iter()
            .find(|l| l.extensions.iter().any(|e| e == ext))
    }

    /// The id of the weakening rule an added line trips, if any. Only files
    /// of a known language are looked at, and a comment line of that language
    /// is prose.
    pub(crate) fn weakening(&self, path: &str, line: &str) -> Option<&str> {
        let language = self.language_of(path)?;
        let trimmed = line.trim_start();
        if language
            .comment_prefixes
            .iter()
            .any(|p| trimmed.starts_with(p.as_str()))
        {
            return None;
        }
        language
            .weakening
            .iter()
            .find(|w| {
                contains(line, &w.contains, w.word_start)
                    && w.and_contains.iter().all(|c| line.contains(c.as_str()))
            })
            .map(|w| w.id.as_str())
    }

    /// True when `line` holds a test marker of the file's language. A comment
    /// line is prose: `// #[test]` is a test switched off, not a test. A
    /// marker must start a word, so `submit(` is not the marker `it(`.
    pub(crate) fn is_test_marker(&self, path: &str, line: &str) -> bool {
        self.language_of(path).is_some_and(|l| {
            let trimmed = line.trim_start();
            !l.comment_prefixes
                .iter()
                .chain(&l.marker_comment_prefixes)
                .any(|p| trimmed.starts_with(p.as_str()))
                && l.test_markers.iter().any(|m| contains(line, m, true))
        })
    }

    /// The manifest sections to watch when `path` is one of the lint-table
    /// files of a language, else `None`.
    pub(crate) fn lint_sections(&self, path: &str) -> Option<&[String]> {
        let name = path.rsplit('/').next().unwrap_or(path);
        self.languages
            .iter()
            .filter_map(|l| l.lint_tables.as_ref())
            .find(|t| t.files.iter().any(|f| f == name))
            .map(|t| t.sections.as_slice())
    }
}

fn contains(line: &str, needle: &str, word_start: bool) -> bool {
    line.match_indices(needle).any(|(at, _)| {
        !word_start
            || line
                .get(..at)
                .and_then(|before| before.chars().next_back())
                .is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '.'))
    })
}

#[cfg(test)]
mod tests;
