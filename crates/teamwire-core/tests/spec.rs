//! The `plugins` field of the team spec.

use teamwire_core::TeamSpec;

fn json(extra_team: &str, extra_member: &str) -> String {
    format!(
        r#"{{"team":"tw","title":"t"{extra_team},"children":[{{"role":"r","title":"R","mode":"pane"{extra_member}}}]}}"#
    )
}

#[test]
fn plugins_parse_on_team_and_member() {
    let s: TeamSpec = serde_json::from_str(&json(
        r#","plugins":{"a@b":false}"#,
        r#","plugins":{"c@d":true}"#,
    ))
    .unwrap();
    assert_eq!(s.plugins.get("a@b"), Some(&false));
    let member = s.children.first().unwrap();
    assert_eq!(member.plugins.get("c@d"), Some(&true));
}

#[test]
fn plugins_default_to_empty() {
    let s: TeamSpec = serde_json::from_str(&json("", "")).unwrap();
    assert!(s.plugins.is_empty());
    assert!(s.children.iter().all(|m| m.plugins.is_empty()));
}
