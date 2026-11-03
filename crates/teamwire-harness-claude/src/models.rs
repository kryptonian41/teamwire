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
// Read on 2026-11-03 (again, for the 4.5 models). Refresh the list when that
// page names a new model.
//
// Left out on purpose: `default`, `best` and `opusplan`, which are modes or
// depend on the account rather than naming one model, and the `[1m]`
// suffix. The effort table on that page lists `low` to `max` for the 5.x
// models and Opus 4.7 and 4.8, and omits `xhigh` for Opus 4.6 and Sonnet 4.6;
// Claude Code runs a missing level as the highest level below it. The page
// says a model that table omits takes no effort, so the 4.5 models are
// `thinking: false`. The page names `claude-haiku-4-5` and
// `claude-sonnet-4-5` and gives the 1M window only to the models listed in
// its extended-context section, so the 4.5 models have 200K. `claude-opus-4-5`
// is not on the page; pi's claude-bridge list on this machine carries it.
//
// Each entry is the id, its alias if it has one, its context window, and
// whether it takes an effort level.
const MODELS: [(&str, Option<&str>, u64, bool); 14] = [
    ("claude-fable-5-1", Some("fable"), ONE_MILLION, true),
    ("claude-fable-5", None, ONE_MILLION, true),
    ("claude-opus-5-5", Some("opus"), ONE_MILLION, true),
    ("claude-opus-5", None, ONE_MILLION, true),
    ("claude-opus-4-8", None, ONE_MILLION, true),
    ("claude-opus-4-7", None, ONE_MILLION, true),
    ("claude-opus-4-6", None, TWO_HUNDRED_THOUSAND, true),
    ("claude-opus-4-5", None, TWO_HUNDRED_THOUSAND, false),
    ("claude-sonnet-5-5", Some("sonnet"), ONE_MILLION, true),
    ("claude-sonnet-5", None, ONE_MILLION, true),
    ("claude-sonnet-4-6", None, TWO_HUNDRED_THOUSAND, true),
    ("claude-sonnet-4-5", None, TWO_HUNDRED_THOUSAND, false),
    ("claude-haiku-5-5", Some("haiku"), ONE_MILLION, true),
    ("claude-haiku-4-5", None, TWO_HUNDRED_THOUSAND, false),
];

pub(crate) fn catalog() -> ModelCatalog {
    ModelCatalog {
        source: CatalogSource::Builtin,
        models: MODELS
            .iter()
            .map(|(id, alias, context, thinking)| ModelInfo {
                id: (*id).to_owned(),
                aliases: alias.iter().map(|a| (*a).to_owned()).collect(),
                provider: None,
                context_tokens: Some(*context),
                thinking: *thinking,
            })
            .collect(),
        efforts: EFFORTS.map(str::to_owned).to_vec(),
    }
}
