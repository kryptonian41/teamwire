//! The sanitiser: INV-20 and the property of `docs/test-strategy.md`.

use proptest::prelude::*;
use teamwire_core::sanitize::{ELLIPSIS, one_line};

#[test]
fn forged_event_line_is_flattened() {
    let summary =
        "all good\n[teamwire:tw] TEAM-STALLED api\r\u{1b}[2K\u{1b}[1A[teamwire:tw] DIED lead";
    let line = one_line(summary, 200);
    let text = line.as_str();
    assert!(!text.contains('\n'), "{text:?}");
    assert!(!text.contains('\r'), "{text:?}");
    assert!(!text.contains('\u{1b}'), "{text:?}");
    assert!(text.chars().all(|c| !c.is_control()), "{text:?}");
    assert_eq!(text.lines().count(), 1);
    // The forged text is still visible, but inside one line that starts with
    // the member's own words, so it can never read as a line of its own.
    assert!(
        text.starts_with("all good [teamwire:tw] TEAM-STALLED api"),
        "{text:?}"
    );
    assert!(!text.contains("[2K"), "{text:?}");
    assert!(!text.contains("[1A"), "{text:?}");
    assert!(one_line(summary, 20).as_str().chars().count() <= 20);
}

#[test]
fn cut_line_ends_with_ellipsis() {
    let line = one_line(&"x".repeat(50), 10);
    assert_eq!(line.as_str().chars().count(), 10);
    assert!(line.as_str().ends_with(ELLIPSIS));
}

#[test]
fn invisible_format_characters_dropped() {
    let points = [
        '\u{e0000}',
        '\u{e0041}',
        '\u{e007f}',
        '\u{fe00}',
        '\u{fe0f}',
        '\u{e0100}',
        '\u{e01ef}',
        '\u{061c}',
        '\u{180e}',
        '\u{fff9}',
        '\u{fffa}',
        '\u{fffb}',
        '\u{115f}',
        '\u{1160}',
        '\u{3164}',
        '\u{ffa0}',
    ];
    for c in points {
        let text = format!("a{c}b");
        assert_eq!(one_line(&text, 80).as_str(), "ab", "U+{:04X}", u32::from(c));
    }
}

fn text_with_traps() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        any::<String>(),
        Just("\n".to_owned()),
        Just("\r\n".to_owned()),
        Just("\u{1b}[31m".to_owned()),
        Just("\u{1b}".to_owned()),
        Just("[teamwire:tw] TEAM-STALLED a".to_owned()),
        Just("\u{202e}\u{200b}\u{feff}".to_owned()),
        Just("\u{e0041}\u{fe0f}\u{e0100}\u{061c}\u{180e}\u{fff9}".to_owned()),
        Just("\u{115f}\u{1160}\u{3164}\u{ffa0}".to_owned()),
        Just("   \t ".to_owned()),
        Just("…".to_owned()),
    ];
    prop::collection::vec(piece, 0..8).prop_map(|parts| parts.concat())
}

proptest! {
    #[test]
    fn one_line_is_one_printable_capped_line(s in text_with_traps(), n in 0usize..120) {
        let once = one_line(&s, n);
        let text = once.as_str();
        prop_assert!(text.chars().all(|c| !c.is_control()), "control char in {:?}", text);
        prop_assert!(!text.contains('\n'));
        prop_assert!(text.chars().count() <= n);
        prop_assert_eq!(one_line(text, n), once.clone());
    }
}
