//! The spec-kit checks of `teamwire doctor`: printed only inside a git
//! repository whose root holds `.specify/`, each `ok` or `warn`, never a
//! failure. The in-process tests give git's answers through the fake
//! platform; the binary test asks a real git.

use super::*;

/// A constitution `/speckit-constitution` filled in: no placeholder left.
const FILLED: &str = "# wordtool Constitution\n\n## Core Principles\n\n### I. Small CLI\n\
                      Every feature is a subcommand.\n\n**Version**: 1.0.0 | **Ratified**: 2026-11-10\n";

/// A healthy bed whose repository is a spec-kit project: a `.git` and a
/// `.specify/` with a filled constitution. git answers yes to both questions.
fn speckit_bed() -> Bed {
    let mut bed = Bed::healthy();
    let root = bed.home.path();
    std::fs::create_dir_all(root.join(".git")).unwrap();
    put(root, ".specify/memory/constitution.md", FILLED);
    bed.speckit_git = vec![0, 0];
    bed
}

fn put(root: &Path, file: &str, text: &str) {
    let path = root.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn speckit_names(checks: &[Check]) -> Vec<&str> {
    names(checks)
        .into_iter()
        .filter(|n| n.starts_with("spec-kit"))
        .collect()
}

#[test]
fn outside_a_spec_kit_repository_doctor_prints_no_spec_kit_line() {
    // A git repository without `.specify/`.
    let bed = Bed::healthy();
    std::fs::create_dir_all(bed.home.path().join(".git")).unwrap();
    assert_eq!(speckit_names(&bed.checks()), Vec::<&str>::new());

    // A `.specify/` outside any git repository.
    let bed = Bed::healthy();
    put(bed.home.path(), ".specify/memory/constitution.md", FILLED);
    assert_eq!(speckit_names(&bed.checks()), Vec::<&str>::new());
}

#[test]
fn a_committed_constitution_and_an_ignored_pointer_are_ok_and_git_is_asked_at_the_root() {
    let bed = speckit_bed();
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "spec-kit committed"), Level::Pass);
    assert_eq!(level_of(&checks, "spec-kit feature pointer"), Level::Pass);
    let runs = bed.platform.runs();
    let git: Vec<(Vec<String>, Option<PathBuf>)> = runs
        .iter()
        .filter(|(c, _)| c.program == "git")
        .map(|(c, _)| (c.args.clone(), c.cwd.clone()))
        .collect();
    let root = Some(bed.home.path().to_path_buf());
    assert!(
        git.contains(&(
            vec![
                "ls-files".to_owned(),
                "--error-unmatch".to_owned(),
                "--".to_owned(),
                ".specify/memory/constitution.md".to_owned()
            ],
            root.clone()
        )),
        "{git:?}"
    );
    assert!(
        git.contains(&(
            vec![
                "check-ignore".to_owned(),
                "-q".to_owned(),
                ".specify/feature.json".to_owned()
            ],
            root
        )),
        "{git:?}"
    );
}

#[test]
fn an_untracked_constitution_and_a_pointer_git_does_not_ignore_warn_and_name_the_file() {
    let mut bed = speckit_bed();
    bed.speckit_git = vec![1, 1];
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "spec-kit committed"), Level::Warn);
    assert!(
        detail_of(&checks, "spec-kit committed").contains(".specify/memory/constitution.md"),
        "{checks:?}"
    );
    assert_eq!(level_of(&checks, "spec-kit feature pointer"), Level::Warn);
    assert!(
        detail_of(&checks, "spec-kit feature pointer").contains(".specify/feature.json"),
        "{checks:?}"
    );
}

#[test]
fn a_git_that_cannot_answer_is_a_warning() {
    let mut bed = speckit_bed();
    bed.speckit_git = vec![128, 128];
    let checks = bed.checks();
    assert_eq!(level_of(&checks, "spec-kit committed"), Level::Warn);
    assert_eq!(level_of(&checks, "spec-kit feature pointer"), Level::Warn);
}

/// `git` with a fixed author, so a commit works on any machine.
fn git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn line_of<'a>(out: &'a str, name: &str) -> &'a str {
    out.lines()
        .find(|l| {
            l.get(5..)
                .is_some_and(|rest| rest.starts_with(&format!("{name}:")))
        })
        .unwrap_or_else(|| unreachable!("no line {name}: {out}"))
}

#[test]
fn the_binary_asks_a_real_git_from_a_subdirectory_of_the_repository() {
    let bed = Bed::healthy();
    let bins = FakeBins::ensure().unwrap();
    let fake_dir = bed.bin.path().join("fakes");
    bins.install(&fake_dir).unwrap();
    let path = format!("{}:/usr/bin:/bin", fake_dir.display());
    let repo = ignoring_repo();
    let sub = repo.path().join("src");
    std::fs::create_dir_all(&sub).unwrap();
    put(repo.path(), ".specify/memory/constitution.md", FILLED);

    // spec-kit's own `.specify/.gitignore` is not there yet, and nothing is committed.
    let out = teamwire(&bed, &path, &sub).out;
    assert!(
        line_of(&out, "spec-kit committed").starts_with("warn"),
        "{out}"
    );
    assert!(
        line_of(&out, "spec-kit feature pointer").starts_with("warn"),
        "{out}"
    );
    assert!(!out.contains("FAIL spec-kit"), "{out}");

    put(repo.path(), ".specify/.gitignore", "feature.json\n");
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-q", "-m", "spec-kit"]);
    let out = teamwire(&bed, &path, &sub).out;
    assert!(
        line_of(&out, "spec-kit committed").starts_with("ok"),
        "{out}"
    );
    assert!(
        line_of(&out, "spec-kit feature pointer").starts_with("ok"),
        "{out}"
    );
}
