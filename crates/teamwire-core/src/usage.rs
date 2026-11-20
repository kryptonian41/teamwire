//! A session's token usage and money, per model, as a harness reads it from
//! its own session file. Metrics read it from the `result.delivering` event.
//! Money is only what the harness recorded: nothing here works it out from
//! prices (ADR 0060).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One model's tokens in a session so far. A harness that does not report a
/// kind leaves it 0, and its docs say which.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenCounts {
    /// Tokens read that were not read from the cache.
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
    /// The money the harness recorded, in whole millionths of a US dollar.
    /// A float would lose `Eq`. 0 means none recorded, and the JSON then
    /// leaves the key out, so a harness without money writes the four token
    /// keys only.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub cost_micro_usd: u64,
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_zero(n: &u64) -> bool {
    *n == 0
}

impl TokenCounts {
    /// Adds `other` to these counts, each saturating.
    pub fn add(&mut self, other: Self) {
        self.input = self.input.saturating_add(other.input);
        self.output = self.output.saturating_add(other.output);
        self.cache_read = self.cache_read.saturating_add(other.cache_read);
        self.cache_creation = self.cache_creation.saturating_add(other.cache_creation);
        self.cost_micro_usd = self.cost_micro_usd.saturating_add(other.cost_micro_usd);
    }

    /// A dollar figure as whole millionths, rounded to the nearest. A
    /// negative or not-a-number figure is 0, and one too large for `u64`
    /// is `u64::MAX`.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a float to int cast saturates: below 0 and NaN give 0, too large gives u64::MAX"
    )]
    pub fn micro_usd(dollars: f64) -> u64 {
        (dollars * 1_000_000.0).round() as u64
    }
}

/// Token counts by model name.
pub type SessionUsage = BTreeMap<String, TokenCounts>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_serialises_as_the_event_shape_metrics_read() {
        let mut usage = SessionUsage::new();
        let mut counts = TokenCounts {
            input: 1,
            output: 2,
            cache_read: 3,
            cache_creation: 4,
            cost_micro_usd: 0,
        };
        counts.add(TokenCounts {
            input: u64::MAX,
            ..TokenCounts::default()
        });
        usage.insert("m".to_owned(), counts);
        assert_eq!(
            serde_json::to_value(&usage).unwrap(),
            serde_json::json!({"m": {"input": u64::MAX, "output": 2, "cache_read": 3, "cache_creation": 4}})
        );
    }

    #[test]
    fn money_adds_and_rides_in_the_json_only_when_recorded() {
        let mut counts = TokenCounts {
            cost_micro_usd: 120_480,
            ..TokenCounts::default()
        };
        counts.add(TokenCounts {
            cost_micro_usd: 2_250,
            ..TokenCounts::default()
        });
        assert_eq!(counts.cost_micro_usd, 122_730);
        counts.add(TokenCounts {
            cost_micro_usd: u64::MAX,
            ..TokenCounts::default()
        });
        assert_eq!(counts.cost_micro_usd, u64::MAX);
        let json = serde_json::json!({"input": 0, "output": 0, "cache_read": 0, "cache_creation": 0, "cost_micro_usd": 5});
        let read: TokenCounts = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(read.cost_micro_usd, 5);
        assert_eq!(serde_json::to_value(read).unwrap(), json);
        // An event written before money existed still reads, as no money.
        let old: TokenCounts = serde_json::from_value(
            serde_json::json!({"input": 1, "output": 2, "cache_read": 3, "cache_creation": 4}),
        )
        .unwrap();
        assert_eq!(old.cost_micro_usd, 0);
    }

    #[test]
    fn a_dollar_figure_rounds_to_the_nearest_millionth() {
        assert_eq!(TokenCounts::micro_usd(0.12048), 120_480);
        assert_eq!(TokenCounts::micro_usd(0.000_000_4), 0);
        assert_eq!(TokenCounts::micro_usd(0.000_000_6), 1);
        assert_eq!(TokenCounts::micro_usd(1.587_8), 1_587_800);
        assert_eq!(TokenCounts::micro_usd(0.0), 0);
        assert_eq!(TokenCounts::micro_usd(-0.5), 0);
        assert_eq!(TokenCounts::micro_usd(f64::NAN), 0);
        assert_eq!(TokenCounts::micro_usd(f64::INFINITY), u64::MAX);
    }
}
