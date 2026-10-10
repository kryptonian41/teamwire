//! The lead's skill file: the text this binary carries, the hash stamp that
//! marks a copy teamwire wrote, and what an installed copy is.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::LeadSetupError;

/// The lead's skill, compiled in so each binary installs the skill that
/// matches its own commands.
pub const SKILL_TEXT: &str = include_str!("../../../../examples/skills/teamwire/SKILL.md");

/// The frontmatter key of the stamp. It sits under `metadata`, the map the
/// skill format leaves for a tool's own keys.
const STAMP_KEY: &str = "  teamwire-sha256: ";

/// What the installed skill file is, compared with `SKILL_TEXT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillState {
    Missing,
    /// This binary's skill, stamped.
    Current,
    /// teamwire wrote it and nobody edited it since, but it is another
    /// version, or this binary's text without the stamp.
    Outdated,
    /// Its text no longer matches its stamp, or it has no stamp and is not
    /// this binary's text: someone edited or wrote it by hand.
    Edited,
}

/// `<claude dir>/skills/teamwire/SKILL.md`.
pub(crate) fn path(claude_dir: &Path) -> PathBuf {
    claude_dir.join("skills").join("teamwire").join("SKILL.md")
}

fn sha256_hex(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

/// The byte offset of the line that closes the frontmatter, or `None` when
/// the text has no frontmatter.
fn frontmatter_end(text: &str) -> Option<usize> {
    const OPEN: &str = "---\n";
    const CLOSE: &str = "\n---\n";
    let rest = text.strip_prefix(OPEN)?;
    // `rest` starts after the opening line; the closing line starts one
    // byte into the `CLOSE` match, after its leading newline.
    rest.find(CLOSE).map(|i| OPEN.len() + i + 1)
}

/// `text` with a `metadata` block holding its own hash added as the last
/// frontmatter key. `None` when the text has no frontmatter.
pub(crate) fn stamp(text: &str) -> Option<String> {
    let end = frontmatter_end(text)?;
    let (head, tail) = text.split_at(end);
    Some(format!(
        "{head}metadata:\n{STAMP_KEY}{}\n{tail}",
        sha256_hex(text)
    ))
}

/// The text without its stamp, and the hash the stamp holds. `None` when
/// there is no stamp at the end of the frontmatter.
fn unstamp(text: &str) -> Option<(String, &str)> {
    let end = frontmatter_end(text)?;
    let head = text.get(..end)?;
    let start = head.rfind("\nmetadata:\n")? + 1;
    let block = head.get(start..)?;
    let hash = block
        .strip_prefix("metadata:\n")?
        .strip_prefix(STAMP_KEY)?
        .strip_suffix('\n')?;
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut body = String::with_capacity(text.len());
    body.push_str(text.get(..start)?);
    body.push_str(text.get(end..)?);
    Some((body, hash))
}

/// What `installed` is, given the text this binary would install.
pub(crate) fn classify(installed: &str, ours: &str) -> SkillState {
    match unstamp(installed) {
        Some((body, hash)) if sha256_hex(&body) == hash => {
            if body == ours {
                SkillState::Current
            } else {
                SkillState::Outdated
            }
        }
        Some(_) => SkillState::Edited,
        None if installed == ours => SkillState::Outdated,
        None => SkillState::Edited,
    }
}

/// The state of the skill file at `path`.
pub(crate) fn read(path: &Path, ours: &str) -> Result<SkillState, LeadSetupError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(classify(&text, ours)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(SkillState::Missing),
        Err(e) => Err(LeadSetupError::Read {
            path: path.to_path_buf(),
            detail: e.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "---\nname: x\ndescription: y\n---\n\n# Body\n";

    #[test]
    fn a_stamped_copy_is_current_and_unstamps_to_the_original() {
        let stamped = stamp(DOC).unwrap();
        assert!(
            stamped.starts_with("---\nname: x\ndescription: y\nmetadata:\n  teamwire-sha256: ")
        );
        assert!(stamped.ends_with("\n---\n\n# Body\n"));
        assert_eq!(unstamp(&stamped).unwrap().0, DOC);
        assert_eq!(classify(&stamped, DOC), SkillState::Current);
    }

    #[test]
    fn an_unedited_copy_of_another_version_is_outdated() {
        let old = stamp("---\nname: x\n---\nold body\n").unwrap();
        assert_eq!(classify(&old, DOC), SkillState::Outdated);
    }

    #[test]
    fn our_text_without_a_stamp_is_outdated_so_install_stamps_it() {
        assert_eq!(classify(DOC, DOC), SkillState::Outdated);
    }

    #[test]
    fn an_edit_after_install_or_a_hand_written_skill_is_edited() {
        let edited = stamp(DOC).unwrap().replace("# Body", "# My body");
        assert_eq!(classify(&edited, DOC), SkillState::Edited);
        assert_eq!(
            classify("---\nname: x\n---\nmine\n", DOC),
            SkillState::Edited
        );
        assert_eq!(classify("no frontmatter", DOC), SkillState::Edited);
    }

    #[test]
    fn the_shipped_skill_can_be_stamped_and_has_no_metadata_of_its_own() {
        // A `metadata` key in the shipped file would make the stamp a
        // duplicate YAML key.
        let head = SKILL_TEXT
            .get(..frontmatter_end(SKILL_TEXT).unwrap())
            .unwrap();
        assert!(!head.contains("\nmetadata:"));
        let stamped = stamp(SKILL_TEXT).unwrap();
        assert_eq!(classify(&stamped, SKILL_TEXT), SkillState::Current);
    }
}
