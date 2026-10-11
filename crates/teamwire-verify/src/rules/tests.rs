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

#[test]
fn a_commented_out_test_is_not_a_test_marker_in_any_language() {
    let r = rules();
    for (path, line) in [
        ("a_test.go", "// func TestX(t *testing.T) {"),
        ("tests/test_a.py", "# def test_login():"),
        ("src/a.test.ts", "// it(\"logs in\", () => {"),
        ("src/test/java/ATest.java", "// @Test"),
    ] {
        assert!(!r.is_test_marker(path, line), "{path}: {line}");
    }
    assert!(r.is_test_marker("a_test.go", "func TestX(t *testing.T) {"));
    assert!(r.is_test_marker("tests/test_a.py", "    def test_login():"));
    assert!(r.is_test_marker("src/a.test.ts", "  it(\"logs in\", () => {"));
    assert!(r.is_test_marker("src/test/java/ATest.java", "    @Test"));
}

#[test]
fn a_weakening_in_a_comment_is_still_flagged_where_comments_hold_weakenings() {
    let r = rules();
    assert!(r.weakening("a.go", "//nolint:errcheck").is_some());
    assert!(r.weakening("a.py", "# type: ignore").is_some());
    assert!(r.weakening("a.ts", "// eslint-disable-next-line").is_some());
}

#[test]
fn javascript_markers_start_a_word() {
    let r = rules();
    for line in [
        "await init();",
        "emit(x);",
        "submit(form);",
        "/x/.test(s)",
        "const y = latest(a);",
    ] {
        assert!(!r.is_test_marker("src/a.js", line), "{line}");
    }
    for line in [
        "it('x', () => {",
        "  test(\"y\", async () => {",
        "});it('z', f)",
    ] {
        assert!(r.is_test_marker("src/a.js", line), "{line}");
    }
}

#[test]
fn a_rust_weakening_inside_a_string_literal_is_fixture_text() {
    let r = rules();
    for line in [
        // The two lines verify wrongly flagged in the cli and xtask tests.
        r##"        &[("src/lib.rs", "#[allow(dead_code)]\nfn a() {}\n")],"##,
        r#"            "//! Purpose.\n#[test]\n#[cfg_attr(unix, ignore = \"x\")]\nfn one() {}\n","#,
        r##"    std::fs::write(dir.join("a.rs"), "#[allow(dead_code)]\nfn f() {}\n").unwrap();"##,
        r#"    let q = "a \" b #[allow(x)]";"#,
    ] {
        assert!(r.weakening("src/lib.rs", line).is_none(), "{line}");
    }
}

#[test]
fn a_real_rust_weakening_after_a_string_on_the_same_line_is_flagged() {
    let r = rules();
    for line in [
        r#"let s = "x"; #[allow(dead_code)] fn f() {}"#,
        r#"let q = '"'; #[allow(dead_code)] fn f() {}"#,
        r#"#[allow(dead_code)] // "quoted""#,
    ] {
        assert!(r.weakening("src/lib.rs", line).is_some(), "{line}");
    }
}

#[test]
fn other_languages_still_flag_a_marker_inside_a_string() {
    let r = rules();
    assert!(r.weakening("a.py", r#"x = "y"  # type: ignore"#).is_some());
}
