use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::model::Session;

#[derive(Debug)]
pub enum PaneError {
    Spawn(std::io::Error),
    NonZeroExit(std::process::ExitStatus, String),
    Parse(serde_json::Error),
    Ambiguous { cwd: PathBuf, count: usize },
}

impl std::fmt::Display for PaneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PaneError::Spawn(e) => write!(f, "failed to spawn herdr: {e}"),
            PaneError::NonZeroExit(status, stderr) => {
                write!(f, "herdr exited with {status}: {}", stderr.trim())
            }
            PaneError::Parse(e) => write!(f, "failed to parse herdr agent list output: {e}"),
            PaneError::Ambiguous { cwd, count } => write!(
                f,
                "{count} herdr panes run claude in {}; can't pick one",
                cwd.display()
            ),
        }
    }
}

impl std::error::Error for PaneError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PaneError::Spawn(e) => Some(e),
            PaneError::NonZeroExit(..) | PaneError::Ambiguous { .. } => None,
            PaneError::Parse(e) => Some(e),
        }
    }
}

pub trait PaneManager {
    /// Re-reads the pane list on every call rather than caching it: pane ids
    /// come and go as the human opens and closes panes. `Ok(None)` means no
    /// pane runs the session.
    fn find_pane(&self, session: &Session) -> Result<Option<String>, PaneError>;

    fn focus_pane(&self, pane_id: &str) -> Result<(), PaneError>;
}

pub struct Herdr {
    bin: PathBuf,
}

impl Herdr {
    /// `None` outside a herdr pane. Prefers the `HERDR_BIN_PATH` herdr injects
    /// over a `PATH` lookup, which may resolve to a different install.
    pub fn from_env() -> Option<Self> {
        if !is_herdr_env(std::env::var_os("HERDR_ENV").as_deref()) {
            return None;
        }
        let bin = std::env::var_os("HERDR_BIN_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("herdr"));
        Some(Self { bin })
    }

    /// Safe to call while the TUI owns the terminal: these subcommands talk to
    /// the herdr daemon over its socket, and `output()` captures both streams.
    fn run(&self, args: &[&str]) -> Result<Vec<u8>, PaneError> {
        let output = Command::new(&self.bin)
            .args(args)
            .output()
            .map_err(PaneError::Spawn)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            return Err(PaneError::NonZeroExit(output.status, stderr));
        }

        Ok(output.stdout)
    }
}

impl PaneManager for Herdr {
    fn find_pane(&self, session: &Session) -> Result<Option<String>, PaneError> {
        let stdout = self.run(&["agent", "list"])?;
        select_pane(&parse_agents(&stdout)?, session)
    }

    fn focus_pane(&self, pane_id: &str) -> Result<(), PaneError> {
        // `herdr pane focus` only moves by direction; `agent focus` is the one
        // that takes an arbitrary pane id, across workspaces and tabs.
        self.run(&["agent", "focus", pane_id]).map(|_| ())
    }
}

/// Takes the value instead of reading the variable itself so tests don't have
/// to mutate the process environment (`set_var` is unsafe in edition 2024, and
/// tests run in parallel).
fn is_herdr_env(value: Option<&OsStr>) -> bool {
    value == Some(OsStr::new("1"))
}

#[derive(Debug, Deserialize)]
struct AgentListResponse {
    result: AgentListResult,
}

#[derive(Debug, Deserialize)]
struct AgentListResult {
    agents: Vec<AgentInfo>,
}

