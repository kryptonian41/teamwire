//! The `plugins` field of the team spec.

use teamwire_core::TeamSpec;

fn parse(extra_team: &str, extra_member: &str) -> TeamSpec {
    let json = format!(
        r#"{{"team":"tw","title":"t"{extra_team},"children":[{{"role":"r","title":"R","mode":"pane"{extra_member}}}]}}"#
    );
    serde_json::from_str(&json).unwrap()
}

#[test]
fn plugins_parse_on_team_and_member() {
    let s = parse(r#","plugins":{"a@b":false}"#, r#","plugins":{"c@d":true}"#);
    assert_eq!(s.plugins.get("a@b"), Some(&false));
    assert_eq!(s.children[0].plugins.get("c@d"), Some(&true));
}

#[test]
fn plugins_default_to_empty() {
    let s = parse("", "");
    assert!(s.plugins.is_empty());
    assert!(s.children[0].plugins.is_empty());
}
