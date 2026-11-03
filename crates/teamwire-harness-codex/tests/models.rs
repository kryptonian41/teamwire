//! The fixed model list of Codex: every name resolves, and the efforts are
//! the values of `model_reasoning_effort`.

use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{CatalogSource, Harness, HarnessError, ModelCatalog};
use teamwire_fakes::FakePlatform;
use teamwire_harness_codex::{CodexCli, CodexConfig};

fn catalog() -> Result<ModelCatalog, HarnessError> {
    CodexCli::new(
        Arc::new(FakePlatform::new()),
        CodexConfig {
            codex_home: "/nowhere/codex".into(),
            codex_bin: "codex".into(),
        },
    )
    .models(Duration::from_secs(1))
}

#[test]
fn the_list_is_fixed_and_says_so() {
    let catalog = catalog().unwrap();
    assert_eq!(catalog.source, CatalogSource::Builtin);
    assert!(!catalog.models.is_empty());
}

#[test]
fn every_listed_name_resolves_through_find() {
    let catalog = catalog().unwrap();
    for model in &catalog.models {
        for name in std::iter::once(&model.id).chain(&model.aliases) {
            assert_eq!(catalog.find(name).map(|m| &m.id), Some(&model.id), "{name}");
        }
    }
    assert!(catalog.find("gpt-6.1-sol").is_some());
    assert!(catalog.find("sonnet").is_none());
}

#[test]
fn the_efforts_are_the_values_of_the_reasoning_effort_setting() {
    assert_eq!(
        catalog().unwrap().efforts,
        ["low", "medium", "high", "xhigh", "max", "ultra"]
    );
}
