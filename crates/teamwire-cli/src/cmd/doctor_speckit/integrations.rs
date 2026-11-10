//! The integration lines of the spec-kit checks: the spec-kit version and
//! each installed integration's command folder, both read from the
//! committed `.specify/integration.json`.

use std::path::Path;

use super::{Base, INTEGRATION_FILE};
use crate::cmd::doctor::Check;

/// The spec-kit release teamwire's guidance was tested with, as spec-kit
/// itself prints it, and its commit.
const TESTED_VERSION: &str = "1.3.3.dev0";
const TESTED_COMMIT: &str = "c438494";

/// teamwire's spec-kit guidance names commands and file formats of the 1.3
/// line; another release may rename them.
pub(super) fn version_check(base: &Base<'_>) -> Check {
    let name = "spec-kit version";
    let at = base.at();
    let tested = format!(
        "teamwire's spec-kit guidance was tested with {TESTED_VERSION} (spec-kit commit \
         {TESTED_COMMIT})"
    );
    let upgrade = format!(
        "with spec-kit 1.3 installed (`specify --version`), run `specify integration upgrade \
         <key>` for each integration, then commit on {}",
        base.name
    );
    let version = match base.show(INTEGRATION_FILE) {
        Ok(Some(text)) => serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|json| json.get("version")?.as_str().map(str::to_owned)),
        Ok(None) => None,
        Err(e) => return Check::warn(name, e),
    };
    match version {
        Some(v) if v == "1.3" || v.starts_with("1.3.") => {
            Check::pass(name, format!("{v} in {at}; {tested}"))
        }
        Some(v) => Check::warn(
            name,
            format!(
                "{INTEGRATION_FILE} in {at} records {v}, outside the 1.3 line; {tested}, and \
                 another release may rename its commands or files. To match the guidance, \
                 {upgrade}"
            ),
        ),
        None => Check::warn(
            name,
            format!(
                "{INTEGRATION_FILE} in {at} records no version, so doctor cannot tell whether \
                 the guidance fits it; {tested}. Fix {INTEGRATION_FILE} as the spec-kit \
                 integrations line says"
            ),
        ),
    }
}

/// Where each integration teamwire can run keeps its spec-kit commands: a
/// folder, and the start and end of an entry's name in it. Another
/// integration is listed without a folder check.
const COMMAND_FOLDERS: [(&str, &str, &str, &str); 3] = [
    ("claude", ".claude/skills", "speckit-", ""),
    ("codex", ".agents/skills", "speckit-", ""),
    ("pi", ".pi/prompts", "speckit.", ".md"),
];

fn command_folder(integration: &str) -> Option<(&'static str, &'static str, &'static str)> {
    COMMAND_FOLDERS
        .iter()
        .find(|(key, ..)| *key == integration)
        .map(|&(_, dir, start, end)| (dir, start, end))
}

/// Every command folder, whichever integration is installed.
pub(super) fn command_folders() -> impl Iterator<Item = (&'static str, &'static str, &'static str)>
{
    COMMAND_FOLDERS
        .iter()
        .map(|&(_, dir, start, end)| (dir, start, end))
}

/// True when the commit holds an entry of `dir` whose name has the start and
/// end: a file, or a folder of files.
fn has_entry(base: &Base<'_>, dir: &str, start: &str, end: &str) -> bool {
    base.under(dir).any(|rest| {
        let entry = rest.split('/').next().unwrap_or(rest);
        entry.starts_with(start) && entry.ends_with(end)
    })
}

/// The integrations spec-kit counts as installed: `installed_integrations`,
/// and the older keys `default_integration` and `integration`, the way
/// spec-kit's `normalize_integration_state` reads them.
pub(super) fn installed_integrations(json: &serde_json::Value) -> Vec<String> {
    let key = |name: &str| {
        json.get(name)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .map(str::to_owned)
    };
    let mut installed: Vec<String> = Vec::new();
    for k in json
        .get("installed_integrations")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|k| !k.is_empty())
    {
        if !installed.iter().any(|i| i == k) {
            installed.push(k.to_owned());
        }
    }
    if let Some(default) = key("default_integration").or_else(|| key("integration"))
        && !installed.contains(&default)
    {
        installed.insert(0, default);
    }
    installed
}

/// True when the folder on disk holds an entry whose name has the start and end.
fn on_disk(dir: &Path, start: &str, end: &str) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.starts_with(start) && n.ends_with(end))
        })
    })
}

pub(super) fn integrations_check(base: &Base<'_>) -> Check {
    let name = "spec-kit integrations";
    let file = INTEGRATION_FILE;
    let at = base.at();
    // `specify integration install` refuses a broken file and writes a new one in its place.
    let repair = format!(
        "run `specify integration install <key>` for each integration members use, then commit \
         on {}",
        base.name
    );
    let text = match base.show(file) {
        Ok(Some(text)) => text,
        Ok(None) if base.root.join(file).is_file() => {
            return Check::warn(
                name,
                format!(
                    "{file} is on disk but not in {at}; commit it on {}",
                    base.name
                ),
            );
        }
        Ok(None) => return Check::warn(name, format!("{file} is not in {at}; {repair}")),
        Err(e) => return Check::warn(name, e),
    };
    let installed = match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(json) => installed_integrations(&json),
        Err(e) => {
            return Check::warn(
                name,
                format!("could not read {file} in {at}: {e}; delete it, {repair}"),
            );
        }
    };
    let missing: Vec<String> = installed
        .iter()
        .filter_map(|key| {
            let (dir, start, end) = command_folder(key)?;
            (!has_entry(base, dir, start, end)).then(|| {
                if on_disk(&base.root.join(dir), start, end) {
                    format!(
                        "{key} has no {dir}/{start}*{end} in {at}, though the folder is on disk; \
                         commit it as the spec-kit committed line says"
                    )
                } else {
                    format!(
                        "{key} has no {dir}/{start}*{end} in {at}; run `specify integration \
                         upgrade {key}`, then commit the folder on {}",
                        base.name
                    )
                }
            })
        })
        .collect();
    let list = if installed.is_empty() {
        "none".to_owned()
    } else {
        installed.join(", ")
    };
    if missing.is_empty() {
        Check::pass(name, format!("installed in {at}: {list}"))
    } else {
        Check::warn(
            name,
            format!("{file} in {at} lists {list}, but {}", missing.join("; ")),
        )
    }
}