#[derive(Debug, Clone, Deserialize)]
struct AgentInfo {
    pane_id: String,
    agent: Option<String>,
    agent_session: Option<AgentSessionInfo>,
    cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
struct AgentSessionInfo {
    kind: String,
    value: String,
}

impl AgentInfo {
    /// Only `kind: "id"` carries the Claude session UUID; the format of
    /// `kind: "path"` values is unverified, so no id is derived from them.
    fn session_id(&self) -> Option<&str> {
        self.agent_session
            .as_ref()
            .filter(|s| s.kind == "id")
            .map(|s| s.value.as_str())
    }
}

fn parse_agents(stdout: &[u8]) -> Result<Vec<AgentInfo>, PaneError> {
    serde_json::from_slice::<AgentListResponse>(stdout)
        .map(|r| r.result.agents)
        .map_err(PaneError::Parse)
}

/// Several panes can share a `cwd`, so the session id is tried first and
/// `cwd` is only a fallback — one that refuses to guess between candidates.
fn select_pane(agents: &[AgentInfo], session: &Session) -> Result<Option<String>, PaneError> {
    if let Some(agent) = agents
        .iter()
        .find(|a| a.session_id() == Some(session.session_id.as_str()))
    {
        return Ok(Some(agent.pane_id.clone()));
    }

    let candidates: Vec<&AgentInfo> = agents
        .iter()
        .filter(|a| a.agent.as_deref() == Some("claude"))
        .filter(|a| a.cwd.as_deref().is_some_and(|c| same_dir(c, &session.cwd)))
        // A pane already known to run a different session can't be this one.
        .filter(|a| a.session_id().is_none_or(|id| id == session.session_id))
        .collect();

    match candidates.as_slice() {
        [] => Ok(None),
        [one] => Ok(Some(one.pane_id.clone())),
        many => Err(PaneError::Ambiguous {
            cwd: session.cwd.clone(),
            count: many.len(),
        }),
    }
}

/// herdr and Claude Code may spell the same directory differently through a
/// symlink (`/tmp` vs `/private/tmp` on macOS).
fn same_dir(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SessionKind;

    const REAL_AGENT_LIST: &str = r##"{"id":"cli:agent:list","result":{"agents":[
        {"agent":"claude",
         "agent_session":{"agent":"claude","kind":"id","source":"herdr:claude","value":"f633dbc6-3f3e-4297-a6dd-4f07b3f4b662"},
         "agent_status":"working",
         "cwd":"/Users/m1sk9/Repositories/github.com/m1sk9/strays",
         "focused":false,
         "foreground_cwd":"/Users/m1sk9/Repositories/github.com/m1sk9/strays",
         "pane_id":"w5P:p1","revision":8,"state_change_seq":33,
         "tab_id":"w5P:t1","terminal_id":"term_65caaf53f10976",
         "terminal_title":"◐ #20のプラン計画","terminal_title_stripped":"#20のプラン計画",
         "workspace_id":"w5P"}
    ],"type":"agent_list"}}"##;

    fn session(session_id: &str, cwd: &str) -> Session {
        Session {
            id: session_id.chars().take(8).collect(),
            session_id: session_id.to_string(),
            cwd: PathBuf::from(cwd),
            kind: SessionKind::Interactive,
            started_at: 0,
            name: "name".to_string(),
            state: None,
            pid: None,
            status: None,
        }
    }

    fn agent(pane_id: &str, agent: &str, session: Option<(&str, &str)>, cwd: &str) -> AgentInfo {
        AgentInfo {
            pane_id: pane_id.to_string(),
            agent: Some(agent.to_string()),
            agent_session: session.map(|(kind, value)| AgentSessionInfo {
                kind: kind.to_string(),
                value: value.to_string(),
            }),
            cwd: Some(PathBuf::from(cwd)),
        }
    }

    #[test]
    fn is_herdr_env_only_accepts_one() {
        assert!(is_herdr_env(Some(OsStr::new("1"))));
        assert!(!is_herdr_env(None));
        assert!(!is_herdr_env(Some(OsStr::new("0"))));
        assert!(!is_herdr_env(Some(OsStr::new(""))));
    }

    #[test]
    fn parses_real_agent_list_output() {
        let agents = parse_agents(REAL_AGENT_LIST.as_bytes()).unwrap();

        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].pane_id, "w5P:p1");
        assert_eq!(
            agents[0].session_id(),
            Some("f633dbc6-3f3e-4297-a6dd-4f07b3f4b662")
        );
        assert_eq!(
            agents[0].cwd.as_deref(),
            Some(Path::new(
                "/Users/m1sk9/Repositories/github.com/m1sk9/strays"
            ))
        );
    }

    #[test]
    fn parses_agent_with_null_session_and_cwd() {
        let json = r#"{"result":{"agents":[
            {"pane_id":"w1:p1","agent":null,"agent_session":null,"cwd":null}
        ]}}"#;

        let agents = parse_agents(json.as_bytes()).unwrap();

        assert_eq!(agents[0].pane_id, "w1:p1");
        assert!(agents[0].session_id().is_none());
        assert!(agents[0].cwd.is_none());
    }

    #[test]
    fn rejects_non_json_agent_list_output() {
        assert!(matches!(
            parse_agents(b"not json"),
            Err(PaneError::Parse(_))
        ));
    }

    #[test]
    fn select_pane_prefers_exact_session_id_match_over_cwd() {
        let agents = [
            agent("w1:p1", "claude", None, "/work"),
            agent("w1:p2", "claude", Some(("id", "abc")), "/elsewhere"),
        ];

        let pane = select_pane(&agents, &session("abc", "/work")).unwrap();

        assert_eq!(pane.as_deref(), Some("w1:p2"));
    }

    #[test]
    fn select_pane_ignores_path_kind_sessions_for_id_match() {
        let agents = [agent(
            "w1:p1",
            "claude",
            Some(("path", "abc")),
            "/elsewhere",
        )];

        let pane = select_pane(&agents, &session("abc", "/work")).unwrap();

        assert_eq!(pane, None);
    }

    #[test]
    fn select_pane_falls_back_to_cwd_for_claude_agents_without_session_id() {
        let agents = [agent("w1:p1", "claude", None, "/work")];

        let pane = select_pane(&agents, &session("abc", "/work")).unwrap();

        assert_eq!(pane.as_deref(), Some("w1:p1"));
    }

    #[test]
    fn select_pane_excludes_agents_known_to_run_another_session() {
        let agents = [
            agent("w1:p1", "claude", Some(("id", "other")), "/work"),
            agent("w1:p2", "claude", None, "/work"),
        ];

        let pane = select_pane(&agents, &session("abc", "/work")).unwrap();

        assert_eq!(pane.as_deref(), Some("w1:p2"));
    }

    #[test]
    fn select_pane_ignores_non_claude_agents_in_cwd_fallback() {
        let agents = [agent("w1:p1", "codex", None, "/work")];

        let pane = select_pane(&agents, &session("abc", "/work")).unwrap();

        assert_eq!(pane, None);
    }

    #[test]
    fn select_pane_errors_when_several_panes_share_the_cwd() {
        let agents = [
            agent("w1:p1", "claude", None, "/work"),
            agent("w1:p2", "claude", None, "/work"),
        ];

        let result = select_pane(&agents, &session("abc", "/work"));

        assert!(matches!(result, Err(PaneError::Ambiguous { count: 2, .. })));
    }

    #[test]
    fn select_pane_returns_none_without_any_match() {
        let agents = [agent(
            "w1:p1",
            "claude",
            Some(("id", "other")),
            "/elsewhere",
        )];

        let pane = select_pane(&agents, &session("abc", "/work")).unwrap();

        assert_eq!(pane, None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn same_dir_resolves_symlinked_tmp_on_macos() {
        assert!(same_dir(Path::new("/tmp"), Path::new("/private/tmp")));
    }
}
