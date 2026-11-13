//! `owns` matching (ADR 0009 point 5, VER-7). A pattern ending in `/` is a
//! directory prefix. Any other pattern is a glob where `*` stays inside one
//! path segment and `**` crosses segments.

use globset::{Glob, GlobBuilder, GlobSet, GlobSetBuilder};

/// A member's `owns` list, compiled once per verify run.
#[derive(Debug)]
pub struct Owns {
    prefixes: Vec<String>,
    globs: GlobSet,
    /// Patterns that are not valid globs. They match nothing, and verify
    /// flags each one so the mistake is seen.
    pub invalid: Vec<String>,
}

fn glob(pattern: &str) -> Result<Glob, globset::Error> {
    GlobBuilder::new(pattern).literal_separator(true).build()
}

impl Owns {
    pub fn new(patterns: &[String]) -> Self {
        let mut prefixes = Vec::new();
        let mut builder = GlobSetBuilder::new();
        let mut invalid = Vec::new();
        for raw in patterns {
            let pattern = raw.strip_prefix("./").unwrap_or(raw);
            if pattern.ends_with('/') {
                prefixes.push(pattern.to_owned());
            } else {
                match glob(pattern) {
                    Ok(g) => {
                        builder.add(g);
                    }
                    Err(_) => invalid.push(raw.clone()),
                }
            }
        }
        let globs = builder.build().unwrap_or_else(|_| GlobSet::empty());
        Self {
            prefixes,
            globs,
            invalid,
        }
    }

    /// True when the member listed no pattern at all.
    pub fn is_empty(&self) -> bool {
        self.prefixes.is_empty() && self.globs.is_empty() && self.invalid.is_empty()
    }

    pub fn matches(&self, path: &str) -> bool {
        self.prefixes.iter().any(|p| path.starts_with(p.as_str())) || self.globs.is_match(path)
    }
}

#[cfg(test)]
mod tests;
