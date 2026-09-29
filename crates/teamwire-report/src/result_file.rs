//! The result file `outbox/<role>.md`: what the lead reads when a member
//! finishes. teamwire owns it (INV-16): it starts with a marker, it is
//! written atomically before any delivery (REP-9), and a file at that path
//! without the marker is moved to the member's own directory first.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use teamwire_core::{FinalSource, Member, Timestamp, one_line};
use teamwire_store::{CompletionRecord, Ctx};

const MARKER_PREFIX: &str = "<!-- teamwire:generated";
const TASK_CHARS: usize = 300;

/// What was written.
pub(crate) struct Written {
    pub(crate) path: PathBuf,
    pub(crate) sha256: [u8; 32],
}

/// Everything the file shows.
pub(crate) struct Rendered<'a> {
    pub(crate) team: &'a str,
    pub(crate) team_dir: &'a Path,
    pub(crate) member: &'a Member,
    pub(crate) record: &'a CompletionRecord,
    pub(crate) final_message: Option<&'a str>,
    pub(crate) final_source: FinalSource,
    pub(crate) report: &'a Path,
    pub(crate) now: Timestamp,
}

pub(crate) fn path_of(team_dir: &Path, member: &Member) -> PathBuf {
    team_dir
        .join("outbox")
        .join(format!("{}.md", member.role.as_str()))
}

/// Writes the result file, first moving an unmarked file out of the way.
pub(crate) fn write(ctx: &Ctx<'_>, r: &Rendered<'_>) -> std::result::Result<Written, String> {
    let path = path_of(r.team_dir, r.member);
    let mut rescued = None;
    if let Ok(existing) = std::fs::read(&path)
        && !existing.starts_with(MARKER_PREFIX.as_bytes())
    {
        let dir = r.team_dir.join("reports");
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let target = dir.join(format!(
            "{}.rescued-{}.md",
            r.member.role.as_str(),
            compact(r.now)
        ));
        ctx.platform
            .write_atomic(&target, &existing)
            .map_err(|e| format!("could not keep the file found at {}: {e}", path.display()))?;
        rescued = Some(target);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let bytes = render(r, rescued.as_deref()).into_bytes();
    ctx.platform
        .write_atomic(&path, &bytes)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Written {
        path,
        sha256: Sha256::digest(&bytes).into(),
    })
}

/// `20260929T140311482Z`: a UTC time that is safe in a file name.
fn compact(at: Timestamp) -> String {
    at.to_string()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect()
}

fn render(r: &Rendered<'_>, rescued: Option<&Path>) -> String {
    let c = &r.record.completion;
    let task = std::fs::read_to_string(
        r.team_dir
            .join("briefs")
            .join(format!("{}.md", r.member.role)),
    )
    .map_or_else(
        |_| "(the brief is not available)".to_owned(),
        |t| one_line(&t, TASK_CHARS).into_string(),
    );
    let final_text = match (r.final_message, r.final_source) {
        (Some(text), _) => text.to_owned(),
        (None, _) => "(the final message was not available from this harness)".to_owned(),
    };
    let moved = rescued.map_or_else(String::new, |path| {
        format!(
            "- A file that was at this path before, moved to: {}\n",
            path.display()
        )
    });
    format!(
        "{MARKER_PREFIX} completion={} -->\n# Result from {} ({})\n\n\
         - Team: {}\n- Status: {}\n- Generation: {}\n- Completion: {}\n- Time: {}\n\
         - Final message from: {}\n- Member report: {}\n{moved}\
         \n## Summary\n\n{}\n\n## Task (first {TASK_CHARS} characters)\n\n{task}\n\n\
         ## Final message\n\n{final_text}\n",
        c.ulid,
        c.role,
        c.status,
        r.team,
        c.status,
        c.generation,
        c.n,
        c.created_at,
        r.final_source,
        r.report.display(),
        one_line(&c.summary, 2000),
    )
}
