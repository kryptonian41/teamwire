use super::{
    header_path, hunk_start, in_lint_section, scan_patch, section_by_line, sets_nothing,
    test_removed_flag,
};
use crate::rules::Ruleset;

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

/// The ids of the flags the patch text raises without reading a repository.
fn ids_of(patch: &str) -> Vec<String> {
    let rules = Ruleset::load().unwrap();
    let (mut flags, files) = scan_patch(patch, &rules);
    for file in &files {
        let path = file.new_path.as_deref().or(file.old_path.as_deref());
        flags.extend(path.and_then(|p| test_removed_flag(file, p, &rules)));
    }
    flags.into_iter().map(|f| f.id).collect()
}

#[test]
fn commented_test_attribute_is_a_removed_test() {
    let patch = "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n\
                 @@ -3 +3 @@\n-#[test]\n+// #[test]\n";
    assert_eq!(ids_of(patch), ["test-removed:src/a.rs"]);
}

#[test]
fn deleted_file_with_inline_tests_is_flagged() {
    let patch = "diff --git a/src/parser.rs b/src/parser.rs\ndeleted file mode 100644\n\
                 --- a/src/parser.rs\n+++ /dev/null\n@@ -1,4 +0,0 @@\n-fn p() {}\n-#[test]\n-fn t() {}\n";
    assert_eq!(ids_of(patch), ["test-removed:src/parser.rs"]);
}

#[test]
fn a_deleted_test_file_is_the_test_deleted_flag_only() {
    let patch = "diff --git a/tests/a.rs b/tests/a.rs\ndeleted file mode 100644\n\
                 --- a/tests/a.rs\n+++ /dev/null\n@@ -1,2 +0,0 @@\n-#[test]\n-fn t() {}\n";
    assert!(ids_of(patch).is_empty());
}

#[test]
fn cfg_attr_ignore_is_weakening() {
    for line in [
        "#[cfg_attr(test, ignore)]",
        "#[cfg_attr(not(ci), ignore = \"slow\")]",
        "#[cfg(any())]",
        "#[cfg(FALSE)]",
        "#[cfg(not(all()))]",
    ] {
        let patch = format!(
            "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1,0 +2 @@\n+{line}\n"
        );
        assert_eq!(ids_of(&patch), ["weakened:src/a.rs:2"], "{line}");
    }
    let fine = "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1,0 +2 @@\n+#[cfg_attr(test, derive(Debug))]\n";
    assert!(ids_of(fine).is_empty());
}
