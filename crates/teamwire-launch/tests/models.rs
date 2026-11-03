//! A member's task class, effort and model, checked against the model list
//! its harness gives. Only an unknown class, an unknown effort and a
//! provider the list lacks are refused; everything else warns.

#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
#[cfg(test)]
#[path = "support/common.rs"]
mod common;

use std::sync::Arc;

use common::{Rig, codes, pane_member, team_spec, with};
use serde_json::{Value, json};
use teamwire_core::{
    CatalogSource, Harness, HarnessError, HarnessErrorCode, HarnessId, ModelCatalog, ModelInfo,
};
use teamwire_fakes::FakeHarness;

fn row(provider: Option<&str>, id: &str, aliases: &[&str]) -> ModelInfo {
    ModelInfo {
        id: id.to_owned(),
        aliases: aliases.iter().map(|a| (*a).to_owned()).collect(),
        provider: provider.map(str::to_owned),
        context_tokens: None,
        thinking: true,
    }
}

/// A list shaped like Claude Code's: no providers, three levels.
fn builtin() -> ModelCatalog {
    ModelCatalog {
        source: CatalogSource::Builtin,
        models: vec![row(None, "claude-sonnet-5-5", &["sonnet"])],
        efforts: vec!["low".into(), "medium".into(), "high".into()],
    }
}

/// A list shaped like pi's: one model through two providers.
fn listed() -> ModelCatalog {
    ModelCatalog {
        source: CatalogSource::Listed,
        models: vec![
            row(Some("claude-bridge"), "claude-opus-5-5", &[]),
            row(Some("amazon-bedrock"), "claude-opus-5-5", &[]),
            row(Some("claude-bridge"), "claude-sonnet-5-5", &[]),
        ],
        efforts: vec!["off".into(), "low".into(), "high".into()],
    }
}

/// A classed pane member with `fields` added.
fn member(rig: &Rig, role: &str, fields: &Value) -> Value {
    let mut m = with(
        pane_member(role, role, rig.work.path()),
        "task_class",
        json!("contained_feature"),
    );
    for (key, value) in fields.as_object().unwrap() {
        m = with(m, key, value.clone());
    }
    m
}

fn warnings_of(rig: &Rig, children: &[Value]) -> Vec<String> {
    rig.validate(&team_spec("demo", children), false)
        .unwrap()
        .warnings
}

#[test]
fn a_member_without_a_class_warns_once_and_still_launches() {
    let rig = Rig::new();
    let plain = pane_member("a", "a", rig.work.path());
    let classed = member(&rig, "b", &json!({}));
    let warnings = warnings_of(&rig, &[plain, classed]);
    assert_eq!(
        warnings,
        ["member a has no task_class; outcomes for it cannot be read per class"]
    );
}

#[test]
fn an_unknown_class_is_refused() {
    let rig = Rig::new();
    let bad = with(
        pane_member("a", "a", rig.work.path()),
        "task_class",
        json!("hard_stuff"),
    );
    let errors = rig.validate(&team_spec("demo", &[bad]), false).unwrap_err();
    assert_eq!(codes(&errors), ["task_class_unknown"]);
    assert!(rig.home_is_empty());
}

#[test]
fn an_effort_the_harness_does_not_take_is_refused_naming_its_levels() {
    let rig = Rig::new();
    rig.harness.set_models(Ok(builtin()));
    let spec = team_spec(
        "demo",
        &[
            member(&rig, "a", &json!({"effort": "ultra"})),
            member(&rig, "b", &json!({"effort": "high"})),
        ],
    );
    let errors = rig.validate(&spec, false).unwrap_err();
    assert_eq!(codes(&errors), ["effort_unknown"]);
    let error = errors.first().unwrap();
    assert_eq!(error.role.as_deref(), Some("a"));
    assert!(
        error.message.contains("harness fake") && error.message.contains("low, medium, high"),
        "{}",
        error.message
    );
    assert!(rig.home_is_empty());
}

#[test]
fn a_model_the_list_cannot_match_warns_and_still_launches() {
    let rig = Rig::new();
    rig.harness.set_models(Ok(builtin()));
    let warnings = warnings_of(&rig, &[member(&rig, "a", &json!({"model": "gpt-9"}))]);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    let warning = warnings.first().unwrap();
    assert!(
        warning.contains("harness fake")
            && warning.contains("\"gpt-9\"")
            && warning.contains("`teamwire models --harness fake` lists what it can reach"),
        "{warning}"
    );
}

