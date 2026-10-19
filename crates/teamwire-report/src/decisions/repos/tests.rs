use super::*;

fn dirs(paths: &[&str], home: Option<&str>) -> Vec<PathBuf> {
    // `repo_dirs` reads members; build the same result from bare paths.
    let home = home.map(|h| normalize(Path::new(h)));
    paths
        .iter()
        .map(|p| normalize(Path::new(p)))
        .filter(|d| d.parent().is_some() && home.as_ref() != Some(d))
        .collect()
}

#[test]
fn a_worktree_path_becomes_its_main_repository() {
    assert_eq!(
        normalize(Path::new("/nonexistent/proj/.worktrees/team-a/crate")),
        PathBuf::from("/nonexistent/proj")
    );
}

#[test]
fn dot_dot_is_resolved_for_a_path_that_is_gone() {
    assert_eq!(
        normalize(Path::new("/nonexistent/a/../b/./c")),
        PathBuf::from("/nonexistent/b/c")
    );
}

#[test]
fn the_home_directory_and_root_are_left_out() {
    let list = dirs(
        &["/nonexistent/me", "/", "/nonexistent/me/proj"],
        Some("/nonexistent/me"),
    );
    assert_eq!(list, [PathBuf::from("/nonexistent/me/proj")]);
}
