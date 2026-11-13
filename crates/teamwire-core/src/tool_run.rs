//! The tool call a member's session is running now, as `status --role`
//! shows it (F10): its name and when it started, in a form every harness
//! can give. A harness that cannot tell gives none, never a guess.

use serde::{Deserialize, Serialize};

use crate::timestamp::Timestamp;

/// A tool call that started and has no result yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolRun {
    /// The tool's name as the harness records it, such as `Bash`.
    pub name: String,
    pub started_at: Timestamp,
}
