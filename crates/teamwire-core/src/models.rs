//! The models a harness can run, the class of task a member is given, and
//! the public evidence on which model and effort suit which class.
//!
//! A lead picks a member's model from the models the member's harness can
//! reach, so the list comes from the harness, never from a list of one
//! vendor's names. A lead also names each member's task class, and launch
//! records it, so a run's outcome can later be read per class.

use serde::{Deserialize, Serialize};

use crate::strings::string_enum;

string_enum! {
    /// How hard a member's task is to reason about, as the lead judges it at
    /// launch. The class picks the model and effort; the member's `kind`
    /// only sets its role. Launch writes the class as `task_class` into the
    /// `data` of the member's `member.creating` event.
    pub enum TaskClass {
        /// A change a script could nearly make: a rename across files, a
        /// moved function, a dependency bump with call-site edits.
        MechanicalEdit => "mechanical_edit",
        /// One feature or fix inside one module, with its test.
        ContainedFeature => "contained_feature",
        /// A change to a shared interface and every caller of it.
        CrossCutting => "cross_cutting",
        /// Finding why something fails, before any fix.
        RootCause => "root_cause",
        /// Reasoning about ordering, races, crashes or data loss.
        ConcurrencyDataLoss => "concurrency_data_loss",
        /// Reading code and data to map how something works. Writes nothing.
        ReadOnlySurvey => "read_only_survey",
        /// Judging other agents' work: a review or the closer's checks.
        Judgement => "judgement",
    }
}

string_enum! {
    /// Where a harness's model list came from.
    pub enum CatalogSource {
        /// The harness program printed the list, so it holds only the models
        /// the program can reach on this machine (pi's `--list-models`).
        Listed => "listed",
        /// The harness program has no list command, so the adapter carries a
        /// fixed list from the harness's documentation. A model on it may
        /// still be out of reach for this account.
        Builtin => "builtin",
    }
}

/// One model a harness can start a member on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInfo {
    /// The value a spec puts in `model` for this harness, exactly as the
    /// harness's launch passes it on.
    pub id: String,
    /// Other values the harness accepts for the same model, such as
    /// Claude Code's `sonnet`. Empty when there are none.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// The provider or vendor the harness reaches the model through, such
    /// as `claude-bridge` or `amazon-bedrock` in pi. `None` when the harness
    /// has only one.
    #[serde(default)]
    pub provider: Option<String>,
    /// The context window in tokens, when the harness reports it.
    #[serde(default)]
    pub context_tokens: Option<u64>,
    /// Whether the model takes an effort (thinking) level at all.
    pub thinking: bool,
}

/// Every model a harness can start a member on, and the effort levels the
/// harness takes, lowest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCatalog {
    pub source: CatalogSource,
    pub models: Vec<ModelInfo>,
    /// The values the harness accepts in a spec's `effort`, lowest first.
    /// Launch refuses any other value for a member of this harness.
    pub efforts: Vec<String>,
}

impl ModelCatalog {
    /// The model whose `id` or one of whose `aliases` is `name`.
    pub fn find(&self, name: &str) -> Option<&ModelInfo> {
        self.models
            .iter()
            .find(|m| m.id == name || m.aliases.iter().any(|a| a == name))
    }

    /// Every provider the list names, sorted, each once. Empty for a
    /// harness that reaches its models one way only.
    pub fn providers(&self) -> Vec<&str> {
        let mut providers: Vec<&str> = self
            .models
            .iter()
            .filter_map(|m| m.provider.as_deref())
            .collect();
        providers.sort_unstable();
        providers.dedup();
        providers
    }

