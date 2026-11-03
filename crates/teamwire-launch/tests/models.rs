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
use teamwire_launch::SpecError;

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

/// pi's list as pi 1.5.0 printed it on this project's machine: the rows of
/// the pi adapter's recorded fixture, then the rows `pi --list-models opus`
/// printed, recorded in `support/pi-list-models-opus.txt`. pi spells one
/// model differently per provider: claude-bridge lists `claude-opus-5-5`,
/// Amazon Bedrock lists `us.anthropic.claude-opus-5-5` and other forms.
/// `bedrock: false` leaves Bedrock out, as `TEAMWIRE_PI_EXCLUDE_PROVIDERS`
/// does.
fn real_pi(bedrock: bool) -> ModelCatalog {
    let tables = [
        include_str!("../../teamwire-harness-pi/tests/fixtures/list-models.txt"),
        include_str!("support/pi-list-models-opus.txt"),
    ];
    let mut models: Vec<ModelInfo> = Vec::new();
    for line in tables.iter().flat_map(|t| t.lines().skip(1)) {
        let cells: Vec<&str> = line.split_whitespace().collect();
        let &[provider, id, _, _, thinking, ..] = cells.as_slice() else {
            continue;
        };
        let known = models
            .iter()
            .any(|m| m.id == id && m.provider.as_deref() == Some(provider));
        if known || (!bedrock && provider == "amazon-bedrock") {
            continue;
        }
        let mut info = row(Some(provider), id, &[]);
        info.thinking = thinking == "yes";
        models.push(info);
    }
    ModelCatalog {
        source: CatalogSource::Listed,
        models,
        efforts: ["off", "minimal", "low", "medium", "high", "xhigh", "max"]
            .map(String::from)
            .to_vec(),
    }
}

/// A classed pane member with `fields` added.
fn member(rig: &Rig, role: &str, fields: &Value) -> Value {
    let mut m = with(
        pane_member(role, role, rig.work.path()),
        "task_class",
        json!("contained_feature"),
    );
    for (key, value) in fields.as_object().into_iter().flatten() {
        m = with(m, key, value.clone());
    }
    m
}

fn warnings_of(rig: &Rig, children: &[Value]) -> Result<Vec<String>, Vec<SpecError>> {
    rig.validate(&team_spec("demo", children), false)
        .map(|plan| plan.warnings)
}

#[test]
fn a_member_without_a_class_warns_once_and_still_launches() {
    let rig = Rig::new();
    let plain = pane_member("a", "a", rig.work.path());
    let classed = member(&rig, "b", &json!({}));
    let warnings = warnings_of(&rig, &[plain, classed]).unwrap();
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
    let warnings = warnings_of(&rig, &[member(&rig, "a", &json!({"model": "gpt-9"}))]).unwrap();
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
        )
        .unwrap();
        assert!(warnings.is_empty(), "{model}: {warnings:?}");
    }
}

#[test]
fn a_provider_prefix_the_list_lacks_is_refused() {
    let rig = Rig::new();
    rig.harness.set_models(Ok(real_pi(true)));
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
    rig.harness.set_models(Ok(real_pi(true)));
    let warnings = warnings_of(
        &rig,
        &[member(
            &rig,
            "a",
            &json!({"model": "claude-bridge/claude-opus-5-5:high"}),
        )],
    )
    .unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn a_bare_model_two_providers_offer_warns_naming_them() {
    let rig = Rig::new();
    rig.harness.set_models(Ok(real_pi(true)));
    let warnings = warnings_of(
        &rig,
        &[
            member(&rig, "a", &json!({"model": "claude-opus-5-5"})),
            // A value that names its provider gets no warning.
            member(
                &rig,
                "b",
                &json!({"model": "claude-bridge/claude-sonnet-4-6"}),
            ),
        ],
    )
    .unwrap();
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
fn a_bare_opus_on_the_real_pi_list_warns_naming_the_providers() {
    for (bedrock, providers) in [
        (true, "amazon-bedrock, claude-bridge"),
        (false, "claude-bridge"),
    ] {
        let rig = Rig::new();
        rig.harness.set_models(Ok(real_pi(bedrock)));
        let warnings = warnings_of(&rig, &[member(&rig, "a", &json!({"model": "opus"}))]).unwrap();
        assert_eq!(warnings.len(), 1, "{bedrock}: {warnings:?}");
        let warning = warnings.first().unwrap();
        assert!(
            warning.starts_with("role a:")
                && warning.contains(&format!("offer a match are {providers};"))
                && warning.contains("picks the provider itself")
                && warning.contains("TEAMWIRE_PI_EXCLUDE_PROVIDERS")
                && warning.contains("write provider/id")
                && !warning.contains("does not list"),
            "{bedrock}: {warning}"
        );
    }
}

#[test]
fn sonnet_and_opus_on_pi_are_found_and_a_name_pi_lacks_is_not() {
    let rig = Rig::new();
    rig.harness.set_models(Ok(real_pi(false)));
    for model in ["sonnet", "opus", "sonnet:high", "claude-opus-5-5"] {
        let warnings = warnings_of(&rig, &[member(&rig, "a", &json!({"model": model}))]).unwrap();
        assert!(
            warnings.iter().all(|w| !w.contains("does not list")),
            "{model}: {warnings:?}"
        );
    }
    let warnings = warnings_of(&rig, &[member(&rig, "a", &json!({"model": "gpt-9"}))]).unwrap();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings
            .first()
            .unwrap()
            .contains("does not list model \"gpt-9\""),
        "{warnings:?}"
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
    )
    .unwrap();
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
    assert!(
        warnings_of(&rig, &[member(&rig, "a", &json!({}))])
            .unwrap()
            .is_empty()
    );
    assert_eq!(rig.harness.log().count_named("models"), 0);
    let warnings = warnings_of(&rig, &[member(&rig, "a", &json!({"effort": "ultra"}))]).unwrap();
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
    )
    .unwrap();
    assert_eq!(rig.harness.log().count_named("models"), 1);
    assert_eq!(other.log().count_named("models"), 1);
}
