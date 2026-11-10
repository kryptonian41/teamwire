//! A session's token usage, per model, as a harness reads it from its own
//! session file. Metrics read it from the `result.delivering` event.

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
}

impl TokenCounts {
    /// Adds `other` to these counts, each saturating.
    pub fn add(&mut self, other: Self) {
        self.input = self.input.saturating_add(other.input);
        self.output = self.output.saturating_add(other.output);
        self.cache_read = self.cache_read.saturating_add(other.cache_read);
        self.cache_creation = self.cache_creation.saturating_add(other.cache_creation);
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
}
