//! The words of one task read against the tracked files (ADR 0047,
//! ADR 0049): which are repository paths, which the lead must check, and
//! which still count in the overlap.

use teamwire_verify::Owns;

use super::parse::has_extension;

/// The words of one task, read against the tracked files (ADR 0047).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Expanded {
    /// The repository paths, each once.
    pub(crate) paths: Vec<String>,
    /// The words with a `/` that rules 1 to 3 turned away, each once. The
    /// lead checks them for a new file in a new folder. A word with two
    /// readings carries a note on the tracked path it also ends.
    pub(crate) not_paths: Vec<String>,
    /// The words of `not_paths`, without a note, that only rule 2 or 3 or
    /// ADR 0049 rule D turned away. A new
    /// folder's files land here, so they still count in the overlap, though
    /// no draft holds them. Code counts nowhere.
    pub(crate) unresolved: Vec<String>,
    /// The words with no `/` that resolved to a root file, each once
    /// (ADR 0049 rule C). They stay in `paths`. The lead checks each,
    /// since a bare name may mean a file in a folder the task names.
    pub(crate) bare_roots: Vec<String>,
}

/// How one word reads against the tracked files.
enum Read {
    Paths(Vec<String>),
    /// Code (ADR 0047 rule 1): the lead sees it when it holds a `/`.
    Code,
    /// A word that does not resolve (rules 2 and 3): the lead sees it, and
    /// the overlap counts it, when it holds a `/`.
    Unresolved,
    /// A word that could be a new file or a tracked path it ends (ADR 0049
    /// rule D). It reads as `Unresolved`, and the lead sees this note, such
    /// as `(also ends crates/c/tests/preset.rs)`, after it.
    TwoReadings(String),
    /// A word ADR 0044 already turned away, such as a word with no
    /// extension that names no tracked folder.
    Skip,
}

/// The words of `paths` that are repository paths (FR-013, ADR 0047), with
/// each glob (a path that holds `*`) replaced by the tracked files it
/// matches, as verify's `owns` matches them. A glob that matches no tracked
/// file is no path.
///
/// A word with a `/` is a path when it names a tracked file or a tracked
/// folder (kept with a trailing `/`), or when it ends in an extension and
/// its parent folder is tracked, as a file a task creates does, and no
/// tracked path ends in it (ADR 0049 rule D). Otherwise a word that is the
/// end of exactly one tracked path is that path. A word with
/// no `/` is a path only when it names a tracked file at the repository's
/// root, such as `README.md`: an event name such as `knit.progress` or a
/// bare `lib.rs` is not.
pub(crate) fn expand(paths: &[String], tracked: &[String]) -> Expanded {
    let mut out = Expanded::default();
    let add = |list: &mut Vec<String>, p: &str| {
        if !list.iter().any(|o| o == p) {
            list.push(p.to_owned());
        }
    };
    for word in paths {
        match read(word, tracked) {
            Read::Paths(found) => {
                for p in &found {
                    add(&mut out.paths, p);
                }
                if !word.contains(['/', '*']) {
                    add(&mut out.bare_roots, word);
                }
            }
            Read::Code if word.contains('/') => add(&mut out.not_paths, word),
            Read::Unresolved if word.contains('/') => {
                add(&mut out.not_paths, word);
                add(&mut out.unresolved, word);
            }
            Read::TwoReadings(note) => {
                add(&mut out.not_paths, &format!("{word} {note}"));
                add(&mut out.unresolved, word);
            }
            Read::Code | Read::Unresolved | Read::Skip => {}
        }
    }
    out
}

fn read(word: &str, tracked: &[String]) -> Read {
    if is_code(word) {
        return Read::Code;
    }
    if !word.contains('*') {
        return repo_path(word, tracked);
    }
    let owns = Owns::new(&[word.to_owned()]);
    let matched: Vec<String> = tracked
        .iter()
        .filter(|t| owns.matches(t))
        .cloned()
        .collect();
    if matched.is_empty() {
        Read::Unresolved
    } else {
        Read::Paths(matched)
    }
}

/// True when `word` is code, not a path (ADR 0047 rule 1): it holds a quote,
/// a bracket, a comma, a semicolon, a `!` or a `::`, as
/// `"z/c/**".to_owned(` and `Path::new(env!(..))` do. The words are split
/// at white space before this, so a word never holds one.
fn is_code(word: &str) -> bool {
    word.contains(['"', '\'', '(', ')', ',', ';', '!']) || word.contains("::")
}

