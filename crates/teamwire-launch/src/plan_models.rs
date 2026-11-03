//! Each member's task class, effort and model, checked against the model
//! list of the member's harness. Launch refuses only what is certainly
//! wrong: an effort the harness does not take, and a `provider/` prefix the
//! harness lists no model from. A model name the list cannot match only
//! warns, because a fixed list can lag the vendor and pi's patterns are
//! fuzzy. A harness that gives no list skips both checks with a warning, so
//! a slow or broken harness program never blocks a launch. Which rows a
//! `model` names is core's `ModelCatalog::matching`, so launch and
//! `teamwire models` read names by one rule.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use teamwire_core::{HarnessId, ModelCatalog, Registry};

use crate::{PlannedMember, SpecError};

/// How long a harness may take to list its models.
const MODELS_TIMEOUT: Duration = Duration::from_secs(10);

/// One warning per member without a task class, then the effort and model
/// checks. A harness is asked for its list once, and only when one of its
/// members sets a model or an effort: with neither there is nothing to
/// check, and pi's list runs a program.
pub(crate) fn model_checks(
    registry: &Registry,
    members: &[PlannedMember],
    errors: &mut Vec<SpecError>,
    warnings: &mut Vec<String>,
) {
    for m in members.iter().filter(|m| m.spec.task_class.is_none()) {
        warnings.push(format!(
            "member {} has no task_class; outcomes for it cannot be read per class",
            m.spec.role
        ));
    }
    let mut catalogs: BTreeMap<HarnessId, Option<ModelCatalog>> = BTreeMap::new();
    for m in members {
        if m.spec.model.is_none() && m.spec.effort.is_none() {
            continue;
        }
        let catalog = catalogs
            .entry(m.harness.clone())
            .or_insert_with(|| catalog_of(registry, &m.harness, warnings));
        if let Some(catalog) = catalog {
            check_effort(m, catalog, errors);
            check_model(m, catalog, errors, warnings);
        }
    }
}

/// The harness's model list, or `None` with one warning that says why.
fn catalog_of(
    registry: &Registry,
    id: &HarnessId,
    warnings: &mut Vec<String>,
) -> Option<ModelCatalog> {
    let listed = registry
        .harness(id)
        .map_err(|e| e.to_string())
        .and_then(|h| h.models(MODELS_TIMEOUT).map_err(|e| e.to_string()));
    match listed {
        Ok(catalog) => Some(catalog),
        Err(why) => {
            warnings.push(format!(
                "harness {id} gave no model list ({why}); launch did not check the model and \
                 effort of its members"
            ));
            None
        }
    }
}

/// Refuses an effort outside the harness's levels. A list with no levels
/// says nothing about effort, so it refuses nothing.
fn check_effort(m: &PlannedMember, catalog: &ModelCatalog, errors: &mut Vec<SpecError>) {
    let Some(effort) = m.spec.effort.as_deref() else {
        return;
    };
    if catalog.efforts.is_empty() || catalog.efforts.iter().any(|e| e == effort) {
        return;
    }
    let role = m.spec.role.as_str();
    errors.push(SpecError::new(
        Some(role),
        Some("effort"),
        "effort_unknown",
        format!(
            "role {role}: harness {} takes no effort {effort:?}; its levels are {}",
            m.harness,
            catalog.efforts.join(", ")
        ),
    ));
}