#[test]
fn a_pi_style_pattern_that_matches_gets_no_warning() {
    let rig = Rig::new();
    rig.harness.set_models(Ok(builtin()));
    for model in ["sonnet:high", "claude-sonnet-5-5", "sonnet"] {
        let warnings = warnings_of(
            &rig,
            &[member(&rig, "a", &json!({"model": model, "effort": "low"}))],
        );
        assert!(warnings.is_empty(), "{model}: {warnings:?}");
    }
}

#[test]
fn a_provider_prefix_the_list_lacks_is_refused() {
    let rig = Rig::new();
    rig.harness.set_models(Ok(listed()));
    let spec = team_spec(
        "demo",
        &[member(
            &rig,
            "a",
            &json!({"model": "openai/claude-opus-5-5"}),
        )],
    );
    let errors = rig.validate(&spec, false).unwrap_err();
    assert_eq!(codes(&errors), ["model_provider_unavailable"]);
    let message = &errors.first().unwrap().message;
    assert!(
        message.contains("provider openai") && message.contains("amazon-bedrock, claude-bridge"),
        "{message}"
    );
}

#[test]
fn a_provider_prefix_the_list_has_launches_without_a_warning() {
    let rig = Rig::new();
    rig.harness.set_models(Ok(listed()));
    let warnings = warnings_of(
        &rig,
        &[member(
            &rig,
            "a",
            &json!({"model": "claude-bridge/claude-opus-5-5:high"}),
        )],
    );
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn a_bare_model_two_providers_offer_warns_naming_them() {
    let rig = Rig::new();
    rig.harness.set_models(Ok(listed()));
    let warnings = warnings_of(
        &rig,
        &[
            member(&rig, "a", &json!({"model": "claude-opus-5-5"})),
            member(&rig, "b", &json!({"model": "claude-sonnet-5-5"})),
        ],
    );
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    let warning = warnings.first().unwrap();
    assert!(
        warning.starts_with("role a:")
            && warning.contains("amazon-bedrock, claude-bridge")
            && warning.contains("write provider/id"),
        "{warning}"
    );
}

#[test]
fn a_failing_list_warns_once_and_refuses_nothing() {
    let rig = Rig::new();
    rig.harness.set_models(Err(HarnessError::new(
        HarnessErrorCode::Timeout,
        "pi --list-models ran past 10 s",
    )));
    let warnings = warnings_of(
        &rig,
        &[
            member(&rig, "a", &json!({"model": "nonsense", "effort": "ultra"})),
            member(&rig, "b", &json!({"model": "openai/x"})),
        ],
    );
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    let warning = warnings.first().unwrap();
    assert!(
        warning.contains("harness fake") && warning.contains("ran past 10 s"),
        "{warning}"
    );
}

#[test]
fn a_harness_without_a_list_warns_only_when_there_is_something_to_check() {
    let rig = Rig::new();
    assert!(warnings_of(&rig, &[member(&rig, "a", &json!({}))]).is_empty());
    assert_eq!(rig.harness.log().count_named("models"), 0);
    let warnings = warnings_of(&rig, &[member(&rig, "a", &json!({"effort": "ultra"}))]);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings.first().unwrap().contains("unsupported"),
        "{warnings:?}"
    );
}

#[test]
fn each_harness_is_asked_for_its_list_once() {
    let mut rig = Rig::new();
    let other = Arc::new(FakeHarness::with_id(HarnessId::from_static("fakeb")));
    rig.registry
        .add_harness(Arc::clone(&other) as Arc<dyn Harness>)
        .unwrap();
    rig.harness.set_models(Ok(builtin()));
    other.set_models(Ok(builtin()));
    let on_other = json!({"model": "sonnet", "harness": "fakeb"});
    warnings_of(
        &rig,
        &[
            member(&rig, "a", &json!({"model": "sonnet"})),
            member(&rig, "b", &json!({"effort": "low"})),
            member(&rig, "c", &json!({"model": "sonnet", "effort": "high"})),
            member(&rig, "d", &on_other),
            member(&rig, "e", &on_other),
        ],
    );
    assert_eq!(rig.harness.log().count_named("models"), 1);
    assert_eq!(other.log().count_named("models"), 1);
}
