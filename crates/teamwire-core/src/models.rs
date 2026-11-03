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
}
