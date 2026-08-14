mod claude;

pub use claude::ClaudeProvider;

use std::process::Command;

use crate::model::Session;

#[derive(Debug)]
pub enum ProviderError {
    Spawn(std::io::Error),
    NonZeroExit(std::process::ExitStatus, String),
    Parse(serde_json::Error),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::Spawn(e) => write!(f, "failed to spawn claude: {e}"),
            ProviderError::NonZeroExit(status, stderr) => {
                write!(f, "claude exited with {status}: {stderr}")
            }
            ProviderError::Parse(e) => {
                write!(f, "failed to parse claude agents --json output: {e}")
            }
        }
    }
}

impl std::error::Error for ProviderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ProviderError::Spawn(e) => Some(e),
            ProviderError::NonZeroExit(..) => None,
            ProviderError::Parse(e) => Some(e),
        }
    }
}

pub trait AgentProvider {
    fn list_sessions(&self) -> Result<Vec<Session>, ProviderError>;

    /// `None` when the session can't be safely opened in this terminal (e.g. it's
    /// already interactive somewhere else — jumping to that requires a pane
    /// manager like herdr, not yet supported here).
    fn attach_command(&self, session: &Session) -> Option<Command>;

    /// `None` when this provider doesn't understand the session's kind well
    /// enough to fork it safely.
    fn fork_command(&self, session: &Session) -> Option<Command>;
}
