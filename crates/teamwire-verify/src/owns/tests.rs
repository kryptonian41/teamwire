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
