use super::*;

#[test]
fn task_class_round_trips_through_its_string() {
    for class in TaskClass::ALL {
        assert_eq!(TaskClass::parse(class.as_str()).ok(), Some(*class));
    }
}

#[test]
fn find_matches_an_id_or_an_alias() {
    let catalog = ModelCatalog {
        source: CatalogSource::Builtin,
        models: vec![ModelInfo {
            id: "claude-sonnet-5-5".into(),
            aliases: vec!["sonnet".into()],
            provider: None,
            context_tokens: None,
            thinking: true,
        }],
        efforts: vec!["low".into()],
    };
    assert_eq!(
        catalog.find("sonnet").map(|m| m.id.as_str()),
        Some("claude-sonnet-5-5")
    );
    assert!(catalog.find("opus").is_none());
}

#[test]
fn normalise_model_drops_provider_region_date_and_version_parts() {
    for (name, want) in [
        ("deepseek.v3.2", "deepseek-v3.2"),
        ("deepseek.v3-v1:0", "deepseek-v3"),
        ("deepseek.r1-v1:0", "deepseek-r1"),
        (
            "us.anthropic.claude-opus-4-5-20251101-v1:0",
            "claude-opus-4-5",
        ),
        ("anthropic.claude-opus-4-6-v1", "claude-opus-4-6"),
        ("global.openai.gpt-6.1-sol", "gpt-6.1-sol"),
        ("global.anthropic.claude-sonnet-4-5", "claude-sonnet-4-5"),
        ("amazon.nova-2-lite-v1:0", "nova-2-lite"),
        ("jp.amazon.nova-2-lite-v1:0", "nova-2-lite"),
        ("amazon-bedrock/amazon.nova-premier-v1:0", "nova-premier"),
        ("claude-bridge/claude-opus-5-5", "claude-opus-5-5"),
        ("Claude-Opus-4-5", "claude-opus-4-5"),
        ("claude-sonnet-4-5@20250929", "claude-sonnet-4-5"),
        ("gpt-4o-2024-08-06", "gpt-4o"),
        ("claude-opus-4-20250514", "claude-opus-4"),
        ("global.moonshotai.kimi-k3", "kimi-k3"),
    ] {
        assert_eq!(normalise_model(name), want, "{name}");
    }
}

#[test]
fn normalise_model_keeps_a_models_own_dots_and_version() {
    for name in [
        "gpt-5.2",
        "gpt-6.1-sol",
        "deepseek-v3.2",
        "deepseek-v4",
        "deepseek-v4-pro",
        "deepseek-coder-v2",
        "kimi-k2.5",
        "o3",
    ] {
        assert_eq!(normalise_model(name), name);
    }
    assert_ne!(normalise_model("gpt-5.2"), normalise_model("gpt-5"));
}

fn row(provider: Option<&str>, id: &str, aliases: &[&str]) -> ModelInfo {
    ModelInfo {
        id: id.into(),
        aliases: aliases.iter().map(|a| (*a).into()).collect(),
        provider: provider.map(Into::into),
        context_tokens: None,
        thinking: true,
    }
}

/// Rows spelled as pi 1.5.0 lists them on this project's machine.
fn pi_rows() -> ModelCatalog {
    ModelCatalog {
        source: CatalogSource::Listed,
        models: vec![
            row(Some("amazon-bedrock"), "anthropic.claude-opus-4-6-v1", &[]),
            row(Some("amazon-bedrock"), "us.anthropic.claude-opus-5-5", &[]),
            row(Some("amazon-bedrock"), "deepseek.v3.2", &[]),
            row(Some("openrouter"), "anthropic/claude-x", &[]),
            row(Some("claude-bridge"), "claude-opus-5-5", &[]),
            row(Some("claude-bridge"), "claude-sonnet-4-6", &[]),
        ],
        efforts: Vec::new(),
    }
}

fn named(catalog: &ModelCatalog, model: &str) -> Vec<String> {
    catalog
        .matching(model)
        .iter()
        .map(|r| format!("{}/{}", r.provider.as_deref().unwrap_or("-"), r.id))
        .collect()
}

#[test]
fn a_bare_pattern_matches_every_providers_spelling() {
    let catalog = pi_rows();
    let opus = [
        "amazon-bedrock/anthropic.claude-opus-4-6-v1",
        "amazon-bedrock/us.anthropic.claude-opus-5-5",
        "claude-bridge/claude-opus-5-5",
    ];
    assert_eq!(named(&catalog, "opus"), opus);
    assert_eq!(named(&catalog, "OPUS:high"), opus);
    assert_eq!(
        named(&catalog, "claude-opus-5-5"),
        [
            "amazon-bedrock/us.anthropic.claude-opus-5-5",
            "claude-bridge/claude-opus-5-5"
        ]
    );
    assert_eq!(
        named(&catalog, "sonnet"),
        ["claude-bridge/claude-sonnet-4-6"]
    );
    assert_eq!(
        named(&catalog, "deepseek-v3.2"),
        ["amazon-bedrock/deepseek.v3.2"]
    );
    assert!(named(&catalog, "gpt-9").is_empty());
}

