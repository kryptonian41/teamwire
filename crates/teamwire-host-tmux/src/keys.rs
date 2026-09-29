//! tmux's spelling of `Key`, and the `send-keys` argument list for each.

use teamwire_core::Key;

/// The `send-keys` arguments (after the pane) for one key. A printable
/// character is sent literally with `-l`, so `;` or a word tmux knows as a
/// key name is typed as itself.
pub(crate) fn send_args(key: Key) -> Vec<String> {
    let named = |name: &str| vec![name.to_owned()];
    match key {
        Key::Enter => named("Enter"),
        Key::Escape => named("Escape"),
        Key::Tab => named("Tab"),
        Key::Backspace => named("BSpace"),
        Key::Up => named("Up"),
        Key::Down => named("Down"),
        Key::Left => named("Left"),
        Key::Right => named("Right"),
        Key::CtrlC => named("C-c"),
        // tmux reads a lone `;` argument as the end of one command, so it
        // is sent as `\;`, which tmux turns back into `;`.
        Key::Char(';') => vec!["-l".to_owned(), "--".to_owned(), "\\;".to_owned()],
        Key::Char(c) => vec!["-l".to_owned(), "--".to_owned(), c.to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_keys_use_tmux_names() {
        assert_eq!(send_args(Key::Enter), ["Enter"]);
        assert_eq!(send_args(Key::Backspace), ["BSpace"]);
        assert_eq!(send_args(Key::CtrlC), ["C-c"]);
    }

    #[test]
    fn a_character_is_literal_even_when_it_looks_like_an_option() {
        assert_eq!(send_args(Key::Char('-')), ["-l", "--", "-"]);
        assert_eq!(send_args(Key::Char('1')), ["-l", "--", "1"]);
    }

    #[test]
    fn a_semicolon_is_escaped_so_tmux_does_not_end_the_command() {
        assert_eq!(send_args(Key::Char(';')), ["-l", "--", "\\;"]);
    }
}
