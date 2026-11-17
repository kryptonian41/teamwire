//! The lead's own handoff (ADR 0051): the point at which a lead is told to
//! hand its teams to a fresh session, the setting that moves the point, and
//! the files of one handoff under `$TEAMWIRE_HOME/handoffs/<session>/`.
//!
//! Nothing here touches a file: the hooks, the report crate and the launch
//! crate all build the same paths from this one place.

use std::path::{Path, PathBuf};

use crate::harness::{DEFAULT_HANDOFF_PCT, Fill};
use crate::names::SessionId;

/// The lowest point at which a lead is told to hand off. A fresh lead holds
/// about 98,000 tokens once its skill has loaded, so a lower point would
/// tell every new lead to hand off again at once. On a 200,000-token window
/// the point is therefore never reached, and the note never fires.
pub const MIN_LEAD_HANDOFF_TOKENS: u64 = 200_000;

/// The environment variable, and the `config.toml` key, of the setting.
pub const LEAD_HANDOFF_SETTING: &str = "TEAMWIRE_LEAD_HANDOFF_TOKENS";

/// What `TEAMWIRE_LEAD_HANDOFF_TOKENS` says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeadHandoffSetting {
    /// No value: the point is `DEFAULT_HANDOFF_PCT` of the window.
    Unset,
    /// `off`: the lead is never told to hand off.
    Off,
    /// A token count. A count below `MIN_LEAD_HANDOFF_TOKENS` counts as that
    /// floor.
    Tokens(u64),
    /// A value that is neither a whole number nor `off`, such as `40%`. It
    /// counts as `Unset`, and `teamwire doctor` names it.
    Invalid(String),
}

impl LeadHandoffSetting {
    /// Reads one value. An empty value is `Unset`, `off` in any case is
    /// `Off`, and a whole number may group its digits with `_` or `,`, as
    /// `400_000` or `400,000`.
    #[must_use]
    pub fn parse(value: Option<&str>) -> Self {
        let Some(text) = value.map(str::trim).filter(|t| !t.is_empty()) else {
            return Self::Unset;
        };
        if text.eq_ignore_ascii_case("off") {
            return Self::Off;
        }
        let digits: String = text.chars().filter(|c| !matches!(c, '_' | ',')).collect();
        if !digits.is_empty()
            && digits.chars().all(|c| c.is_ascii_digit())
            && let Ok(tokens) = digits.parse()
        {
            return Self::Tokens(tokens);
        }
        Self::Invalid(text.to_owned())
    }

    #[must_use]
    pub fn is_off(&self) -> bool {
        *self == Self::Off
    }

    /// The configured token count, when the setting gives one.
    #[must_use]
    pub fn tokens(&self) -> Option<u64> {
        match self {
            Self::Tokens(tokens) => Some(*tokens),
            Self::Unset | Self::Off | Self::Invalid(_) => None,
        }
    }
}

impl Fill {
    /// The token count at which a lead is told to hand off: the configured
    /// count, else `DEFAULT_HANDOFF_PCT` of the window, and never below
    /// `MIN_LEAD_HANDOFF_TOKENS`.
    #[must_use]
    pub fn lead_handoff_at(self, configured: Option<u64>) -> u64 {
        configured
            .unwrap_or_else(|| self.window.saturating_mul(DEFAULT_HANDOFF_PCT) / 100)
            .max(MIN_LEAD_HANDOFF_TOKENS)
    }
}

/// The files of one lead's handoff, all in `$TEAMWIRE_HOME/handoffs/<old
/// session id>/`. Nothing deletes the folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandoffFiles {
    pub dir: PathBuf,
    /// `handoff.md`, the document the old lead writes for the new one.
    pub document: PathBuf,
    /// An empty file: the lead was told its context passed its point.
    pub noted: PathBuf,
    /// An empty file: the lead's stop was held once after the note.
    pub held: PathBuf,
}

impl HandoffFiles {
    /// The files of `session`'s handoff under `home`. `None` when the
    /// session id is not a safe single path component: a harness's id is
    /// opaque, and one such as `../x` must not name a folder elsewhere.
    #[must_use]
    pub fn of(home: &Path, session: &SessionId) -> Option<Self> {
        let id = session.as_str();
        let safe = !id.starts_with('.')
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
        if !safe {
            return None;
        }
        let dir = home.join("handoffs").join(id);
        Some(Self {
            document: dir.join("handoff.md"),
            noted: dir.join("noted"),
            held: dir.join("held"),
            dir,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill(tokens: u64, window: u64) -> Fill {
        Fill { tokens, window }
    }

    #[test]
    fn the_lead_point_is_forty_percent_with_a_floor() {
        assert_eq!(fill(0, 1_000_000).lead_handoff_at(None), 400_000);
        // 40% of 200k is 80k, below the floor.
        assert_eq!(fill(0, 200_000).lead_handoff_at(None), 200_000);
        assert_eq!(fill(0, 1_000_000).lead_handoff_at(Some(300_000)), 300_000);
        assert_eq!(fill(0, 1_000_000).lead_handoff_at(Some(0)), 200_000);
        assert_eq!(
            fill(0, 1_000_000).lead_handoff_at(Some(150_000)),
            MIN_LEAD_HANDOFF_TOKENS
        );
    }

    #[test]
    fn the_setting_reads_a_count_off_or_nothing() {
        let parse = |v: &str| LeadHandoffSetting::parse(Some(v));
        assert_eq!(LeadHandoffSetting::parse(None), LeadHandoffSetting::Unset);
        assert_eq!(parse("  "), LeadHandoffSetting::Unset);
        assert_eq!(parse("off"), LeadHandoffSetting::Off);
        assert_eq!(parse("OFF"), LeadHandoffSetting::Off);
        assert_eq!(parse("300000"), LeadHandoffSetting::Tokens(300_000));
        assert_eq!(parse("300_000"), LeadHandoffSetting::Tokens(300_000));
        assert_eq!(parse("0"), LeadHandoffSetting::Tokens(0));
        assert_eq!(parse("abc"), LeadHandoffSetting::Invalid("abc".to_owned()));
        assert_eq!(parse("40%"), LeadHandoffSetting::Invalid("40%".to_owned()));
        assert_eq!(parse("-5"), LeadHandoffSetting::Invalid("-5".to_owned()));
        assert_eq!(parse("_"), LeadHandoffSetting::Invalid("_".to_owned()));
        assert_eq!(parse("40%").tokens(), None);
        assert_eq!(parse("7").tokens(), Some(7));
    }

    #[test]
    fn the_handoff_files_sit_in_one_folder_per_session() {
        let id = SessionId::parse("46e0fa45-9b35-4ded-b055-b86c0b68cbc4").unwrap();
        let files = HandoffFiles::of(Path::new("/h"), &id).unwrap();
        let dir = PathBuf::from("/h/handoffs/46e0fa45-9b35-4ded-b055-b86c0b68cbc4");
        assert_eq!(files.document, dir.join("handoff.md"));
        assert_eq!(files.noted, dir.join("noted"));
        assert_eq!(files.held, dir.join("held"));
        assert_eq!(files.dir, dir);
    }

    #[test]
    fn an_id_that_is_not_one_safe_path_part_has_no_files() {
        for bad in ["../x", "a/b", "..", ".hidden", "a\\b", "é"] {
            let id = SessionId::parse(bad).unwrap();
            assert!(HandoffFiles::of(Path::new("/h"), &id).is_none(), "{bad}");
        }
    }
}