#[test]
fn an_exact_id_adds_only_rows_of_the_same_model() {
    let mut catalog = pi_rows();
    catalog
        .models
        .push(row(Some("amazon-bedrock"), "anthropic.claude-v2:0", &[]));
    assert_eq!(normalise_model("anthropic.claude-v2:0"), "claude");
    assert_eq!(
        named(&catalog, "anthropic.claude-v2:0"),
        ["amazon-bedrock/anthropic.claude-v2:0"]
    );
}

#[test]
fn a_provider_prefix_limits_the_match_to_that_provider() {
    let catalog = pi_rows();
    assert_eq!(
        named(&catalog, "claude-bridge/opus:high"),
        ["claude-bridge/claude-opus-5-5"]
    );
    assert_eq!(
        named(&catalog, "claude-bridge/claude-opus-5-5:high"),
        ["claude-bridge/claude-opus-5-5"]
    );
    assert_eq!(
        named(&catalog, "anthropic/claude-x"),
        ["openrouter/anthropic/claude-x"]
    );
    assert!(named(&catalog, "openai/claude-opus-5-5").is_empty());
    assert_eq!(
        catalog.providers(),
        ["amazon-bedrock", "claude-bridge", "openrouter"]
    );
}

#[test]
fn a_list_without_providers_matches_only_an_id_or_an_alias() {
    let catalog = ModelCatalog {
        source: CatalogSource::Builtin,
        models: vec![row(None, "claude-sonnet-5-5", &["sonnet"])],
        efforts: Vec::new(),
    };
    assert_eq!(named(&catalog, "sonnet:high"), ["-/claude-sonnet-5-5"]);
    assert!(named(&catalog, "sonnet-5").is_empty());
    assert!(catalog.providers().is_empty());
}

#[test]
fn same_model_finds_a_vendor_name_under_every_spelling() {
    let catalog = pi_rows();
    let ids = |name: &str| -> Vec<&str> {
        catalog
            .same_model(name)
            .iter()
            .map(|r| r.id.as_str())
            .collect()
    };
    assert_eq!(
        ids("claude-opus-5-5"),
        ["us.anthropic.claude-opus-5-5", "claude-opus-5-5"]
    );
    assert_eq!(ids("deepseek-v3.2"), ["deepseek.v3.2"]);
    assert!(ids("opus").is_empty());
}

/// Rows spelled as pi 1.5.0 lists them through `OpenRouter` and Bedrock.
fn dotted_rows() -> ModelCatalog {
    ModelCatalog {
        source: CatalogSource::Listed,
        models: vec![
            row(Some("openrouter"), "anthropic/claude-opus-5.5", &[]),
            row(Some("openrouter"), "anthropic/claude-opus-5.5:batch", &[]),
            row(Some("openrouter"), "anthropic/claude-opus-5", &[]),
            row(Some("openrouter"), "openai/gpt-5.4", &[]),
            row(Some("amazon-bedrock"), "us.anthropic.claude-opus-5-5", &[]),
        ],
        efforts: Vec::new(),
    }
}

#[test]
fn same_model_reads_a_dot_between_two_digits_as_a_hyphen() {
    let catalog = dotted_rows();
    let ids = |name: &str| -> Vec<&str> {
        catalog
            .same_model(name)
            .iter()
            .map(|r| r.id.as_str())
            .collect()
    };
    let opus = ["anthropic/claude-opus-5.5", "us.anthropic.claude-opus-5-5"];
    assert_eq!(ids("claude-opus-5-5"), opus);
    assert_eq!(ids("claude-opus-5.5"), opus);
    assert_eq!(ids("Claude-Opus-5-5-20260301"), opus);
    assert_eq!(ids("gpt-5.4"), ["openai/gpt-5.4"]);
    assert_eq!(ids("gpt-5-4"), ["openai/gpt-5.4"]);
    // The version is still part of the name: Opus 5 is not Opus 5.5.
    assert_eq!(ids("claude-opus-5"), ["anthropic/claude-opus-5"]);
    assert!(ids("claude-opus-55").is_empty());
}

#[test]
fn matching_keeps_a_dotted_and_a_hyphenated_version_apart() {
    // pi's `--model` does not find `claude-opus-5.5` by `claude-opus-5-5`,
    // so a spec that writes the hyphen names only the row spelled that way.
    let catalog = dotted_rows();
    assert_eq!(
        named(&catalog, "claude-opus-5-5"),
        ["amazon-bedrock/us.anthropic.claude-opus-5-5"]
    );
    assert_eq!(
        named(&catalog, "openrouter/anthropic/claude-opus-5.5"),
        ["openrouter/anthropic/claude-opus-5.5"]
    );
    assert!(named(&catalog, "openrouter/claude-opus-5-5").is_empty());
}
