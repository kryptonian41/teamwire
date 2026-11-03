//! Each member's task class, effort and model, checked against the model
//! list of the member's harness. Launch refuses only what is certainly
//! wrong: an effort the harness does not take, and a `provider/` prefix the
//! harness lists no model from. A model name the list cannot match only
//! warns, because a fixed list can lag the vendor and pi's patterns are
//! fuzzy. A harness that gives no list skips both checks with a warning, so
//! a slow or broken harness program never blocks a launch.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use teamwire_core::{HarnessId, ModelCatalog, ModelInfo, Registry};

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

/// Warns about a model the list cannot match, or one that more than one
/// provider offers, and refuses a `provider/` prefix the list has no model
/// from.
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
    let rows = matching(catalog, model);
    if let Some(row) = rows.first() {
        let offered: BTreeSet<&str> = rows.iter().filter_map(|r| r.provider.as_deref()).collect();
        if offered.len() > 1 {
            let named: Vec<&str> = offered.into_iter().collect();
            warnings.push(format!(
                "role {role}: harness {harness} offers model {model:?} through more than one \
                 provider ({}), so the harness picks the provider itself; write provider/id, \
                 such as {}/{}",
                named.join(", "),
                named.first().copied().unwrap_or_default(),
                row.id
            ));
        }
        return;
    }
    let providers: BTreeSet<&str> = catalog
        .models
        .iter()
        .filter_map(|r| r.provider.as_deref())
        .collect();
    if let Some((prefix, _)) = model.split_once('/')
        && !providers.is_empty()
        && !providers.iter().any(|p| p.eq_ignore_ascii_case(prefix))
    {
        errors.push(SpecError::new(
            Some(role),
            Some("model"),
            "model_provider_unavailable",
            format!(
                "role {role}: harness {harness} lists no model from provider {prefix} for model \
                 {model:?}. A provider the user excluded, or one without a credential on this \
                 machine, is not listed. The listed providers are {}",
                providers.into_iter().collect::<Vec<_>>().join(", ")
            ),
        ));
        return;
    }
    warnings.push(format!(
        "role {role}: harness {harness} does not list model {model:?}; \
         `teamwire models --harness {harness}` lists what it can reach"
    ));
}

/// The rows `model` names. pi takes a `:<thinking>` suffix, so the part
/// before the last `:` is tried when the whole name matches nothing.
fn matching<'c>(catalog: &'c ModelCatalog, model: &str) -> Vec<&'c ModelInfo> {
    let mut names = vec![model];
    if let Some((before, _)) = model.rsplit_once(':') {
        names.push(before);
    }
    names
        .into_iter()
        .map(|name| {
            catalog
                .models
                .iter()
                .filter(|row| names_row(row, name))
                .collect::<Vec<_>>()
        })
        .find(|rows| !rows.is_empty())
        .unwrap_or_default()
}

/// True when `name` is the row's id or an alias, either bare or as
/// `provider/id`.
fn names_row(row: &ModelInfo, name: &str) -> bool {
    let bare = |n: &str| {
        row.id.eq_ignore_ascii_case(n) || row.aliases.iter().any(|a| a.eq_ignore_ascii_case(n))
    };
    bare(name)
        || row.provider.as_deref().is_some_and(|p| {
            name.split_once('/')
                .is_some_and(|(prefix, rest)| prefix.eq_ignore_ascii_case(p) && bare(rest))
        })
}

#[cfg(test)]
mod tests {
    use teamwire_core::CatalogSource;

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
        matching(catalog, model)
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
