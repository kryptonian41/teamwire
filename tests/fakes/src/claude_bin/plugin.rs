//! `claude plugin ...` at user scope: local directory marketplaces, plugin
//! install, update, enable and uninstall, with the files Claude Code keeps
//! them in (`plugins/known_marketplaces.json`, `plugins/installed_plugins.json`
//! and `enabledPlugins` in `settings.json`, all under `<dir>`).
//!
//! Like Claude Code, install copies the plugin into `plugins/cache/` under its
//! `plugin.json` version, and `update` copies again only when the version
//! changed. A test therefore sees a stale copy unless teamwire runs `update`
//! and the plugin's version moved. `FAKE_CLAUDE_PLUGIN_FAIL=<command>`, for
//! example `install` or `marketplace add`, makes that command exit 1.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{Map, Value, json};

use crate::fsutil;

/// A refusal Claude Code itself makes: exit 1, the reason on stderr.
fn fail(message: &str) -> ExitCode {
    use std::io::Write as _;
    writeln!(std::io::stderr().lock(), "{message}").ok();
    ExitCode::from(1)
}

fn read(path: &Path) -> Value {
    fsutil::read_text(path)
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| json!({}))
}

fn write(path: &Path, doc: &Value) -> Result<(), ExitCode> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    fsutil::write_atomic(path, doc.to_string().as_bytes())
        .map_err(|e| super::refuse(&format!("cannot write {}: {e}", path.display())))
}

/// The object under `key` in `doc`, made when missing.
fn object<'a>(doc: &'a mut Value, key: &str) -> Option<&'a mut Map<String, Value>> {
    doc.as_object_mut()?
        .entry(key)
        .or_insert_with(|| json!({}))
        .as_object_mut()
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            #[expect(
                clippy::disallowed_methods,
                reason = "the fake copies a fixture tree into a temp dir; it is not a store file"
            )]
            let copied = std::fs::copy(entry.path(), target);
            copied?;
        }
    }
    Ok(())
}

/// Where a plugin's files are in its marketplace directory, and its version.
fn plugin_source(known: &Value, id: &str) -> Result<(PathBuf, String), ExitCode> {
    let (name, market) = id
        .split_once('@')
        .ok_or_else(|| fail(&format!("Plugin {id} must be written plugin@marketplace")))?;
    let dir = known
        .get(market)
        .and_then(|m| m.get("source")?.get("path")?.as_str())
        .map(PathBuf::from)
        .ok_or_else(|| fail(&format!("Marketplace {market} is not configured")))?;
    let listing = read(&dir.join(".claude-plugin/marketplace.json"));
    let source = listing
        .get("plugins")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|p| p.get("name").and_then(Value::as_str) == Some(name))
        .and_then(|p| p.get("source")?.as_str())
        .ok_or_else(|| fail(&format!("Plugin {name} not found in marketplace {market}")))?;
    let root = dir.join(source);
    let version = read(&root.join(".claude-plugin/plugin.json"))
        .get("version")
        .and_then(Value::as_str)
        .map_or_else(|| "unknown".to_owned(), str::to_owned);
    Ok((root, version))
}

/// The positional arguments: everything that is not a flag or a flag's value.
fn positional(argv: &[String]) -> Vec<&str> {
    let mut out = Vec::new();
    let mut skip = false;
    for a in argv {
        if skip {
            skip = false;
        } else if a == "--scope" {
            skip = true;
        } else if !a.starts_with('-') {
            out.push(a.as_str());
        }
    }
    out
}

/// Runs `claude plugin ...`. `claude_dir` is where the fake keeps Claude's
/// files; teamwire must name the same directory in `CLAUDE_CONFIG_DIR`.
pub(super) fn run(claude_dir: &Path, argv: &[String]) -> ExitCode {
    let rest = argv.get(1..).unwrap_or_default();
    if rest.iter().any(|a| a == "--help") {
        super::say("Usage: claude plugin|plugins [options] [command]");
        return ExitCode::SUCCESS;
    }
    let config_dir = std::env::var_os("CLAUDE_CONFIG_DIR");
    if config_dir.as_deref() != Some(claude_dir.as_os_str()) {
        return super::refuse(&format!(
            "CLAUDE_CONFIG_DIR is {config_dir:?}, not {}; the real claude would write elsewhere",
            claude_dir.display()
        ));
    }
    let (action, args) = match rest.split_first() {
        Some((m, tail)) if m == "marketplace" => match tail.split_first() {
            Some((verb, args)) => (format!("marketplace {verb}"), args),
            None => return super::refuse("no marketplace command"),
        },
        Some((verb, args)) => (verb.clone(), args),
        None => return super::refuse("no plugin command"),
    };
    if action != "marketplace update" && !rest.windows(2).any(|w| w == ["--scope", "user"]) {
        return super::refuse("only `--scope user` is implemented");
    }
    if std::env::var("FAKE_CLAUDE_PLUGIN_FAIL").ok().as_deref() == Some(action.as_str()) {
        return fail("FAKE_CLAUDE_PLUGIN_FAIL asked this command to fail");
    }
    let plugins = claude_dir.join("plugins");
    let known_path = plugins.join("known_marketplaces.json");
    let installed_path = plugins.join("installed_plugins.json");
    let settings_path = claude_dir.join("settings.json");
    let (mut known, mut installed, mut settings) = (
        read(&known_path),
        read(&installed_path),
        read(&settings_path),
    );
    let args = positional(args);
    let first = args.first().copied();

    let outcome = match (action.as_str(), first) {
        ("marketplace add", Some(dir)) => marketplace_add(&mut known, dir),
        ("marketplace update", name) => match name {
            Some(n) if known.get(n).is_none() => Err(fail(&format!("Marketplace {n} not found"))),
            _ => Ok(()),
        },
        ("marketplace remove", Some(name)) => {
            match known.as_object_mut().and_then(|o| o.remove(name)) {
                Some(_) => Ok(()),
                None => Err(fail(&format!("Marketplace {name} not found"))),
            }
        }
        ("install", Some(id)) => install(&known, &mut installed, &mut settings, claude_dir, id),
        ("update", Some(id)) => update(&known, &mut installed, claude_dir, id),
        ("enable" | "disable", Some(id)) => {
            toggle(&installed, &mut settings, id, action == "enable")
        }
        ("uninstall", Some(id)) => uninstall(&mut installed, &mut settings, id),
        _ => Err(super::refuse(&format!(
            "`plugin {action}` is not implemented"
        ))),
    };
    if let Err(code) = outcome {
        return code;
    }
    for (path, doc) in [
        (&known_path, &known),
        (&installed_path, &installed),
        (&settings_path, &settings),
    ] {
        if let Err(code) = write(path, doc) {
            return code;
        }
    }
    super::say(&format!("fake-claude: plugin {action} done"));
    ExitCode::SUCCESS
}

