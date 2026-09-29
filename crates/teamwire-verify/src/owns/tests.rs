use super::Owns;

fn owns(patterns: &[&str]) -> Owns {
    Owns::new(&patterns.iter().map(|p| (*p).to_owned()).collect::<Vec<_>>())
}

#[test]
fn a_trailing_slash_is_a_directory_prefix() {
    let o = owns(&["crates/teamwire-verify/"]);
    assert!(o.matches("crates/teamwire-verify/src/lib.rs"));
    assert!(o.matches("crates/teamwire-verify/Cargo.toml"));
    assert!(!o.matches("crates/teamwire-verify-extra/lib.rs"));
    assert!(!o.matches("crates/teamwire-verify"));
}

#[test]
fn a_single_star_stays_in_one_segment() {
    let o = owns(&["src/*.rs"]);
    assert!(o.matches("src/lib.rs"));
    assert!(!o.matches("src/cmd/verify.rs"));
}

#[test]
fn a_double_star_crosses_segments() {
    let o = owns(&["src/**/*.rs"]);
    assert!(o.matches("src/lib.rs"));
    assert!(o.matches("src/cmd/verify.rs"));
    assert!(!o.matches("tests/verify.rs"));
}

#[test]
fn a_leading_dot_slash_is_ignored() {
    assert!(owns(&["./docs/"]).matches("docs/a.md"));
}

#[test]
fn an_invalid_glob_matches_nothing_and_is_reported() {
    let o = owns(&["src/[", "docs/"]);
    assert_eq!(o.invalid, ["src/["]);
    assert!(o.matches("docs/a.md"));
    assert!(!o.matches("src/["));
}

/// A small matcher written straight from the rule in ADR 0009: `dir/` is a
/// prefix; otherwise segments are matched one by one, `*` inside a segment
/// stands for any run of characters, and a `**` segment stands for any run of
/// whole segments (at least one when it ends the pattern).
mod reference {
    pub(super) fn matches(pattern: &str, path: &str) -> bool {
        if pattern.ends_with('/') {
            return path.starts_with(pattern);
        }
        let pattern: Vec<&str> = pattern.split('/').collect();
        let path: Vec<&str> = path.split('/').collect();
        segments(&pattern, &path)
    }

    fn segments(pattern: &[&str], path: &[&str]) -> bool {
        match pattern.split_first() {
            None => path.is_empty(),
            Some((&"**", [])) => !path.is_empty(),
            Some((&"**", rest)) => (0..=path.len())
                .any(|skip| path.get(skip..).is_some_and(|tail| segments(rest, tail))),
            Some((first, rest)) => path
                .split_first()
                .is_some_and(|(head, tail)| within_segment(first, head) && segments(rest, tail)),
        }
    }

    fn within_segment(pattern: &str, text: &str) -> bool {
        let parts: Vec<&str> = pattern.split('*').collect();
        let Some((first, rest)) = parts.split_first() else {
            return false;
        };
        let Some(mut remaining) = text.strip_prefix(first) else {
            return false;
        };
        let Some((last, middle)) = rest.split_last() else {
            return remaining.is_empty();
        };
        for part in middle {
            let Some(at) = remaining.find(part) else {
                return false;
            };
            remaining = remaining.get(at + part.len()..).unwrap_or("");
        }
        remaining.ends_with(last) && remaining.len() >= last.len()
    }
}

mod property {
    use proptest::prelude::*;

    use super::{Owns, reference};

    fn pattern_segment() -> impl Strategy<Value = String> {
        prop::sample::select(vec!["a", "b", "ab", "*", "**", "a*", "*b", "a*b", "*a*"])
            .prop_map(str::to_owned)
    }

    fn pattern() -> impl Strategy<Value = String> {
        (
            prop::collection::vec(pattern_segment(), 1..4),
            any::<bool>(),
        )
            .prop_map(|(segs, dir)| {
                let joined = segs.join("/");
                // A directory pattern is a plain prefix: no wildcards in this test.
                if dir {
                    format!("{}/", joined.replace('*', "a"))
                } else {
                    joined
                }
            })
    }

    fn path() -> impl Strategy<Value = String> {
        prop::collection::vec(
            prop::sample::select(vec!["a", "b", "ab", "ba", "aab"]),
            1..5,
        )
        .prop_map(|segs| segs.join("/"))
    }

    proptest! {
        #[test]
        fn owns_agrees_with_the_reference_matcher(pattern in pattern(), path in path()) {
            let owns = Owns::new(std::slice::from_ref(&pattern));
            prop_assert_eq!(
                owns.matches(&path),
                reference::matches(&pattern, &path),
                "pattern {} path {}", pattern, path
            );
        }
    }
}
