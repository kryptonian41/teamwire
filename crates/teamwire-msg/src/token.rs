//! Where a member's token comes from (ADR 0023 point 4).

/// `TEAMWIRE_TOKEN`, else the contents of the file `TEAMWIRE_TOKEN_FILE`
/// names, with trailing whitespace removed. A harness that cannot put the
/// token in a session's environment without putting it in argv (Codex)
/// passes the file's path instead. An unreadable or empty file is no token.
pub fn caller_token(get: impl Fn(&str) -> Option<String>) -> Option<String> {
    if let Some(token) = get("TEAMWIRE_TOKEN").filter(|t| !t.is_empty()) {
        return Some(token);
    }
    let path = get("TEAMWIRE_TOKEN_FILE").filter(|p| !p.is_empty())?;
    let text = std::fs::read_to_string(path).ok()?;
    let token = text.trim_end();
    (!token.is_empty()).then(|| token.to_owned())
}

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    reason = "the tests write the token file a launch would have written"
)]
mod tests {
    use super::*;

    fn lookup(pairs: &[(&str, String)]) -> impl Fn(&str) -> Option<String> + use<> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect();
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
        }
    }

    #[test]
    fn the_variable_wins_over_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("token");
        std::fs::write(&file, "from-file\n").unwrap();
        let path = file.display().to_string();
        let both = lookup(&[
            ("TEAMWIRE_TOKEN", "from-env".into()),
            ("TEAMWIRE_TOKEN_FILE", path),
        ]);
        assert_eq!(caller_token(both).as_deref(), Some("from-env"));
    }

    #[test]
    fn the_file_is_read_and_its_newline_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("token");
        std::fs::write(&file, "abc123\n").unwrap();
        let get = lookup(&[("TEAMWIRE_TOKEN_FILE", file.display().to_string())]);
        assert_eq!(caller_token(get).as_deref(), Some("abc123"));
    }

    #[test]
    fn a_missing_or_empty_file_is_no_token() {
        let dir = tempfile::tempdir().unwrap();
        let empty = dir.path().join("empty");
        std::fs::write(&empty, "\n").unwrap();
        for path in [dir.path().join("gone"), empty] {
            let get = lookup(&[("TEAMWIRE_TOKEN_FILE", path.display().to_string())]);
            assert_eq!(caller_token(get), None);
        }
        assert_eq!(caller_token(lookup(&[])), None);
    }
}
