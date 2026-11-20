//! The family of a model: the vendor line it belongs to, such as Claude,
//! GPT or Gemini. Code works out a family to label a model and to warn
//! about a set of members, never to pick a harness or refuse a launch
//! (ADR 0053, ADR 0056).
//!
//! The tables below name vendors and the first word of their model ids.
//! They name no model, so a vendor's next model needs no change here. A
//! harness whose every model is of one family says so itself, through
//! `Harness::model_family`, because the core never matches a harness id.

use crate::strings::string_enum;

string_enum! {
    /// The vendor line of a model. `Unknown` is any model the tables below
    /// do not place: the function never guesses.
    pub enum ModelFamily {
        Claude => "claude",
        Gpt => "gpt",
        Gemini => "gemini",
        Gemma => "gemma",
        Grok => "grok",
        Deepseek => "deepseek",
        Kimi => "kimi",
        Glm => "glm",
        Qwen => "qwen",
        Mistral => "mistral",
        Llama => "llama",
        Unknown => "unknown",
    }
}

/// The vendor part of an `OpenRouter` id, `openrouter/<vendor>/<id>`, and
/// the family it names. Google is not a row: it makes two lines, Gemini and
/// Gemma, so its id decides (`GOOGLE`).
const VENDORS: &[(&str, ModelFamily)] = &[
    ("anthropic", ModelFamily::Claude),
    ("openai", ModelFamily::Gpt),
    ("x-ai", ModelFamily::Grok),
    ("deepseek", ModelFamily::Deepseek),
    ("moonshotai", ModelFamily::Kimi),
    ("z-ai", ModelFamily::Glm),
    ("qwen", ModelFamily::Qwen),
    ("mistralai", ModelFamily::Mistral),
    ("meta", ModelFamily::Llama),
    ("meta-llama", ModelFamily::Llama),
];

/// The first word of a vendor's model ids, once `normalise_model` has
/// dropped the provider and region parts, and the family it names.
const STARTS: &[(&str, ModelFamily)] = &[
    ("claude", ModelFamily::Claude),
    ("gpt-", ModelFamily::Gpt),
    ("chatgpt-", ModelFamily::Gpt),
    ("gemini", ModelFamily::Gemini),
    ("gemma", ModelFamily::Gemma),
    ("grok", ModelFamily::Grok),
    ("deepseek", ModelFamily::Deepseek),
    ("kimi", ModelFamily::Kimi),
    ("glm", ModelFamily::Glm),
    ("qwen", ModelFamily::Qwen),
    ("mistral", ModelFamily::Mistral),
    ("codestral", ModelFamily::Mistral),
    ("devstral", ModelFamily::Mistral),
    ("llama", ModelFamily::Llama),
];

/// The start of an `openrouter/google/<id>` id, and the family it names.
/// Google makes two lines, so its vendor part alone names no family, and
/// any other Google id is `Unknown`.
const GOOGLE: &[(&str, ModelFamily)] = &[
    ("gemini", ModelFamily::Gemini),
    ("gemma", ModelFamily::Gemma),
];

/// The family of `model`, the string a spec's `model` holds, such as
/// `openrouter/openai/gpt-6.2-astra` or `claude-bridge/claude-opus-5-5`.
/// Reads no file and calls nothing.
///
/// The string is lowercased and loses a trailing `:suffix`, such as pi's
/// `:medium`. An `OpenRouter` id of three parts is placed by its vendor
/// part alone, after a leading `~`, so `openrouter/openai/o3` is GPT
/// although `o3` starts no row. The vendor `google` is the one exception:
/// its id must start with `gemini` or `gemma`. Any other string is placed
/// by the start of its `normalise_model` name. A string neither table
/// places is `Unknown`.
pub fn model_family(model: &str) -> ModelFamily {
    let lower = model.trim().to_ascii_lowercase();
    let whole = lower.rsplit_once(':').map_or(lower.as_str(), |(id, _)| id);
    let parts: Vec<&str> = whole.split('/').collect();
    if let ["openrouter", vendor, id] = parts.as_slice() {
        let vendor = vendor.strip_prefix('~').unwrap_or(vendor);
        if vendor == "google" {
            return by_start(GOOGLE, id);
        }
        return VENDORS
            .iter()
            .find(|(name, _)| *name == vendor)
            .map_or(ModelFamily::Unknown, |&(_, family)| family);
    }
    by_start(STARTS, &super::normalise_model(whole))
}

