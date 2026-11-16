//! ADR 0049 rule A: a dotted module name counts as a mention only on a
//! line that does not import it. Most files that name a module only import
//! it, and listing each one buried the files the lead needed.

use super::reason::token_starts;

/// True when `text` holds `module` as a whole token on at least one line
/// that is not a Python import of it. The whole line decides, so a comment
/// after the import on that line does not count. A comment, a doc, a
/// string or a line of an import linter config on its own line counts.
pub(crate) fn named(text: &str, module: &str) -> bool {
    token_starts(text, module, |_| false).any(|at| !imports(line_at(text, at), module))
}

/// The line of `text` that holds the byte at `at`.
fn line_at(text: &str, at: usize) -> &str {
    let head = text.get(..at).unwrap_or("");
    let start = head.rfind('\n').map_or(0, |i| i + 1);
    let rest = text.get(at..).unwrap_or("");
    let end = rest.find('\n').map_or(text.len(), |i| at + i);
    text.get(start..end).unwrap_or("")
}

/// True when `line`, after leading white space, is a Python import of
/// `module` or of a module under it: `import M`, `import M as m`,
/// `import os, M.sub`, `from M import X`, or `from P import N` where `P.N`
/// is `M`. A line that starts with `from M` and has no `import` after it,
/// such as prose, is no import.
fn imports(line: &str, module: &str) -> bool {
    let names = |word: &str| {
        word.strip_prefix(module)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
    };
    // The first word of each item of an import list, before a comment.
    let items = |list: &str| -> Vec<String> {
        list.split('#')
            .next()
            .unwrap_or("")
            .split(',')
            .filter_map(|item| {
                item.trim_matches(|c: char| c.is_whitespace() || c == '(' || c == ')')
                    .split_whitespace()
                    .next()
            })
            .map(str::to_owned)
            .collect()
    };
    let line = line.trim();
    let after = |keyword: &str| {
        line.strip_prefix(keyword)
            .filter(|rest| rest.starts_with([' ', '\t']))
    };
    if let Some(rest) = after("from") {
        let rest = rest.trim_start();
        let (from, tail) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        let Some(list) = tail
            .trim_start()
            .strip_prefix("import")
            .filter(|r| r.is_empty() || r.starts_with([' ', '\t', '(']))
        else {
            return false;
        };
        return names(from) || items(list).iter().any(|n| names(&format!("{from}.{n}")));
    }
    after("import").is_some_and(|rest| items(rest).iter().any(|n| names(n)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: &str = "shared.db.enums";

    /// The whole line decides: a comment after the import on the same line
    /// is part of an import line, so it is no mention either. The code
    /// block in a markdown file is an import line too.
    #[test]
    fn an_import_of_the_module_is_not_a_mention() {
        for text in [
            "from shared.db.enums import X  # see shared.db.enums for values\n",
            "import shared.db.enums  # shared.db.enums\n",
            "import shared.db.enums.sub\n",
            "from shared.db.enums import (X,\n    Y)\n",
            "```python\nfrom shared.db.enums import X\n```\n",
            "from shared.db import enums  # shared.db.enums\n",
            "from shared.db import (os, enums as e)  # shared.db.enums\n",
            "from shared.db.enums import ReportVisibility\n",
            "    from shared.db.enums import (\n        A,\n    )\n",
            "import shared.db.enums\n",
            "import shared.db.enums as e\n",
            "import os, shared.db.enums\r\n",
            "from shared.db.enums.sub import Y\n",
        ] {
            assert!(!named(text, M), "{text}");
        }
    }

    /// A comment, a doc line, a string and an import linter line name the
    /// module. So does a line that starts with `from` but is no import.
    #[test]
    fn a_line_that_does_not_import_the_module_is_a_mention() {
        for text in [
            "from shared.db.enums import X\n# see shared.db.enums for the values\n",
            "Values live in `shared.db.enums`.\n",
            "MOD = \"shared.db.enums\"\n",
            "layers =\n    shared.db.enums\n",
            "from shared.db.enums you get the values\n",
            "import_module(shared.db.enums)\n",
            "import os  # see shared.db.enums\n",
        ] {
            assert!(named(text, M), "{text}");
        }
    }
}