fn marketplace_add(known: &mut Value, dir: &str) -> Result<(), ExitCode> {
    let listing = read(&Path::new(dir).join(".claude-plugin/marketplace.json"));
    let Some(name) = listing.get("name").and_then(Value::as_str) else {
        return Err(fail(&format!(
            "{dir} has no .claude-plugin/marketplace.json"
        )));
    };
    let Some(map) = known.as_object_mut() else {
        return Err(super::refuse("known_marketplaces.json is not an object"));
    };
    if map.contains_key(name) {
        return Err(fail(&format!("Marketplace {name} already exists")));
    }
    map.insert(
        name.to_owned(),
        json!({"source": {"source": "directory", "path": dir}, "installLocation": dir}),
    );
    Ok(())
}

fn install(
    known: &Value,
    installed: &mut Value,
    settings: &mut Value,
    claude_dir: &Path,
    id: &str,
) -> Result<(), ExitCode> {
    let (root, version) = plugin_source(known, id)?;
    let Some(map) = object(installed, "plugins") else {
        return Err(super::refuse("installed_plugins.json is not an object"));
    };
    if map.contains_key(id) {
        return Err(fail(&format!("Plugin {id} is already installed")));
    }
    let copy = cached_copy(claude_dir, id, &version, &root)?;
    map.insert(
        id.to_owned(),
        json!([{"scope": "user", "installPath": copy, "version": version}]),
    );
    set_enabled(settings, id, true)
}

fn update(
    known: &Value,
    installed: &mut Value,
    claude_dir: &Path,
    id: &str,
) -> Result<(), ExitCode> {
    let (root, version) = plugin_source(known, id)?;
    let Some(entry) = object(installed, "plugins")
        .and_then(|m| m.get_mut(id))
        .and_then(|e| e.get_mut(0))
    else {
        return Err(fail(&format!("Plugin {id} is not installed")));
    };
    if entry.get("version").and_then(Value::as_str) == Some(version.as_str()) {
        super::say(&format!("{id} is already at the latest version"));
        return Ok(());
    }
    let copy = cached_copy(claude_dir, id, &version, &root)?;
    entry["installPath"] = json!(copy);
    entry["version"] = json!(version);
    Ok(())
}

fn cached_copy(
    claude_dir: &Path,
    id: &str,
    version: &str,
    from: &Path,
) -> Result<PathBuf, ExitCode> {
    let (name, market) = id.split_once('@').unwrap_or((id, id));
    let to = claude_dir
        .join("plugins/cache")
        .join(market)
        .join(name)
        .join(version);
    copy_tree(from, &to)
        .map_err(|e| super::refuse(&format!("cannot copy {}: {e}", from.display())))?;
    Ok(to)
}

fn set_enabled(settings: &mut Value, id: &str, on: bool) -> Result<(), ExitCode> {
    let Some(map) = object(settings, "enabledPlugins") else {
        return Err(super::refuse("settings.json is not an object"));
    };
    map.insert(id.to_owned(), json!(on));
    Ok(())
}

fn toggle(installed: &Value, settings: &mut Value, id: &str, on: bool) -> Result<(), ExitCode> {
    if installed.get("plugins").and_then(|p| p.get(id)).is_none() {
        return Err(fail(&format!("Plugin {id} is not installed")));
    }
    set_enabled(settings, id, on)
}

fn uninstall(installed: &mut Value, settings: &mut Value, id: &str) -> Result<(), ExitCode> {
    if object(installed, "plugins")
        .and_then(|m| m.remove(id))
        .is_none()
    {
        return Err(fail(&format!("Plugin {id} is not installed")));
    }
    if let Some(map) = object(settings, "enabledPlugins") {
        map.remove(id);
    }
    Ok(())
}
