//! Argument parsing for the herdr command line, just enough for the
//! commands teamwire uses.

use std::collections::{BTreeMap, BTreeSet};

/// Flags that take a value. Any other `--flag` is a switch.
const VALUE_FLAGS: &[&str] = &[
    "kind",
    "pane",
    "timeout",
    "source",
    "lines",
    "workspace",
    "cwd",
    "label",
    "until",
    "direction",
    "tab",
    "format",
    "env",
];

#[derive(Debug, Default)]
pub(crate) struct Args {
    /// Words that are not flags: the command, then its operands.
    pub(crate) words: Vec<String>,
    values: BTreeMap<String, String>,
    switches: BTreeSet<String>,
    /// Everything after a bare `--`: the agent's own arguments.
    pub(crate) tail: Vec<String>,
}

impl Args {
    pub(crate) fn parse(argv: &[String]) -> Self {
        let mut out = Self::default();
        let mut iter = argv.iter();
        while let Some(arg) = iter.next() {
            if arg == "--" {
                out.tail = iter.by_ref().cloned().collect();
                break;
            }
            match arg.strip_prefix("--") {
                Some(flag) if VALUE_FLAGS.contains(&flag) => {
                    if let Some(value) = iter.next() {
                        out.values.insert(flag.to_owned(), value.clone());
                    }
                }
                Some(flag) => {
                    out.switches.insert(flag.to_owned());
                }
                None => out.words.push(arg.clone()),
            }
        }
        out
    }

    /// The two command words, such as `"agent start"`.
    pub(crate) fn command(&self) -> String {
        self.words
            .iter()
            .take(2)
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The operand after the command words, by position (0 is the first).
    pub(crate) fn operand(&self, n: usize) -> Option<&str> {
        self.words.get(2 + n).map(String::as_str)
    }

    pub(crate) fn value(&self, flag: &str) -> Option<&str> {
        self.values.get(flag).map(String::as_str)
    }

    /// The name of every flag the call used, value flags and switches.
    pub(crate) fn flag_names(&self) -> impl Iterator<Item = &str> {
        self.values
            .keys()
            .chain(self.switches.iter())
            .map(String::as_str)
    }

    pub(crate) fn has(&self, switch: &str) -> bool {
        self.switches.contains(switch)
    }

    pub(crate) fn millis(&self, flag: &str) -> Option<u64> {
        self.value(flag).and_then(|v| v.parse().ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Args {
        let argv: Vec<String> = text.split(' ').map(str::to_owned).collect();
        Args::parse(&argv)
    }

    #[test]
    fn splits_command_operands_flags_and_tail() {
        let a =
            parse("agent start tw_a_b --kind claude --pane w1:p1 --timeout 90000 -- -n tw_a_b --x");
        assert_eq!(a.command(), "agent start");
        assert_eq!(a.operand(0), Some("tw_a_b"));
        assert_eq!(a.value("kind"), Some("claude"));
        assert_eq!(a.millis("timeout"), Some(90000));
        assert_eq!(a.tail, ["-n", "tw_a_b", "--x"]);
    }

    #[test]
    fn switches_take_no_value() {
        let a = parse("agent prompt tw_a_b hello --wait --timeout 5");
        assert!(a.has("wait"));
        assert_eq!(a.operand(1), Some("hello"));
        assert_eq!(a.millis("timeout"), Some(5));
    }
}
