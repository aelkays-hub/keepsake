use std::path::PathBuf;

use thiserror::Error;

/// Errors from probing media.
#[derive(Debug, Error)]
pub enum ProbeError {
    #[error("{tool} not found at {path}")]
    ToolMissing { tool: &'static str, path: PathBuf },

    #[error("could not start {tool}: {source}")]
    Spawn {
        tool: &'static str,
        source: std::io::Error,
    },

    #[error("{tool} did not answer within {secs} s")]
    Timeout { tool: &'static str, secs: u64 },

    #[error("{tool} stopped unexpectedly: {detail}")]
    Crashed { tool: &'static str, detail: String },

    #[error("{tool} failed: {detail}")]
    Tool { tool: &'static str, detail: String },

    #[error("cannot parse {what}: {detail}")]
    Parse { what: &'static str, detail: String },

    #[error("invalid path for {tool}: {path}")]
    BadPath { tool: &'static str, path: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl ProbeError {
    pub(crate) fn parse(what: &'static str, detail: impl ToString) -> Self {
        ProbeError::Parse {
            what,
            detail: detail.to_string(),
        }
    }
}
