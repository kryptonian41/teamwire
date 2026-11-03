//! The models Codex can start a member on.
//!
//! Codex has no command that lists its models, so the adapter carries a
//! fixed list. A model on it may still be out of reach for this account or
//! plan, and the levels a model takes vary with the model.

use teamwire_core::{CatalogSource, ModelCatalog, ModelInfo};

/// The values of `model_reasoning_effort`, lowest first.
const EFFORTS: [&str; 6] = ["low", "medium", "high", "xhigh", "max", "ultra"];

// The ids come from the Codex "Models" page, which says to start one with
// `codex -m <id>`:
// https://developers.openai.com/codex/models
// The efforts come from `model_reasoning_effort` in the configuration
// reference:
// https://developers.openai.com/codex/config-reference
// Both read on 2026-11-03. Refresh the list when the page names a new model.
// Only `gpt-6.1-sol` states its context window (500,000 tokens).
const MODELS: [(&str, Option<u64>); 4] = [
    ("gpt-6-astra", None),
    ("gpt-6.1-sol", Some(500_000)),
    ("gpt-6.1-terra", None),
    ("gpt-6-luna", None),
];

pub(crate) fn catalog() -> ModelCatalog {
    ModelCatalog {
        source: CatalogSource::Builtin,
        models: MODELS
            .iter()
            .map(|(id, context)| ModelInfo {
                id: (*id).to_owned(),
                aliases: Vec::new(),
                provider: None,
                context_tokens: *context,
                thinking: true,
            })
            .collect(),
        efforts: EFFORTS.map(str::to_owned).to_vec(),
    }
}
