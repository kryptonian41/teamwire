//! Wall-clock time as teamwire stores it: UTC, RFC 3339, milliseconds.

use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::{OffsetDateTime, UtcOffset};

/// A point in time, in UTC, to the millisecond. Its text form is
/// `2026-09-29T14:03:11.482Z`; `parse` accepts any RFC 3339 time and
/// converts it. Only a `Clock` makes the current one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Timestamp(OffsetDateTime);

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("invalid timestamp {value:?}: {reason}")]
pub struct TimestampError {
    pub value: String,
    pub reason: String,
}

impl Timestamp {
    /// Converts to UTC and drops everything below the millisecond.
    pub fn from_datetime(time: OffsetDateTime) -> Self {
        let utc = time.to_offset(UtcOffset::UTC);
        let millis_only = (utc.nanosecond() / 1_000_000) * 1_000_000;
        Self(utc.replace_nanosecond(millis_only).unwrap_or(utc))
    }

    /// `None` when `millis` is outside the years the `time` crate supports.
    pub fn from_unix_millis(millis: i64) -> Option<Self> {
        OffsetDateTime::from_unix_timestamp_nanos(i128::from(millis) * 1_000_000)
            .ok()
            .map(Timestamp)
    }

    pub fn unix_millis(self) -> i64 {
        let millis = self.0.unix_timestamp_nanos().div_euclid(1_000_000);
        i64::try_from(millis).unwrap_or(i64::MAX)
    }

    pub fn as_datetime(self) -> OffsetDateTime {
        self.0
    }

    pub fn parse(text: &str) -> Result<Self, TimestampError> {
        OffsetDateTime::parse(text, &Rfc3339)
            .map(Self::from_datetime)
            .map_err(|e| TimestampError {
                value: text.to_owned(),
                reason: e.to_string(),
            })
    }

    /// `self + by`, or `None` past the supported range.
    pub fn checked_add(self, by: Duration) -> Option<Self> {
        let by = time::Duration::try_from(by).ok()?;
        self.0.checked_add(by).map(Self::from_datetime)
    }

    /// How long after `earlier` this is; zero when it is not later.
    pub fn saturating_since(self, earlier: Self) -> Duration {
        Duration::try_from(self.0 - earlier.0).unwrap_or(Duration::ZERO)
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let form = format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
        );
        match self.0.format(form) {
            Ok(text) => f.write_str(&text),
            Err(_) => Err(fmt::Error),
        }
    }
}

impl std::str::FromStr for Timestamp {
    type Err = TimestampError;
    fn from_str(text: &str) -> Result<Self, TimestampError> {
        Self::parse(text)
    }
}

impl Serialize for Timestamp {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str> as Deserialize>::deserialize(d)?;
        Self::parse(&text).map_err(<D::Error as serde::de::Error>::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_form_is_utc_with_milliseconds() {
        let t = Timestamp::parse("2026-09-29T19:33:11.482917+05:30").unwrap();
        assert_eq!(t.to_string(), "2026-09-29T14:03:11.482Z");
        assert_eq!(Timestamp::parse(&t.to_string()).unwrap(), t);
    }

    #[test]
    fn unix_millis_round_trip() {
        let t = Timestamp::from_unix_millis(1_790_000_000_123).unwrap();
        assert_eq!(t.unix_millis(), 1_790_000_000_123);
    }

    #[test]
    fn since_never_goes_negative() {
        let a = Timestamp::from_unix_millis(1_000).unwrap();
        let b = Timestamp::from_unix_millis(3_500).unwrap();
        assert_eq!(b.saturating_since(a), Duration::from_millis(2_500));
        assert_eq!(a.saturating_since(b), Duration::ZERO);
    }
}
