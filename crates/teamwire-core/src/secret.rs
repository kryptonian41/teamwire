//! Which values must never be printed (SEC-1), and a `Debug` helper that
//! hides them.

use std::fmt;

/// True for `TEAMWIRE_TOKEN`, `CLAUDE_CODE_MESSAGING_TOKEN`, and any name
/// holding `TOKEN`, `KEY`, `SECRET`, `HEADERS`, `PASSWORD`, `PASSWD`,
/// `CREDENTIAL` or `AUTHORIZATION` in any case. `HEADERS` covers
/// `OTEL_EXPORTER_OTLP_HEADERS`, which carries credentials. Bare `AUTH` is not
/// listed because it would also hide `GIT_AUTHOR_NAME`.
pub fn is_secret_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    [
        "TOKEN",
        "KEY",
        "SECRET",
        "HEADERS",
        "PASSWORD",
        "PASSWD",
        "CREDENTIAL",
        "AUTHORIZATION",
    ]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_otlp_headers_value_is_redacted_in_debug_output() {
        let env = vec![(
            "OTEL_EXPORTER_OTLP_HEADERS".to_owned(),
            "authorization=Bearer abc123".to_owned(),
        )];
        let shown = format!("{:?}", RedactedEnv(&env));
        assert!(!shown.contains("abc123"), "{shown}");
        assert!(
            shown.contains("OTEL_EXPORTER_OTLP_HEADERS=<redacted>"),
            "{shown}"
        );
    }
}
