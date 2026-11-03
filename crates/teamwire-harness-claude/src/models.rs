//! The models Claude Code can start a member on.
//!
//! Claude Code has no command that lists its models, so the adapter carries
//! a fixed list. A model on it may still be out of reach for this account,
//! and the aliases resolve to other versions on other providers.

use teamwire_core::{CatalogSource, ModelCatalog, ModelInfo};

/// The values of `claude --effort`, lowest first (`claude --help`, Claude
/// Code 2.1.317).
const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

const ONE_MILLION: u64 = 1_000_000;
const TWO_HUNDRED_THOUSAND: u64 = 200_000;

// The aliases and ids come from Claude Code's "Model configuration" page,
// the table of model aliases and the table of effort levels:
// https://code.claude.com/docs/en/model-config
// Read on 2026-11-03. Refresh the list when that page names a new model.
//
// Left out on purpose: `default`, `best` and `opusplan`, which are modes or
// depend on the account rather than naming one model, and the `[1m]`
// suffix. Every model here takes the whole `low` to `max` range except
// Opus 4.6 and Sonnet 4.6, which lack `xhigh`; Claude Code runs a missing
// level as the highest level below it.
//
// Each entry is the id, its alias if it has one, and its context window.
const MODELS: [(&str, Option<&str>, u64); 11] = [
    ("claude-fable-5-1", Some("fable"), ONE_MILLION),
    ("claude-fable-5", None, ONE_MILLION),
    ("claude-opus-5-5", Some("opus"), ONE_MILLION),
    ("claude-opus-5", None, ONE_MILLION),
    ("claude-opus-4-8", None, ONE_MILLION),
    ("claude-opus-4-7", None, ONE_MILLION),
    ("claude-opus-4-6", None, TWO_HUNDRED_THOUSAND),
    ("claude-sonnet-5-5", Some("sonnet"), ONE_MILLION),
    ("claude-sonnet-5", None, ONE_MILLION),
    ("claude-sonnet-4-6", None, TWO_HUNDRED_THOUSAND),
    ("claude-haiku-5-5", Some("haiku"), ONE_MILLION),
];

pub(crate) fn catalog() -> ModelCatalog {
    ModelCatalog {
        source: CatalogSource::Builtin,
        models: MODELS
            .iter()
            .map(|(id, alias, context)| ModelInfo {
                id: (*id).to_owned(),
                aliases: alias.iter().map(|a| (*a).to_owned()).collect(),
                provider: None,
                context_tokens: Some(*context),
                thinking: true,
            })
            .collect(),
        efforts: EFFORTS.map(str::to_owned).to_vec(),
    }
}