/// The family of the first row of `table` whose start begins `name`, else
/// `Unknown`.
fn by_start(table: &[(&str, ModelFamily)], name: &str) -> ModelFamily {
    table
        .iter()
        .find(|(start, _)| name.starts_with(start))
        .map_or(ModelFamily::Unknown, |&(_, family)| family)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_family_round_trips_through_its_string() {
        for family in ModelFamily::ALL {
            assert_eq!(ModelFamily::parse(family.as_str()).ok(), Some(*family));
        }
    }

    #[test]
    fn an_openrouter_model_is_of_its_gpt_vendor() {
        assert_eq!(
            model_family("openrouter/openai/gpt-6.2-astra"),
            ModelFamily::Gpt
        );
    }

    #[test]
    fn a_suffix_after_the_id_leaves_the_family() {
        assert_eq!(
            model_family("openrouter/openai/gpt-6.2-astra:batch"),
            ModelFamily::Gpt
        );
    }

    #[test]
    fn a_tilde_before_the_vendor_is_dropped() {
        assert_eq!(
            model_family("openrouter/~openai/gpt-sol-latest"),
            ModelFamily::Gpt
        );
    }

    #[test]
    fn a_google_gemini_model_is_gemini() {
        assert_eq!(
            model_family("openrouter/google/gemini-4-carbon"),
            ModelFamily::Gemini
        );
    }

    #[test]
    fn a_google_gemma_model_is_gemma_not_gemini() {
        assert_eq!(
            model_family("openrouter/google/gemma-3-27b-it"),
            ModelFamily::Gemma
        );
    }

    #[test]
    fn a_google_model_that_is_neither_gemini_nor_gemma_is_unknown() {
        // The vendor part alone places any other vendor's model. Google's
        // places only its two lines, so another Google id is not guessed.
        assert_eq!(
            model_family("openrouter/google/lyria-3"),
            ModelFamily::Unknown
        );
    }

    #[test]
    fn an_openrouter_anthropic_model_is_claude() {
        assert_eq!(
            model_family("openrouter/anthropic/claude-opus-5.5"),
            ModelFamily::Claude
        );
    }

    #[test]
    fn openrouter_auto_is_unknown() {
        assert_eq!(model_family("openrouter/auto"), ModelFamily::Unknown);
    }

    #[test]
    fn a_provider_part_is_dropped_before_the_id_is_read() {
        assert_eq!(
            model_family("claude-bridge/claude-opus-5-5"),
            ModelFamily::Claude
        );
    }

    #[test]
    fn a_bedrock_region_and_vendor_word_are_dropped() {
        assert_eq!(
            model_family("amazon-bedrock/us.anthropic.claude-opus-5-5-v1:0"),
            ModelFamily::Claude
        );
    }

    #[test]
    fn a_bedrock_vendor_word_before_a_version_is_the_models_name() {
        assert_eq!(
            model_family("amazon-bedrock/deepseek.v3.2"),
            ModelFamily::Deepseek
        );
    }

    #[test]
    fn the_openrouter_vendor_alone_places_a_model_its_id_does_not() {
        assert_eq!(model_family("openrouter/openai/o3"), ModelFamily::Gpt);
    }

    #[test]
    fn the_model_string_is_read_lowercased() {
        assert_eq!(
            model_family("OpenRouter/OpenAI/GPT-6.2-Astra"),
            ModelFamily::Gpt
        );
        // Only the vendor part places `o3`, so this row needs the lowercasing
        // before the `openrouter/` test, not only the one `normalise_model` does.
        assert_eq!(model_family("OpenRouter/OpenAI/O3"), ModelFamily::Gpt);
    }

    #[test]
    fn an_openrouter_moonshot_model_is_kimi() {
        assert_eq!(
            model_family("openrouter/moonshotai/kimi-k3.1-pro"),
            ModelFamily::Kimi
        );
    }

    #[test]
    fn a_claude_code_alias_alone_is_unknown_to_the_core() {
        // Claude Code's `opus` is Claude only because the harness says so
        // (`Harness::model_family`). The string alone names no vendor.
        assert_eq!(model_family("opus"), ModelFamily::Unknown);
    }

    #[test]
    fn every_vendor_of_the_openrouter_table_has_its_family() {
        for (model, want) in [
            ("openrouter/x-ai/grok-5", ModelFamily::Grok),
            ("openrouter/deepseek/deepseek-v4", ModelFamily::Deepseek),
            ("openrouter/z-ai/glm-5", ModelFamily::Glm),
            ("openrouter/qwen/qwen4-coder", ModelFamily::Qwen),
            ("openrouter/mistralai/devstral-3", ModelFamily::Mistral),
            ("openrouter/meta/llama-5", ModelFamily::Llama),
            ("openrouter/meta-llama/llama-5", ModelFamily::Llama),
            ("openrouter/nousresearch/hermes-5", ModelFamily::Unknown),
        ] {
            assert_eq!(model_family(model), want, "{model}");
        }
    }

    #[test]
    fn every_start_of_the_id_table_has_its_family() {
        for (model, want) in [
            ("claude-sonnet-5-5", ModelFamily::Claude),
            ("gpt-6.1-sol", ModelFamily::Gpt),
            ("chatgpt-4o-latest", ModelFamily::Gpt),
            ("gemini-4-carbon", ModelFamily::Gemini),
            ("gemma-3-27b-it", ModelFamily::Gemma),
            ("amazon-bedrock/google.gemma-4-31b", ModelFamily::Gemma),
            ("grok-5", ModelFamily::Grok),
            ("deepseek-r2", ModelFamily::Deepseek),
            ("global.moonshotai.kimi-k3", ModelFamily::Kimi),
            ("glm-5", ModelFamily::Glm),
            ("qwen4-coder", ModelFamily::Qwen),
            ("mistral-large-3", ModelFamily::Mistral),
            ("codestral-3", ModelFamily::Mistral),
            ("devstral-3", ModelFamily::Mistral),
            ("meta.llama5-70b-instruct-v1:0", ModelFamily::Llama),
            ("gpt", ModelFamily::Unknown),
            ("amazon.nova-2-lite-v1:0", ModelFamily::Unknown),
            ("", ModelFamily::Unknown),
        ] {
            assert_eq!(model_family(model), want, "{model}");
        }
    }
}
