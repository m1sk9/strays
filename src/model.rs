use std::path::PathBuf;

use serde::{Deserialize, Deserializer};

#[derive(Debug, Clone, Deserialize)]
#[serde(from = "RawSession")]
pub struct Session {
    pub id: String,
    pub session_id: String,
    pub cwd: PathBuf,
    pub kind: SessionKind,
    pub started_at: u64,
    pub name: String,
    pub state: Option<SessionState>,
    pub pid: Option<u32>,
    pub status: Option<serde_json::Value>,
}

/// `id` is absent from `claude agents --json` output when `kind` is `"interactive"`;
/// derive it from `sessionId`'s first 8 chars instead of leaving `Session::id` optional.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSession {
    id: Option<String>,
    session_id: String,
    cwd: PathBuf,
    kind: SessionKind,
    started_at: u64,
    name: String,
    state: Option<SessionState>,
    pid: Option<u32>,
    status: Option<serde_json::Value>,
}

impl From<RawSession> for Session {
    fn from(raw: RawSession) -> Self {
        let id = raw
            .id
            .unwrap_or_else(|| raw.session_id.chars().take(8).collect());
        Session {
            id,
            session_id: raw.session_id,
            cwd: raw.cwd,
            kind: raw.kind,
            started_at: raw.started_at,
            name: raw.name,
            state: raw.state,
            pid: raw.pid,
            status: raw.status,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionKind {
    Background,
    Interactive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionState {
    Blocked,
    /// Only `"blocked"` has been observed; keep unknown values instead of failing.
    Other(String),
}

impl<'de> Deserialize<'de> for SessionState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.as_str() {
            "blocked" => SessionState::Blocked,
            _ => SessionState::Other(raw),
        })
    }
}
