//! ADR 0049 rule A: a dotted module name counts as a mention only on a
//! line that does not import it. Most files that name a module only import
//! it, and listing each one buried the files the lead needed.

use super::mentions::token_starts;

/// True when `text` holds `module` as a whole token on at least one line
/// that is not a Python import of it. A comment, a doc, a string or a line
/// of an import linter config counts.
pub(crate) fn named(text: &str, module: &str) -> bool {
    token_starts(text, module, |_| false).any(|at| !imports(text, at, module.len()))
}

/// True when the token at `at..at + len` is the module a Python import line
/// imports: `import M`, `import M as m`, `import os, M` or
/// `from M import X`. A token followed by `.sub` is no whole token, so
/// `from M.sub import Y` never reaches this check.
fn imports(text: &str, at: usize, len: usize) -> bool {
    let start = text
        .get(..at)
        .and_then(|head| head.rfind('\n'))
        .map_or(0, |i| i + 1);
    let head = text.get(start..at).unwrap_or("").trim_start();
    let rest = text.get(at + len..).unwrap_or("");
    let rest = rest.split('\n').next().unwrap_or("").trim_start();
    if let Some(gap) = head.strip_prefix("from") {
        let keyword = rest.strip_prefix("import");
        return !gap.is_empty()
            && gap.trim().is_empty()
            && keyword.is_some_and(|r| r.is_empty() || r.starts_with([' ', '\t', '(', '\r']));
    }
    head.strip_prefix("import").is_some_and(|gap| {
        let listed = gap.trim();
        gap.starts_with(char::is_whitespace) && (listed.is_empty() || listed.ends_with(','))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: &str = "shared.db.enums";

    #[test]
    fn an_import_of_the_module_is_not_a_mention() {
        for text in [
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
        ] {
            assert!(named(text, M), "{text}");
        }
    }
}
