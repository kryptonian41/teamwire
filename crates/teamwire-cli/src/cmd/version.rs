//! `teamwire version`: the binary's version and its schema version.

use std::process::ExitCode;

use crate::Global;
use crate::wiring::Wiring;

/// The two lines `version` prints.
fn text() -> String {
    format!(
        "teamwire {}\nschema {}\n",
        env!("CARGO_PKG_VERSION"),
        teamwire_store::SCHEMA_VERSION
    )
}

#[expect(clippy::print_stdout, reason = "the version is the command's output")]
pub(crate) fn run(_global: &Global, _wiring: &Wiring) -> ExitCode {
    print!("{}", text());
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_binary_and_the_schema_it_writes() {
        let text = text();
        assert!(text.starts_with(&format!("teamwire {}\n", env!("CARGO_PKG_VERSION"))));
        assert!(text.ends_with(&format!("schema {}\n", teamwire_store::SCHEMA_VERSION)));
    }
}
