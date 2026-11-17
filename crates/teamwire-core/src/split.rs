//! `SplitRequest`: what `Host::split_run` needs to run one command in a new
//! pane beside another. The lead handoff starts a fresh lead session this
//! way, as a plain command that holds no teamwire agent name (ADR 0051).

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::secret::RedactedEnv;

/// One command to run in a new pane. `Debug` hides secret values in `env`,
/// as `SpawnRequest` does.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitRequest {
    /// The new pane's label, such as `lead · Fix the prompts`.
    pub label: String,
    /// The working directory of the new pane.
    pub cwd: PathBuf,
    /// The program and its arguments. `argv[0]` is the program.
    pub argv: Vec<String>,
    /// Variables set for the program on top of the pane's own environment.
    pub env: Vec<(String, String)>,
}

impl fmt::Debug for SplitRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SplitRequest")
            .field("label", &self.label)
            .field("cwd", &self.cwd)
            .field("argv", &self.argv)
            .field("env", &RedactedEnv(&self.env))
            .finish()
    }
}
