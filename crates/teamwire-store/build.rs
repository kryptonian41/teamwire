//! Records the git commit this build was made from, so `WRITER_VERSION` tells
//! two builds of the same crate version apart. It reads `.git` files directly
//! (no `git` process, no dependency) and falls back to `unknown`.

use std::path::{Path, PathBuf};

/// The `.git` entry at or above `start`. A worktree's `.git` is a file that
/// names the real directory.
fn git_dir(start: &Path) -> Option<PathBuf> {
    let dot_git = start
        .ancestors()
        .map(|d| d.join(".git"))
        .find(|p| p.exists())?;
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    let text = std::fs::read_to_string(&dot_git).ok()?;
    let target = text.strip_prefix("gitdir:")?.trim();
    Some(dot_git.parent()?.join(target))
}

fn is_hex_sha(text: &str) -> bool {
    text.len() >= 12 && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The commit `HEAD` points at, and the files whose change moves it.
fn head_commit(git: &Path, watch: &mut Vec<PathBuf>) -> Option<String> {
    let head_path = git.join("HEAD");
    watch.push(head_path.clone());
    let head = std::fs::read_to_string(head_path).ok()?;
    let head = head.trim();
    let Some(reference) = head.strip_prefix("ref:").map(str::trim) else {
        return is_hex_sha(head).then(|| head.to_owned());
    };
    // A worktree keeps `HEAD` in its own directory and the branches in the
    // common one, named by its `commondir` file.
    let common = std::fs::read_to_string(git.join("commondir"))
        .ok()
        .map_or_else(|| git.to_path_buf(), |c| git.join(c.trim()));
    for dir in [git, common.as_path()] {
        let path = dir.join(reference);
        watch.push(path.clone());
        if let Ok(text) = std::fs::read_to_string(path)
            && is_hex_sha(text.trim())
        {
            return Some(text.trim().to_owned());
        }
    }
    let packed = common.join("packed-refs");
    watch.push(packed.clone());
    std::fs::read_to_string(packed)
        .ok()?
        .lines()
        .find_map(|line| {
            let (sha, name) = line.split_once(' ')?;
            (name == reference && is_hex_sha(sha)).then(|| sha.to_owned())
        })
}

fn main() {
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from);
    let mut watch = Vec::new();
    let commit = manifest
        .as_deref()
        .and_then(git_dir)
        .and_then(|git| head_commit(&git, &mut watch))
        .map_or_else(|| "unknown".to_owned(), |sha| sha[..12].to_owned());
    // A release build (what `cargo install` runs, in the workspace `target/`)
    // reruns when HEAD or its branch moves, so an installed binary names the
    // commit it was installed from. Every other profile reruns only when this
    // script changes, so a commit does not rebuild every crate above the store;
    // a development build keeps the commit it was first built at.
    if std::env::var("PROFILE").is_ok_and(|p| p == "release") {
        for path in watch {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rustc-env=TEAMWIRE_BUILD_COMMIT={commit}");
}
