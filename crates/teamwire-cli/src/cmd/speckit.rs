//! `teamwire speckit draft`: reads a spec-kit `tasks.md` and prints the
//! files phases and stories share, the tasks that can run apart, and a DRAFT
//! `owns` list per story (ADR 0044, ADR 0047).
//!
//! Never: write a file or a store row, or call the network. Git is asked
//! only for the tracked files. The lists are drafts for the lead to check.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use teamwire_core::{Command, Platform};

use crate::cmd::shared::{REFUSED, fail, say};
use crate::wiring::Wiring;
use crate::{Global, SpeckitArgs, SpeckitCommand, SpeckitDraftArgs};

// Paths, as `doctor_speckit.rs` attaches its parts.
#[path = "speckit/mentions.rs"]
mod mentions;
#[path = "speckit/overlap.rs"]
mod overlap;
#[path = "speckit/parse.rs"]
mod parse;
#[path = "speckit/render.rs"]
mod render;

pub(crate) fn run(args: &SpeckitArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    match &args.command {
        SpeckitCommand::Draft(a) => draft(a, global, wiring),
    }
}

const COMMAND: &str = "speckit draft";
const GIT_TIMEOUT: Duration = Duration::from_secs(10);

fn draft(args: &SpeckitDraftArgs, _global: &Global, wiring: &Wiring) -> ExitCode {
    let shown = args.tasks.display().to_string();
    let text = match std::fs::read_to_string(&args.tasks) {
        Ok(text) => text,
        Err(e) => return fail(COMMAND, &format!("cannot read {shown}: {e}"), REFUSED),
    };
    let list = parse::parse(&text);
    if list.tasks.is_empty() {
        return fail(
            COMMAND,
            &format!("{shown} has no task lines (- [ ] T<n> ...)"),
            REFUSED,
        );
    }
    let platform = wiring.platform.as_ref();
    let folder = feature_folder(&args.tasks);
    let root = match &args.repo {
        Some(dir) => dir.clone(),
        None => match git_top_level(platform, &folder) {
            Ok(root) => root,
            Err(why) => return fail(COMMAND, &why, REFUSED),
        },
    };
    let tracked = match mentions::tracked_files(platform, &root) {
        Ok(files) => files,
        Err(why) => return fail(COMMAND, &why, REFUSED),
    };
    let mut paths: Vec<Vec<String>> = Vec::new();
    // Each task's words that are no repository path, for the lead to check.
    let mut not_paths: Vec<(String, Vec<String>)> = Vec::new();
    for task in &list.tasks {
        let read = overlap::expand(&task.paths, &tracked);
        if !read.not_paths.is_empty() {
            not_paths.push((task.id.clone(), read.not_paths));
        }
        paths.push(read.paths);
    }
    let overlap = overlap::overlap(&list, &paths);
    // The feature folder names every path, so it is never a mention.
    let skip = relative_dir(&root, &folder);
    let searched: Vec<String> = tracked
        .into_iter()
        .filter(|p| skip.as_ref().is_none_or(|dir| !p.starts_with(dir.as_str())))
        .collect();
    let found = mentions::Mentions::new(mentions::read_files(&root, &searched));
    let drafts = drafts(&list, &paths, &found);
    say(&render::render(&render::Report {
        tasks_path: &shown,
        list: &list,
        overlap: &overlap,
        unread: &found.unread,
        not_paths: &not_paths,
        drafts: &drafts,
    }));
    ExitCode::SUCCESS
}

/// One draft per story in story order, then setup and polish: the group's
/// task paths, then the files that mention them.
fn drafts(
    list: &parse::TaskList,
    paths: &[Vec<String>],
    found: &mentions::Mentions,
) -> Vec<render::Draft> {
    let mut groups: BTreeMap<Option<u32>, Vec<String>> = BTreeMap::new();
    for (task, paths) in list.tasks.iter().zip(paths) {
        let group = groups.entry(task.story).or_default();
        for p in paths {
            if !group.contains(p) {
                group.push(p.clone());
            }
        }
    }
    // `None` sorts first; setup and polish goes last.
    let mut order: Vec<Option<u32>> = groups.keys().copied().flatten().map(Some).collect();
    if groups.contains_key(&None) {
        order.push(None);
    }
    order
        .into_iter()
        .map(|key| {
            let group = groups.get(&key).cloned().unwrap_or_default();
            let mut files: Vec<mentions::Found> = group
                .iter()
                .map(|p| mentions::Found {
                    path: p.clone(),
                    reason: "task path".to_owned(),
                })
                .collect();
            files.extend(found.of(&group));
            render::Draft {
                name: key.map_or_else(|| "Setup and polish".to_owned(), |n| format!("US{n}")),
                files,
            }
        })
        .collect()
}

/// The folder that holds `tasks.md`, as given.
fn feature_folder(tasks: &Path) -> PathBuf {
    match tasks.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// `folder` relative to `root`, with a trailing `/`, when it is inside it.
fn relative_dir(root: &Path, folder: &Path) -> Option<String> {
    let root = root.canonicalize().ok()?;
    let folder = folder.canonicalize().ok()?;
    let rel = folder.strip_prefix(&root).ok()?;
    let rel = rel.to_str()?;
    (!rel.is_empty()).then(|| format!("{rel}/"))
}

/// The git top level of `dir`.
fn git_top_level(platform: &dyn Platform, dir: &Path) -> Result<PathBuf, String> {
    let cmd = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .cwd(dir.to_path_buf());
    let out = platform
        .run(&cmd, GIT_TIMEOUT)
        .map_err(|e| format!("could not run git in {}: {e}", dir.display()))?;
    let top = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if out.timed_out || out.exit_code != 0 || top.is_empty() {
        return Err(format!(
            "{} is not in a git repository; name one with --repo <dir>",
            dir.display()
        ));
    }
    Ok(PathBuf::from(top))
}