    /// The rows a spec's `model` names, as the harness resolves it. The
    /// value may end in pi's `:<thinking>` suffix, so the part before the
    /// last `:` is tried when the whole value names nothing. A row matches
    /// by its id or an alias, bare or as `provider/id`, ignoring case.
    ///
    /// On a list whose rows carry a provider, one model has a spelling per
    /// provider, so more rows match. A value that is some row's id also
    /// names every row with the same `normalise_model` name. Any other
    /// value names every row whose `normalise_model` name holds the
    /// value's, because pi's own search finds `opus` in every Opus row of
    /// every provider. A `provider/` prefix keeps the match to that provider.
    pub fn matching(&self, model: &str) -> Vec<&ModelInfo> {
        let mut names = vec![model];
        if let Some((before, _)) = model.rsplit_once(':') {
            names.push(before);
        }
        names
            .into_iter()
            .map(|name| {
                let exact = self.models.iter().any(|row| names_exactly(row, name));
                self.models
                    .iter()
                    .filter(|row| pattern_names(row, name, exact))
                    .collect::<Vec<_>>()
            })
            .find(|rows| !rows.is_empty())
            .unwrap_or_default()
    }

    /// The rows that are the model a vendor calls `name`, under any
    /// harness's or provider's spelling: the rows whose id or alias has the
    /// same `normalise_model` name.
    pub fn same_model(&self, name: &str) -> Vec<&ModelInfo> {
        let key = normalise_model(name);
        self.models
            .iter()
            .filter(|row| names_of(row).any(|n| normalise_model(n) == key))
            .collect()
    }
}

fn names_of(row: &ModelInfo) -> impl Iterator<Item = &String> {
    std::iter::once(&row.id).chain(&row.aliases)
}

/// True when `name` is the row's id or an alias, bare or as `provider/id`.
fn names_exactly(row: &ModelInfo, name: &str) -> bool {
    let exact = |n: &str| names_of(row).any(|x| x.eq_ignore_ascii_case(n));
    exact(name)
        || row.provider.as_deref().is_some_and(|p| {
            name.split_once('/')
                .is_some_and(|(prefix, rest)| prefix.eq_ignore_ascii_case(p) && exact(rest))
        })
}

/// True when the pattern `name` names `row` (see `ModelCatalog::matching`).
/// `equal` says `name` is some row's id, so only the same normalised name
/// counts, not one that merely holds it: Bedrock's `anthropic.claude-v2:0`
/// normalises to `claude`, which every Claude row holds.
fn pattern_names(row: &ModelInfo, name: &str, equal: bool) -> bool {
    if names_exactly(row, name) {
        return true;
    }
    let Some(provider) = row.provider.as_deref() else {
        return false;
    };
    let rest = match name.split_once('/') {
        Some((prefix, rest)) if prefix.eq_ignore_ascii_case(provider) => rest,
        // A `/` that does not follow this row's provider is part of an id,
        // as in OpenRouter's `anthropic/claude-x`, or another provider's
        // prefix: only the whole id names the row.
        Some(_) => return false,
        None => name,
    };
    let key = normalise_model(rest);
    !key.is_empty()
        && names_of(row).any(|n| {
            let n = normalise_model(n);
            if equal { n == key } else { n.contains(&key) }
        })
}

/// The name two spellings of one model are compared by. A vendor, a
/// harness and a provider spell one model differently: pi lists Opus 5.5 as
/// `claude-opus-5-5` through claude-bridge and as
/// `us.anthropic.claude-opus-5-5` through Amazon Bedrock. So the name is
/// lowercased, loses a `provider/` part, loses leading dotted words such as
/// `us.` and `anthropic.`, and loses trailing parts such as `-20251101`,
/// `@20251101`, `-2025-11-01`, `-v1:0` and `-v1`. A dotted word followed
/// by a version, as in Bedrock's `deepseek.v3.2`, is the model's own name,
/// so that dot becomes a hyphen: `deepseek-v3.2`.
pub fn normalise_model(name: &str) -> String {
    let lower = name.trim().to_ascii_lowercase();
    let mut rest = lower.rsplit('/').next().unwrap_or_default();
    let mut joined = None;
    // A leading word is a provider or region only when it is letters and
    // digits alone: a model's own dot, as in `gpt-5.2`, follows a hyphen.
    while let Some((head, tail)) = rest.split_once('.') {
        let word = !head.is_empty()
            && head.bytes().all(|b| b.is_ascii_alphanumeric())
            && head.bytes().any(|b| b.is_ascii_alphabetic());
        if !word || tail.is_empty() {
            break;
        }
        if is_version(tail) {
            joined = Some(format!("{head}-{tail}"));
            break;
        }
        rest = tail;
    }
    let joined = joined.unwrap_or_else(|| rest.to_owned());
    let mut cut = joined.as_str();
    while let Some(shorter) = strip_version(cut).or_else(|| strip_date(cut)) {
        cut = shorter;
    }
    cut.to_owned()
}

