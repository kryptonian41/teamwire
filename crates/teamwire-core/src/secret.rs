//! Which values must never be printed (SEC-1), and a `Debug` helper that
//! hides them.

use std::fmt;

/// True for `TEAMWIRE_TOKEN`, `CLAUDE_CODE_MESSAGING_TOKEN`, and any name
/// holding `TOKEN`, `KEY` or `SECRET` in any case.
pub fn is_secret_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    ["TOKEN", "KEY", "SECRET"]
        .iter()
        .any(|word| upper.contains(word))
}

/// Prints `name=value` pairs with the value of every secret name replaced
/// by `<redacted>`.
pub struct RedactedEnv<'a>(pub &'a [(String, String)]);

impl fmt::Debug for RedactedEnv<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut list = f.debug_list();
        for (name, value) in self.0 {
            if is_secret_name(name) {
                list.entry(&format_args!("{name}=<redacted>"));
            } else {
                list.entry(&format_args!("{name}={value}"));
            }
        }
        list.finish()
    }
}
