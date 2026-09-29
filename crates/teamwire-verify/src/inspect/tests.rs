use super::{header_path, hunk_start, in_lint_section, section_by_line, sets_nothing};

#[test]
fn a_hunk_header_gives_the_first_old_and_new_line() {
    assert_eq!(hunk_start("@@ -10,2 +12,3 @@ fn a()"), (10, 12));
    assert_eq!(hunk_start("@@ -0,0 +1 @@"), (0, 1));
    assert_eq!(hunk_start("@@ -7 +7 @@"), (7, 7));
}

#[test]
fn a_header_path_drops_the_prefix_and_dev_null_is_no_path() {
    assert_eq!(
        header_path("+++ b/src/a.rs", "b/").as_deref(),
        Some("src/a.rs")
    );
    assert_eq!(
        header_path("--- a/a b.rs\t", "a/").as_deref(),
        Some("a b.rs")
    );
    assert_eq!(header_path("+++ /dev/null", "b/"), None);
}

#[test]
fn every_line_of_a_manifest_knows_its_table() {
    let text = "[package]\nname = \"a\"\n\n[lints]\nworkspace = true\n\n[workspace.lints.clippy]\npedantic = \"warn\"\n";
    let by_line = section_by_line(text);
    let sections = ["lints".to_owned(), "workspace.lints".to_owned()];
    assert!(!in_lint_section(&by_line, 2, &sections));
    assert!(in_lint_section(&by_line, 4, &sections));
    assert!(in_lint_section(&by_line, 5, &sections));
    assert!(in_lint_section(&by_line, 7, &sections));
    assert!(in_lint_section(&by_line, 8, &sections));
    assert!(!in_lint_section(&by_line, 99, &sections));
    assert!(!in_lint_section(&by_line, 0, &sections));
}

#[test]
fn opting_into_the_workspace_table_is_not_a_weakening() {
    assert!(sets_nothing("workspace = true"));
    assert!(sets_nothing("  workspace=true "));
    assert!(sets_nothing("[lints.clippy]"));
    assert!(!sets_nothing("unsafe_code = \"allow\""));
}