/// True when `tail` starts with a version, such as `v3` in `v3.2` or `r1`
/// in `r1-v1:0`: at most one letter, then digits, up to a `.`, `-` or `:`.
fn is_version(tail: &str) -> bool {
    let first = tail.split(['.', '-', ':']).next().unwrap_or_default();
    let digits = first
        .strip_prefix(|c: char| c.is_ascii_alphabetic())
        .unwrap_or(first);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

/// `rest` without a trailing Bedrock version: `-v1:0`, or `-v1` after a
/// model that already ends in a number, as in `claude-opus-4-6-v1`. A bare
/// `-v4` after a word is the model's own version, as in `deepseek-v4`.
fn strip_version(rest: &str) -> Option<&str> {
    let (model, version) = rest.rsplit_once("-v")?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let stripped = match version.split_once(':') {
        Some((major, minor)) => digits(major) && digits(minor),
        None => digits(version) && model.ends_with(|c: char| c.is_ascii_digit()),
    };
    (stripped && !model.is_empty()).then_some(model)
}

/// `rest` without a trailing date: `-20251101`, `@20251101` or `-2025-11-01`.
fn strip_date(rest: &str) -> Option<&str> {
    ["-dddddddd", "@dddddddd", "-dddd-dd-dd"]
        .iter()
        .find_map(|shape| {
            let cut = rest.len().checked_sub(shape.len())?;
            let (model, tail) = (rest.get(..cut)?, rest.get(cut..)?);
            let fits = tail.bytes().zip(shape.bytes()).all(|(c, s)| match s {
                b'd' => c.is_ascii_digit(),
                _ => c == s,
            });
            (fits && !model.is_empty()).then_some(model)
        })
}

/// The public evidence file: what published benchmarks and user reports say
/// about which model, at which effort, suits each task class. The file is
/// a dated snapshot, so `as_of` travels with every reading of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    /// The date the entries were last checked, `YYYY-MM-DD`.
    pub as_of: String,
    pub entries: Vec<EvidenceEntry>,
}

/// One claim from one source about one model on one task class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceEntry {
    pub task_class: TaskClass,
    /// The model as its vendor names it, such as `claude-sonnet-5-5` or
    /// `gpt-5.5-codex`. Matching to a harness's ids is the reader's job.
    pub model: String,
    /// The effort the source used, when it says. Free text, because vendors
    /// name their levels differently.
    #[serde(default)]
    pub effort: Option<String>,
    /// `1` is the best the source found for the class; a higher number
    /// ranks lower. Ranks compare entries of one source only.
    pub rank: u32,
    /// What the source measured or reported, in one sentence, with its
    /// number when it has one.
    pub claim: String,
    /// The source's name, such as `SWE-bench Verified leaderboard`.
    pub source: String,
    pub url: String,
    /// The date of the source, `YYYY-MM-DD`.
    pub published: String,
    pub kind: EvidenceKind,
}

string_enum! {
    /// How much a piece of evidence can carry.
    pub enum EvidenceKind {
        /// A published benchmark with a fixed task set and a score.
        Benchmark => "benchmark",
        /// A vendor's own guidance on which model or effort to use.
        Vendor => "vendor",
        /// A written report from users, such as a blog post or a forum
        /// thread with a measured comparison.
        UserReport => "user_report",
    }
}

#[cfg(test)]
mod tests {
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
}
