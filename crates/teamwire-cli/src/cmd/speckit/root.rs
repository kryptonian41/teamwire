//! ADR 0049 rule B: a file at the repository root, such as `AGENTS.md`, is
//! named only by a relative link that resolves to the root. Its path is its
//! bare name, and a bare name, `@AGENTS.md` or `apps/ai/AGENTS.md` most
//! often means a nested file of that name.

use super::mentions::{ends_token, joins};

/// The first relative link in `text`, the text of the tracked file `file`,
/// that names the root file `name`, as written, such as `../../AGENTS.md`:
/// one or more `./` or `../` segments right before the name, which lead
/// from the folder of `file` to the root.
pub(crate) fn link(file: &str, text: &str, name: &str) -> Option<String> {
    let depth = file.matches('/').count();
    text.match_indices(name).find_map(|(at, _)| {
        let head = text.get(..at)?;
        // The token before the name, back to its start.
        let start = head
            .char_indices()
            .rev()
            .find(|(_, c)| !joins(*c))
            .map_or(0, |(i, c)| i + c.len_utf8());
        let prefix = head.get(start..)?;
        let rest = text.get(at + name.len()..)?;
        (ends_token(rest) && reaches_root(prefix, depth)).then(|| format!("{prefix}{name}"))
    })
}

/// True when `prefix` is one or more `./` or `../` segments that lead from
/// a folder `depth` levels below the root to the root, never above it.
fn reaches_root(prefix: &str, depth: usize) -> bool {
    let Some(segments) = prefix.strip_suffix('/') else {
        return false;
    };
    let mut level = depth;
    for segment in segments.split('/') {
        match segment {
            "." => {}
            ".." => match level.checked_sub(1) {
                Some(up) => level = up,
                None => return false,
            },
            _ => return false,
        }
    }
    level == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table of ADR 0049 rule B, for the root file `AGENTS.md`.
    #[test]
    fn only_a_relative_link_that_reaches_the_root_names_a_root_file() {
        let cases = [
            (
                "apps/ai/CLAUDE.md",
                "see ../../AGENTS.md",
                Some("../../AGENTS.md"),
            ),
            ("apps/ai/CLAUDE.md", "see ../AGENTS.md", None),
            ("README.md", "see ./AGENTS.md.", Some("./AGENTS.md")),
            ("apps/ai/CLAUDE.md", "see ./AGENTS.md", None),
            ("apps/ai/CLAUDE.md", "@AGENTS.md", None),
            ("docs/x.md", "see apps/ai/AGENTS.md", None),
            ("README.md", "see AGENTS.md", None),
            (
                "docs/x.md",
                "[rules](./../AGENTS.md)",
                Some("./../AGENTS.md"),
            ),
            ("docs/x.md", "see ../../AGENTS.md", None),
            ("docs/x.md", "see x/../AGENTS.md", None),
            ("docs/x.md", "see ../AGENTS.mdx and ../MY_AGENTS.md", None),
            (
                "docs/x.md",
                "AGENTS.md, then ../AGENTS.md",
                Some("../AGENTS.md"),
            ),
        ];
        for (file, text, want) in cases {
            assert_eq!(
                link(file, text, "AGENTS.md").as_deref(),
                want,
                "{file}: {text}"
            );
        }
    }
}
