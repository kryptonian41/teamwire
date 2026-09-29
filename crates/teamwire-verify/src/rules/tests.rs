use super::Ruleset;

fn rules() -> Ruleset {
    Ruleset::load().unwrap()
}

#[test]
fn every_shipped_rule_file_parses_and_names_its_language() {
    let r = rules();
    let names: Vec<&str> = r.languages.iter().map(|l| l.language.as_str()).collect();
    assert_eq!(
        names,
        ["rust", "python", "javascript", "go", "java", "common"]
    );
}

#[test]
fn rust_rules_cover_every_marker_of_ver_4() {
    let r = rules();
    for line in [
        "#[ignore]",
        "#[ignore = \"slow\"]",
        "    #[should_panic(expected = \"x\")]",
        "#[allow(dead_code)]",
        "#![allow(clippy::all)]",
        "#[expect(clippy::unwrap_used, reason = \"r\")]",
    ] {
        assert!(r.weakening("src/lib.rs", line).is_some(), "{line}");
    }
    assert!(
        r.weakening("src/lib.rs", "// #[ignore] is banned")
            .is_none()
    );
    assert!(r.weakening("src/lib.rs", "//! never #[allow(x)]").is_none());
    assert!(r.weakening("README.md", "#[ignore]").is_none());
    assert!(r.is_test_marker("tests/a.rs", "#[test]"));
}

#[test]
fn rust_config_files_are_check_config_at_any_depth() {
    let r = rules();
    for path in [
        "clippy.toml",
        "crates/a/rustfmt.toml",
        "deny.toml",
        "rust-toolchain.toml",
        ".cargo/config.toml",
        "sub/.cargo/config.toml",
        ".github/workflows/ci.yml",
        "pyproject.toml",
        "web/tsconfig.json",
    ] {
        assert!(r.is_check_config(path), "{path}");
    }
    assert!(!r.is_check_config("src/lib.rs"));
    assert!(!r.is_check_config("Cargo.toml"));
    assert_eq!(
        r.lint_sections("crates/a/Cargo.toml").unwrap(),
        ["lints", "workspace.lints"]
    );
    assert!(r.lint_sections("pyproject.toml").is_none());
}

#[test]
fn test_files_of_each_language_are_recognised() {
    let r = rules();
    for path in [
        "tests/a.rs",
        "crates/x/tests/verify.rs",
        "src/owns/tests.rs",
        "test_a.py",
        "pkg/test/util.py",
        "web/a.test.ts",
        "web/__tests__/a.js",
        "pkg/a_test.go",
        "src/test/java/ATest.java",
    ] {
        assert!(r.is_test_file(path), "{path}");
    }
    assert!(!r.is_test_file("src/lib.rs"));
}

#[test]
fn other_languages_keep_their_python_patterns() {
    let r = rules();
    assert!(
        r.weakening("a.py", "@pytest.mark.skip(reason='x')")
            .is_some()
    );
    assert!(r.weakening("a.py", "x = 1  # type: ignore").is_some());
    assert!(r.weakening("a.ts", "it.skip('x', () => {})").is_some());
    assert!(r.weakening("a.ts", "// @ts-expect-error").is_some());
    assert!(r.weakening("a.go", "\tt.Skip(\"no\")").is_some());
    assert!(r.weakening("A.java", "@Disabled").is_some());
}

#[test]
fn a_word_start_pattern_does_not_match_inside_a_longer_name() {
    let r = rules();
    assert!(r.weakening("a.js", "process.exit(1)").is_none());
    assert!(r.weakening("a.js", "xit('later', f)").is_some());
    assert!(r.weakening("a.js", "  xdescribe('x', f)").is_some());
    assert!(r.weakening("a.py", "def unskip_pytest.skip(").is_none());
}