/// Warns about a model the list cannot match, and about one that names no
/// provider on a list that has providers, and refuses a `provider/` prefix
/// the list has no model from.
fn check_model(
    m: &PlannedMember,
    catalog: &ModelCatalog,
    errors: &mut Vec<SpecError>,
    warnings: &mut Vec<String>,
) {
    let Some(model) = m.spec.model.as_deref() else {
        return;
    };
    let role = m.spec.role.as_str();
    let harness = &m.harness;
    let providers = catalog.providers();
    let prefix = model.split_once('/').map(|(prefix, _)| prefix);
    let named = prefix.is_some_and(|p| providers.iter().any(|q| q.eq_ignore_ascii_case(p)));
    let rows = catalog.matching(model);
    if !rows.is_empty() {
        if providers.is_empty() || named {
            return;
        }
        // pi resolves a bare pattern to whichever provider it prefers, even
        // one teamwire's list leaves out, and a call there can fail.
        let offered: BTreeSet<&str> = rows.iter().filter_map(|r| r.provider.as_deref()).collect();
        warnings.push(format!(
            "role {role}: model {model:?} names no provider, so harness {harness} picks the \
             provider itself, and it may pick one that TEAMWIRE_PI_EXCLUDE_PROVIDERS leaves out \
             of teamwire's list. Of the listed providers, {} offer a match; write provider/id, \
             as `teamwire models --harness {harness}` prints it",
            offered.into_iter().collect::<Vec<_>>().join(", ")
        ));
        return;
    }
    if let Some(prefix) = prefix
        && !providers.is_empty()
        && !named
    {
        errors.push(SpecError::new(
            Some(role),
            Some("model"),
            "model_provider_unavailable",
            format!(
                "role {role}: harness {harness} lists no model from provider {prefix} for model \
                 {model:?}. A provider the user excluded, or one without a credential on this \
                 machine, is not listed. The listed providers are {}",
                providers.join(", ")
            ),
        ));
        return;
    }
    warnings.push(format!(
        "role {role}: harness {harness} does not list model {model:?}; \
         `teamwire models --harness {harness}` lists what it can reach"
    ));
}

#[cfg(test)]
mod tests {
    use teamwire_core::{CatalogSource, ModelInfo};

    use super::*;

    fn row(provider: Option<&str>, id: &str, aliases: &[&str]) -> ModelInfo {
        ModelInfo {
            id: id.to_owned(),
            aliases: aliases.iter().map(|a| (*a).to_owned()).collect(),
            provider: provider.map(str::to_owned),
            context_tokens: None,
            thinking: true,
        }
    }

    fn ids(catalog: &ModelCatalog, model: &str) -> Vec<String> {
        catalog
            .matching(model)
            .iter()
            .map(|r| format!("{}/{}", r.provider.as_deref().unwrap_or("-"), r.id))
            .collect()
    }

    #[test]
    fn a_name_matches_an_id_an_alias_a_provider_form_and_a_thinking_suffix() {
        let catalog = ModelCatalog {
            source: CatalogSource::Listed,
            models: vec![
                row(Some("claude-bridge"), "claude-opus-5-5", &["opus"]),
                row(Some("amazon-bedrock"), "claude-opus-5-5", &["opus"]),
                row(Some("openrouter"), "anthropic/claude-x", &[]),
                row(Some("amazon-bedrock"), "anthropic.claude-v2:0", &[]),
            ],
            efforts: Vec::new(),
        };
        assert_eq!(ids(&catalog, "opus").len(), 2);
        assert_eq!(ids(&catalog, "OPUS:high").len(), 2);
        assert_eq!(
            ids(&catalog, "claude-bridge/opus:high"),
            ["claude-bridge/claude-opus-5-5"]
        );
        assert_eq!(
            ids(&catalog, "claude-bridge/claude-opus-5-5"),
            ["claude-bridge/claude-opus-5-5"]
        );
        assert_eq!(
            ids(&catalog, "anthropic/claude-x"),
            ["openrouter/anthropic/claude-x"]
        );
        assert_eq!(
            ids(&catalog, "openrouter/anthropic/claude-x:low"),
            ["openrouter/anthropic/claude-x"]
        );
        assert_eq!(
            ids(&catalog, "anthropic.claude-v2:0"),
            ["amazon-bedrock/anthropic.claude-v2:0"]
        );
        assert!(ids(&catalog, "sonnet").is_empty());
        assert!(ids(&catalog, "openai/opus").is_empty());
    }
}
