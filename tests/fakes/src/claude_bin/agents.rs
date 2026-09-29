//! `claude agents --json`.

use std::path::Path;
use std::process::ExitCode;

use serde_json::json;

use crate::fsutil;

/// `claude agents --json`: a JSON array of the sessions whose files are in
/// `<dir>/sessions`. Starts no session. As in Claude Code, an interactive row
/// carries its state under `status` and a background row (`kind` `bg`) under
/// `state`. A test adds a background row by writing a session file with
/// `"kind": "bg"` (see `FakeHerdrEnv::add_session_file`).
pub(super) fn run(claude_dir: &Path, argv: &[String]) -> ExitCode {
    if !argv.iter().any(|a| a == "--json") {
        return super::refuse("only `agents --json` is implemented");
    }
    let mut rows = Vec::new();
    let entries = std::fs::read_dir(claude_dir.join("sessions"))
        .into_iter()
        .flatten();
    for path in entries.filter_map(|e| e.ok().map(|e| e.path())) {
        let Some(doc) = fsutil::read_text(&path)
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        else {
            continue;
        };
        let text = |k: &str| doc.get(k).cloned().unwrap_or(serde_json::Value::Null);
        let background = doc.get("kind").and_then(serde_json::Value::as_str) == Some("bg");
        let mut row = json!({
            "id": doc.get("jobId").cloned().unwrap_or_else(|| text("sessionId")),
            "pid": text("pid"),
            "cwd": text("cwd"),
            "kind": text("kind"),
            "startedAt": text("startedAt"),
            "sessionId": text("sessionId"),
            "name": text("name"),
        });
        if let Some(map) = row.as_object_mut() {
            map.insert(
                if background { "state" } else { "status" }.to_owned(),
                text("status"),
            );
        }
        rows.push(row);
    }
    super::say(&serde_json::Value::Array(rows).to_string());
    ExitCode::SUCCESS
}
