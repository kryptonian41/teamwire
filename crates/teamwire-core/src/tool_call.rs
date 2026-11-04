//! One tool call, as a lead's tool hook reports it (ADR 0035): whose call it
//! is, and what it does, in a form every harness can give.

use std::path::PathBuf;

use crate::names::SessionId;

/// A tool-use hook's input, in neutral form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub session: SessionId,
    /// A subagent made the call, not the session's own model.
    pub subagent: bool,
    /// The directory the harness runs in. A relative path in `action` is
    /// read against it.
    pub cwd: Option<PathBuf>,
    pub action: ToolAction,
}

/// What a tool call does, as far as the lead's tool hook needs to know.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolAction {
    /// The call writes these files: an edit, a new file, a notebook or a patch.
    Edit(Vec<PathBuf>),
    /// The call runs this shell command line.
    Shell(String),
    /// Any other call, such as a read or a search.
    Other,
}
