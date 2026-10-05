//! Reading a team spec from JSON.

use teamwire_core::{Role, TeamName, TeamSpec};

use crate::SpecError;

/// Parses a spec. A missing `title` is an empty one. Keys that start with `_` are ignored, and so is every
/// other key the spec does not use. A bad team name or role is reported
/// with the role as written, and every such problem is reported together.
pub fn parse_spec(text: &str) -> Result<TeamSpec, Vec<SpecError>> {
    let mut value: serde_json::Value = serde_json::from_str(text).map_err(|e| {
        vec![SpecError::new(
            None,
            None,
            "bad_json",
            format!("the spec is not valid JSON: {e}"),
        )]
    })?;
    let mut errors = Vec::new();
    if let Some(Err(e)) = value
        .get("team")
        .and_then(serde_json::Value::as_str)
        .map(TeamName::parse)
    {
        errors.push(SpecError::new(
            None,
            Some("team"),
            "bad_name",
            e.to_string(),
        ));
    }
    let children = value.get("children").and_then(serde_json::Value::as_array);
    for child in children.into_iter().flatten() {
        let Some(role) = child.get("role").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if let Err(e) = Role::parse(role) {
            errors.push(SpecError::new(
                Some(role),
                Some("role"),
                "bad_name",
                e.to_string(),
            ));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    // A missing team title reads as an empty one. `validate` takes the title
    // from the team row on `--add` and refuses an empty one on a first launch.
    if let Some(map) = value.as_object_mut() {
        map.entry("title").or_insert_with(|| serde_json::json!(""));
    }
    serde_json::from_value(value).map_err(|e| {
        vec![SpecError::new(
            None,
            None,
            "bad_spec",
            format!("the spec is not a team spec: {e}"),
        )]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn underscore_keys_are_ignored() {
        let spec = parse_spec(
            r#"{"team":"demo","title":"T","_why":"x","children":[
                {"role":"api","title":"A","mode":"pane","_note":1,"cwd":"/tmp","task":"t"}]}"#,
        )
        .unwrap();
        assert_eq!(spec.children.len(), 1);
    }

    #[test]
    fn hyphenated_team_and_role_are_reported_together() {
        let errors = parse_spec(
            r#"{"team":"my-team","title":"T","children":[
                {"role":"a-b","title":"A","mode":"pane","task":"t"}]}"#,
        )
        .unwrap_err();
        assert_eq!(errors.len(), 2);
        assert_eq!(errors.get(1).and_then(|e| e.role.as_deref()), Some("a-b"));
        assert!(errors.iter().all(|e| e.code == "bad_name"));
    }

    #[test]
    fn broken_json_is_one_error() {
        let errors = parse_spec("{").unwrap_err();
        assert_eq!(errors.first().map(|e| e.code.as_str()), Some("bad_json"));
    }
}
