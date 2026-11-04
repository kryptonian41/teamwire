//! Reading a team spec from JSON.

use teamwire_core::{Role, TaskClass, TeamName, TeamSpec};

use crate::SpecError;

/// Parses a spec. A missing `title` is an empty one. Keys that start with `_` are ignored, and so is every
/// other key the spec does not use; `unknown_key_warnings` names those. A
/// bad team name or role is reported with the role as written, and every
/// such problem is reported together.
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
        let role = child.get("role").and_then(serde_json::Value::as_str);
        if let Some(error) = task_class_error(role, child.get("task_class")) {
            errors.push(error);
        }
        let Some(role) = role else {
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
    let refs = value.get("refs").and_then(serde_json::Value::as_array);
    for r in refs.into_iter().flatten() {
        if let Some(why) = ref_shape_problem(r) {
            errors.push(SpecError::new(None, Some("refs"), "bad_ref", why));
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

/// Why `r` is not `{"provider": "linear", "id": "GKA-580"}`: serde would say
/// only "invalid type", and a bare string is the mistake people will make.
fn ref_shape_problem(r: &serde_json::Value) -> Option<String> {
    let Some(map) = r.as_object() else {
        return Some(format!(
            "ref {r} must be an object naming its tracker, such as \
             {{\"provider\": \"linear\", \"id\": \"GKA-580\"}}; teamwire assumes no provider"
        ));
    };
    for key in map.keys() {
        if key != "provider" && key != "id" {
            return Some(format!(
                "ref {r} has an unknown field {key:?}; a ref has only provider and id"
            ));
        }
    }
    for key in ["provider", "id"] {
        if !map.get(key).is_some_and(serde_json::Value::is_string) {
            return Some(format!("ref {r} needs a text field {key:?}"));
        }
    }
    None
}

/// The refusal for a `task_class` that is not one of the seven classes. Read
/// from the JSON before serde, so the refusal names the values a lead may
/// write instead of serde's message about an enum. `null` means unset.
fn task_class_error(role: Option<&str>, value: Option<&serde_json::Value>) -> Option<SpecError> {
    let value = value.filter(|v| !v.is_null())?;
    if value
        .as_str()
        .is_some_and(|text| TaskClass::parse(text).is_ok())
    {
        return None;
    }
    let classes: Vec<&str> = TaskClass::ALL.iter().map(|c| c.as_str()).collect();
    let who = role.map_or_else(|| "a member".to_owned(), |r| format!("role {r}"));
    Some(SpecError::new(
        role,
        Some("task_class"),
        "task_class_unknown",
        format!(
            "{who}: task_class {value} is not a task class; use one of {}",
            classes.join(", ")
        ),
    ))
}

/// The keys `TeamSpec` reads. A test keeps this list equal to the type.
const TEAM_KEYS: [&str; 9] = [
    "team",
    "title",
    "setup",
    "harness",
    "host",
    "children",
    "plugins",
    "context_handoff_tokens",
    "refs",
];

/// The keys `MemberSpec` reads. A test keeps this list equal to the type.
const MEMBER_KEYS: [&str; 22] = [
    "role",
    "title",
    "kind",
    "mode",
    "harness",
    "repo",
    "branch",
    "base",
    "cwd",
    "model",
    "effort",
    "task_class",
    "check",
    "owns",
    "task",
    "task_file",
    "deadline_s",
    "setup",
    "dispatch",
    "accept_trust",
    "plugins",
    "context_handoff_tokens",
];

/// One warning per key the spec does not use, naming the role (or the team
/// level) and the key. A misspelled key such as `acept_trust` would
/// otherwise launch on the default without a word. Keys that start with `_`
/// are notes and get no warning. Text that is not a JSON object gives none:
/// `parse_spec` refuses it.
pub fn unknown_key_warnings(text: &str) -> Vec<String> {
    let Ok(serde_json::Value::Object(team)) = serde_json::from_str(text) else {
        return Vec::new();
    };
    let unknown = |map: &serde_json::Map<String, serde_json::Value>, known: &[&str]| {
        map.keys()
            .filter(|k| !k.starts_with('_') && !known.contains(&k.as_str()))
            .cloned()
            .collect::<Vec<_>>()
    };
    let mut warnings: Vec<String> = unknown(&team, &TEAM_KEYS)
        .into_iter()
        .map(|k| format!("team level: unknown key {k:?} is ignored"))
        .collect();
    let children = team.get("children").and_then(serde_json::Value::as_array);
    for (i, child) in children.into_iter().flatten().enumerate() {
        let Some(map) = child.as_object() else {
            continue;
        };
        let who = map
            .get("role")
            .and_then(serde_json::Value::as_str)
            .map_or_else(|| format!("child {}", i + 1), |r| format!("role {r}"));
        warnings.extend(
            unknown(map, &MEMBER_KEYS)
                .into_iter()
                .map(|k| format!("{who}: unknown key {k:?} is ignored")),
        );
    }
    warnings
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

    /// The keys a value of the type serializes to, which are the keys serde reads.
    fn keys_of(value: impl serde::Serialize) -> Vec<String> {
        let mut keys: Vec<String> = serde_json::to_value(value)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    }

    fn sorted(keys: &[&str]) -> Vec<String> {
        let mut keys: Vec<String> = keys.iter().map(|k| (*k).to_owned()).collect();
        keys.sort();
        keys
    }

    #[test]
    fn the_known_keys_are_the_spec_types_fields() {
        let spec = parse_spec(
            r#"{"team":"demo","title":"T","children":[{"role":"api","title":"A","mode":"pane"}]}"#,
        )
        .unwrap();
        assert_eq!(keys_of(&spec), sorted(&TEAM_KEYS));
        assert_eq!(
            keys_of(spec.children.first().unwrap()),
            sorted(&MEMBER_KEYS)
        );
    }

    #[test]
    fn a_misspelled_key_warns_once_and_the_spec_launches_on_the_default() {
        for (key, value) in [
            ("acept_trust", "false"),
            ("bsae", r#""release-1""#),
            ("chek", r#""cargo test""#),
        ] {
            let text = format!(
                r#"{{"team":"demo","title":"T","children":[
                    {{"role":"api","title":"A","mode":"pane","task":"t","{key}":{value}}}]}}"#
            );
            let spec = parse_spec(&text).unwrap();
            let child = spec.children.first().unwrap();
            assert!(child.accept_trust, "{key}");
            assert_eq!(child.base, None, "{key}");
            assert_eq!(child.check, None, "{key}");
            assert_eq!(
                unknown_key_warnings(&text),
                [format!("role api: unknown key {key:?} is ignored")]
            );
        }
    }

    #[test]
    fn team_level_keys_are_warned_about_and_notes_are_not() {
        let text = r#"{"team":"demo","title":"T","_why":"x","titel":"y","children":[
            {"title":"A","mode":"pane","_note":1,"tsak":"t"}]}"#;
        assert_eq!(
            unknown_key_warnings(text),
            [
                r#"team level: unknown key "titel" is ignored"#,
                r#"child 1: unknown key "tsak" is ignored"#,
            ]
        );
        assert!(unknown_key_warnings("{").is_empty());
    }

    #[test]
    fn a_task_class_is_a_known_key_and_parses() {
        let text = r#"{"team":"demo","title":"T","children":[
            {"role":"api","title":"A","mode":"pane","task":"t","task_class":"root_cause"},
            {"role":"web","title":"W","mode":"pane","task":"t","task_class":null}]}"#;
        assert!(unknown_key_warnings(text).is_empty());
        let spec = parse_spec(text).unwrap();
        let classes: Vec<_> = spec.children.iter().map(|c| c.task_class).collect();
        assert_eq!(classes, [Some(TaskClass::RootCause), None]);
    }

    #[test]
    fn an_unknown_task_class_is_refused_naming_the_seven_classes() {
        for value in [r#""hard""#, "3"] {
            let text = format!(
                r#"{{"team":"demo","title":"T","children":[
                    {{"role":"api","title":"A","mode":"pane","task":"t","task_class":{value}}}]}}"#
            );
            let errors = parse_spec(&text).unwrap_err();
            assert_eq!(errors.len(), 1, "{errors:?}");
            let error = errors.first().unwrap();
            assert_eq!(error.code, "task_class_unknown");
            assert_eq!(error.role.as_deref(), Some("api"));
            assert_eq!(error.field.as_deref(), Some("task_class"));
            assert!(error.message.contains(value), "{}", error.message);
            for class in TaskClass::ALL {
                assert!(error.message.contains(class.as_str()), "{}", error.message);
            }
        }
    }

    #[test]
    fn broken_json_is_one_error() {
        let errors = parse_spec("{").unwrap_err();
        assert_eq!(errors.first().map(|e| e.code.as_str()), Some("bad_json"));
    }
}
