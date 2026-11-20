//! The family of each model `teamwire models` lists: the vendor line, such
//! as Claude or GPT, beside the model in text and as `family` in JSON
//! (ADR 0056).
//!
//! Never: read a file or print. The CLI tests compile this file in beside
//! `view.rs`, so it names nothing of this crate but its sibling `view`.
//!
//! Invariants: the harness places each model (`Harness::model_family`),
//! from the value a spec's `model` takes for it, never the bare id. pi's
//! `openrouter/openai/o3` is GPT by its vendor part, and the bare `o3`
//! names no vendor.

use serde_json::{Value, json};
use teamwire_core::{Harness, ModelCatalog, ModelFamily};

use super::view::spec_value;

/// The family of each model of `catalog`, in the catalog's order.
pub(crate) fn families(harness: &dyn Harness, catalog: &ModelCatalog) -> Vec<ModelFamily> {
    catalog
        .models
        .iter()
        .map(|m| harness.model_family(&spec_value(m)))
        .collect()
}

/// `catalog` as JSON, with each model object's `family` beside its fields.
/// The family is worked out here, not stored in `ModelInfo`, so no harness
/// that builds a `ModelInfo` has to know it.
pub(crate) fn catalog_json(catalog: &ModelCatalog, families: &[ModelFamily]) -> Value {
    let mut doc = json!(catalog);
    if let Some(Value::Array(models)) = doc.get_mut("models") {
        for (model, family) in models.iter_mut().zip(families) {
            if let Value::Object(fields) = model {
                fields.insert("family".into(), json!(family.as_str()));
            }
        }
    }
    doc
}
