mod claude;

pub use claude::ClaudeProvider;

use std::process::Command;

use crate::model::Session;

#[derive(Debug)]
pub enum ProviderError {
    Spawn(std::io::Error),
    NonZeroExit(std::process::ExitStatus),
    Parse(serde_json::Error),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::Spawn(e) => write!(f, "failed to spawn claude: {e}"),
            ProviderError::NonZeroExit(status) => write!(f, "claude exited with {status}"),
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
            ProviderError::NonZeroExit(_) => None,
            ProviderError::Parse(e) => Some(e),
        }
    }
}

pub trait AgentProvider {
    fn list_sessions(&self) -> Result<Vec<Session>, ProviderError>;
    fn resume_command(&self, session: &Session, fork: bool) -> Command;
}
