//! The fixed model list of Claude Code: every alias resolves, and the
//! efforts are the ones `claude --effort` takes.

use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{CatalogSource, Harness, HarnessError, ModelCatalog};
use teamwire_fakes::FakePlatform;
use teamwire_harness_claude::{ClaudeCode, ClaudeConfig};

fn catalog() -> Result<ModelCatalog, HarnessError> {
    ClaudeCode::new(
        Arc::new(FakePlatform::new()),
        ClaudeConfig {
            claude_dir: "/nowhere/claude".into(),
            claude_json: "/nowhere/claude.json".into(),
            claude_bin: "claude".into(),
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
fn every_alias_of_the_docs_resolves_to_its_current_model() {
    let catalog = catalog().unwrap();
    for (alias, id) in [
        ("fable", "claude-fable-5-1"),
        ("opus", "claude-opus-5-5"),
        ("sonnet", "claude-sonnet-5-5"),
        ("haiku", "claude-haiku-5-5"),
    ] {
        assert_eq!(catalog.find(alias).map(|m| m.id.as_str()), Some(id));
    }
}

#[test]
fn every_listed_name_resolves_through_find() {
    let catalog = catalog().unwrap();
    for model in &catalog.models {
        for name in std::iter::once(&model.id).chain(&model.aliases) {
            assert_eq!(catalog.find(name).map(|m| &m.id), Some(&model.id), "{name}");
        }
    }
}

#[test]
fn no_name_is_listed_twice() {
    let catalog = catalog().unwrap();
    let mut names: Vec<&String> = catalog
        .models
        .iter()
        .flat_map(|m| std::iter::once(&m.id).chain(&m.aliases))
        .collect();
    let all = names.len();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), all);
}

#[test]
fn the_efforts_are_the_ones_the_effort_flag_takes() {
    assert_eq!(
        catalog().unwrap().efforts,
        ["low", "medium", "high", "xhigh", "max"]
    );
}

#[test]
fn a_mode_or_an_unlisted_name_does_not_resolve() {
    let catalog = catalog().unwrap();
    for name in ["default", "best", "opusplan", "gpt-6.1-sol", ""] {
        assert!(catalog.find(name).is_none(), "{name}");
    }
}