/// `word` read as a repository path (see `expand`).
fn repo_path(word: &str, tracked: &[String]) -> Read {
    let one = |p: String| Read::Paths(vec![p]);
    if !word.contains('/') {
        return if tracked.iter().any(|t| t == word) {
            one(word.to_owned())
        } else {
            Read::Skip
        };
    }
    let bare = word.trim_end_matches('/');
    if !word.ends_with('/') && tracked.iter().any(|t| t == bare) {
        return one(bare.to_owned());
    }
    let folder = format!("{bare}/");
    if tracked.iter().any(|t| t.starts_with(&folder)) {
        return one(folder);
    }
    if word.ends_with('/') || !has_extension(word, 1) {
        return Read::Skip;
    }
    // A partial path, such as `cmd/send.rs`, is the one tracked path it
    // ends at a `/`. When it ends several, the task did not say which.
    let tail = format!("/{word}");
    let mut ends: Vec<&String> = tracked.iter().filter(|t| t.ends_with(&tail)).collect();
    // A new file counts only in a folder that already holds tracked files.
    let parent = bare.rsplit_once('/').map_or("", |(dir, _)| dir);
    let parent = format!("{parent}/");
    if tracked.iter().any(|t| t.starts_with(&parent)) {
        // `tests/preset.rs` may be a new file in the root `tests/`, or the
        // tracked `crates/c/tests/preset.rs`. The task did not say which,
        // so the lead decides (ADR 0049 rule D).
        ends.sort();
        return match ends.as_slice() {
            [] => one(word.to_owned()),
            [first] => Read::TwoReadings(format!("(also ends {first})")),
            [first, rest @ ..] => {
                Read::TwoReadings(format!("(also ends {first} and {} more)", rest.len()))
            }
        };
    }
    match ends.as_slice() {
        [only] => one((*only).clone()),
        _ => Read::Unresolved,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    /// A glob that matches no tracked file, `src/new/*.rs`, is no path
    /// (ADR 0047 rule 2).
    #[test]
    fn a_star_token_expands_against_tracked_files() {
        let tracked = strings(&[
            "crates/x/src/cmd/speckit/one.rs",
            "crates/x/src/cmd/speckit/two.rs",
            "crates/x/src/cmd/speckit/deep/three.rs",
            "crates/x/src/cmd/other.rs",
            "src/b.rs",
        ]);
        let paths = strings(&["crates/x/src/cmd/speckit/*.rs", "src/new/*.rs", "src/a.rs"]);
        assert_eq!(
            expand(&paths, &tracked).paths,
            strings(&[
                "crates/x/src/cmd/speckit/one.rs",
                "crates/x/src/cmd/speckit/two.rs",
                "src/a.rs"
            ])
        );
    }

    /// ADR 0047 rule 1, from the words 002's `tasks.md` gave: code in
    /// backticks is no path. Each other word sits in the tracked folder
    /// `z/c/`, so only this rule turns it away.
    #[test]
    fn a_word_that_holds_code_is_no_path() {
        let tracked = strings(&["examples/skills/teamwire/SKILL.md", "z/c/one.rs"]);
        let words = strings(&[
            "z/c/**\".to_owned(",
            "Path::new(env!(\"CARGO_MANIFEST_DIR\")).join(\"../../examples/skills/teamwire/SKILL.md",
            "z/c/a\"b.rs",
            "z/c/'a'.rs",
            "z/c/f(x).rs",
            "z/c/a.rs,z/c/b.rs",
            "z/c/a;b.rs",
            "z/c/a!.rs",
            "z/c/a::b.rs",
        ]);
        assert!(expand(&words, &tracked).paths.is_empty());
    }

    /// ADR 0047 rule 2, from 002's `tasks.md`: an example glob that matches
    /// no tracked file and a template path in an untracked folder are no
    /// paths. A tracked file, a tracked folder and a new file in a tracked
    /// folder are.
    #[test]
    fn a_path_must_resolve_against_the_tracked_files() {
        let tracked = strings(&[
            "crates/teamwire-cli/src/cmd/send.rs",
            "specs/002-x/tasks.md",
        ]);
        let words = strings(&[
            "crates/b/**",
            "crates/c/**",
            "specs/NNN-name/tasks.md",
            "crates/teamwire-cli/src/cmd/send.rs",
            "crates/teamwire-cli/src/",
            "crates/teamwire-cli/src/cmd/amend.rs",
            "specs/002-x/plan.md",
        ]);
        assert_eq!(
            expand(&words, &tracked).paths,
            strings(&[
                "crates/teamwire-cli/src/cmd/send.rs",
                "crates/teamwire-cli/src/",
                "crates/teamwire-cli/src/cmd/amend.rs",
                "specs/002-x/plan.md",
            ])
        );
    }

    /// ADR 0047 rule 3, from 002's `tasks.md`: a partial path is the one
    /// tracked path it ends, at a `/`. One that ends two tracked paths, or
    /// none, is no path.
    #[test]
    fn a_partial_path_resolves_when_it_ends_one_tracked_path() {
        let tracked = strings(&[
            "crates/teamwire-cli/src/cmd/send.rs",
            "crates/teamwire-cli/src/cmd/verify.rs",
            "examples/skills/teamwire/SKILL.md",
            "specs/002-x/contracts/amend-cli.md",
            "crates/p/src/cmd/lib.rs",
            "crates/q/src/cmd/lib.rs",
        ]);
        let words = strings(&[
            "cmd/send.rs",
            "cmd/verify.rs",
            "skills/teamwire/SKILL.md",
            "contracts/amend-cli.md",
            "teamwire/skills/teamwire/SKILL.md",
            "cmd/lib.rs",
            "md/send.rs",
        ]);
        assert_eq!(
            expand(&words, &tracked).paths,
            strings(&[
                "crates/teamwire-cli/src/cmd/send.rs",
                "crates/teamwire-cli/src/cmd/verify.rs",
                "examples/skills/teamwire/SKILL.md",
                "specs/002-x/contracts/amend-cli.md",
            ])
        );
    }

    /// ADR 0047 rule 4: each word rules 1 to 3 turn away that holds a `/`
    /// is kept for the lead, once. A word ADR 0044 already turned away, and
    /// a word with no `/`, is not.
    #[test]
    fn each_turned_away_word_with_a_slash_is_kept_for_the_lead() {
        let tracked = strings(&[
            "crates/p/src/cmd/lib.rs",
            "crates/q/src/cmd/lib.rs",
            "z/c/one.rs",
        ]);
        let words = strings(&[
            "z/c/**\".to_owned(",
            "crates/b/**",
            "specs/NNN-name/tasks.md",
            "cmd/lib.rs",
            "teamwire/skills/teamwire/SKILL.md",
            "crates/b/**",
            "foo(\"x.rs",
            "deck_editor/",
            "apps/ai",
            "lib.rs",
            "z/c/one.rs",
        ]);
        let got = expand(&words, &tracked);
        assert_eq!(got.paths, strings(&["z/c/one.rs"]));
        assert_eq!(
            got.not_paths,
            strings(&[
                "z/c/**\".to_owned(",
                "crates/b/**",
                "specs/NNN-name/tasks.md",
                "cmd/lib.rs",
                "teamwire/skills/teamwire/SKILL.md",
            ])
        );
    }

    /// A word that fails only rule 2 or 3 still counts in the overlap; a
    /// code word counts nowhere.
    #[test]
    fn an_unresolved_word_counts_in_the_overlap_and_code_does_not() {
        let tracked = strings(&["crates/p/src/cmd/lib.rs", "crates/q/src/cmd/lib.rs"]);
        let words = strings(&[
            "z/c/**\".to_owned(",
            "crates/b/**",
            "apps/ai/gateway/decks/tools.py",
            "cmd/lib.rs",
        ]);
        assert_eq!(
            expand(&words, &tracked).unresolved,
            strings(&[
                "crates/b/**",
                "apps/ai/gateway/decks/tools.py",
                "cmd/lib.rs"
            ])
        );
    }

    /// ADR 0049 rule D: a word the new-file rule would accept, and that a
    /// tracked path also ends, has two readings, so it is no path. It is
    /// kept for the lead with the first such path and the count of the
    /// rest, and it still counts in the overlap. A word no tracked path
    /// ends stays a new file.
    #[test]
    fn a_partial_path_with_two_readings_is_no_path() {
        let tracked = strings(&[
            "tests/fakes/a.rs",
            "crates/c/tests/preset.rs",
            "crates/d/tests/x.rs",
            "crates/a/tests/x.rs",
            "crates/b/tests/x.rs",
        ]);
        let words = strings(&["tests/preset.rs", "tests/x.rs", "tests/brand_new.rs"]);
        let got = expand(&words, &tracked);
        assert_eq!(got.paths, strings(&["tests/brand_new.rs"]));
        assert_eq!(
            got.not_paths,
            strings(&[
                "tests/preset.rs (also ends crates/c/tests/preset.rs)",
                "tests/x.rs (also ends crates/a/tests/x.rs and 2 more)",
            ])
        );
        assert_eq!(got.unresolved, strings(&["tests/preset.rs", "tests/x.rs"]));
    }

    /// ADR 0049 rule C: a bare word that names a root file stays a path and
    /// is also kept for the lead. A word with a `/`, a glob and a bare word
    /// that names no root file are not kept.
    #[test]
    fn a_bare_word_read_as_a_root_file_is_kept_for_the_lead() {
        let tracked = strings(&["AGENTS.md", "README.md", "apps/x.rs"]);
        let words = strings(&["AGENTS.md", "lib.rs", "apps/x.rs", "*.md", "AGENTS.md"]);
        let got = expand(&words, &tracked);
        assert_eq!(got.bare_roots, strings(&["AGENTS.md"]));
        assert_eq!(got.paths, strings(&["AGENTS.md", "apps/x.rs", "README.md"]));
    }
}
