use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::process::Command;

use serde::Deserialize;

use crate::model::Session;

#[derive(Debug)]
pub enum PaneError {
    Spawn(std::io::Error),
    NonZeroExit(std::process::ExitStatus, String),
    Parse(serde_json::Error),
}

impl std::fmt::Display for PaneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PaneError::Spawn(e) => write!(f, "failed to spawn herdr: {e}"),
            PaneError::NonZeroExit(status, stderr) => {
                write!(f, "herdr exited with {status}: {}", stderr.trim())
            }
            PaneError::Parse(e) => write!(f, "failed to parse herdr agent list output: {e}"),
        }
    }
}

impl std::error::Error for PaneError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PaneError::Spawn(e) => Some(e),
            PaneError::NonZeroExit(..) => None,
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
        Self::from_env_values(
            std::env::var_os("HERDR_ENV").as_deref(),
            std::env::var_os("HERDR_BIN_PATH"),
        )
    }

    /// Takes the values instead of reading the variables itself so tests don't
    /// have to mutate the process environment (`set_var` is unsafe in edition
    /// 2024, and tests run in parallel).
    fn from_env_values(herdr_env: Option<&OsStr>, bin_path: Option<OsString>) -> Option<Self> {
        if herdr_env != Some(OsStr::new("1")) {
            return None;
        }
        let bin = bin_path
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
        Ok(select_pane(&parse_agents(&stdout)?, session))
    }

    fn focus_pane(&self, pane_id: &str) -> Result<(), PaneError> {
        // `herdr pane focus` only moves by direction; `agent focus` is the one
        // that takes an arbitrary pane id, across workspaces and tabs.
        self.run(&["agent", "focus", pane_id]).map(|_| ())
    }
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
    agent_session: Option<AgentSessionInfo>,
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

/// Matches by session id only. A `cwd` fallback was dropped: when the session
/// runs in a terminal outside herdr, a lone claude pane in the same directory
/// is an unrelated session, and focusing it would be silently wrong.
fn select_pane(agents: &[AgentInfo], session: &Session) -> Option<String> {
    agents
        .iter()
        .find(|a| a.session_id() == Some(session.session_id.as_str()))
        .map(|a| a.pane_id.clone())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::sync::OnceLock;

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

    fn session(session_id: &str) -> Session {
        Session {
            id: session_id.chars().take(8).collect(),
            session_id: session_id.to_string(),
            cwd: PathBuf::from("/work"),
            kind: SessionKind::Interactive,
            started_at: 0,
            name: "name".to_string(),
            state: None,
            pid: None,
            status: None,
        }
    }

    fn agent(pane_id: &str, session: Option<(&str, &str)>) -> AgentInfo {
        AgentInfo {
            pane_id: pane_id.to_string(),
            agent_session: session.map(|(kind, value)| AgentSessionInfo {
                kind: kind.to_string(),
                value: value.to_string(),
            }),
        }
    }

    /// Stands in for the herdr CLI. Every script is written once, before any
    /// test spawns one: executing a file while another thread still holds it
    /// open for writing fails with ETXTBSY on Linux.
    struct FakeHerdr {
        ok: PathBuf,
        garbage: PathBuf,
    }

    fn fake_herdr() -> &'static FakeHerdr {
        static FAKE: OnceLock<FakeHerdr> = OnceLock::new();
        FAKE.get_or_init(|| {
            let dir = std::env::temp_dir().join(format!("strays-fake-herdr-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let write = |name: &str, body: &str| {
                let path = dir.join(name);
                std::fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
                path
            };
            FakeHerdr {
                ok: write(
                    "ok",
                    r#"case "$1 $2 $3" in
  "agent list ") printf '%s' '{"result":{"agents":[{"pane_id":"w1:p1","agent_session":{"kind":"id","value":"abc"}}]}}' ;;
  "agent focus w1:p1") ;;
  *) echo '{"error":{"code":"agent_not_found"}}' >&2; exit 1 ;;
esac
"#,
                ),
                garbage: write("garbage", "echo not json\n"),
            }
        })
    }

    fn herdr_at(bin: &Path) -> Herdr {
        Herdr::from_env_values(Some(OsStr::new("1")), Some(bin.into())).unwrap()
    }

    #[test]
    fn from_env_values_requires_herdr_env_to_be_one() {
        assert!(Herdr::from_env_values(Some(OsStr::new("1")), None).is_some());
        assert!(Herdr::from_env_values(None, None).is_none());
        assert!(Herdr::from_env_values(Some(OsStr::new("0")), None).is_none());
        assert!(Herdr::from_env_values(Some(OsStr::new("")), None).is_none());
    }

    #[test]
    fn from_env_values_prefers_herdr_bin_path_over_path_lookup() {
        let herdr =
            Herdr::from_env_values(Some(OsStr::new("1")), Some("/opt/herdr".into())).unwrap();
        assert_eq!(herdr.bin, Path::new("/opt/herdr"));

        let herdr = Herdr::from_env_values(Some(OsStr::new("1")), None).unwrap();
        assert_eq!(herdr.bin, Path::new("herdr"));
    }

    #[test]
    fn find_pane_matches_the_session_in_herdr_agent_list() {
        let herdr = herdr_at(&fake_herdr().ok);

        assert_eq!(
            herdr.find_pane(&session("abc")).unwrap().as_deref(),
            Some("w1:p1")
        );
        assert_eq!(herdr.find_pane(&session("other")).unwrap(), None);
    }

    #[test]
    fn find_pane_reports_unparsable_agent_list_output() {
        let herdr = herdr_at(&fake_herdr().garbage);

        assert!(matches!(
            herdr.find_pane(&session("abc")),
            Err(PaneError::Parse(_))
        ));
    }

    #[test]
    fn focus_pane_runs_herdr_agent_focus() {
        let herdr = herdr_at(&fake_herdr().ok);

        assert!(herdr.focus_pane("w1:p1").is_ok());
    }

    #[test]
    fn focus_pane_reports_herdr_stderr_on_failure() {
        let herdr = herdr_at(&fake_herdr().ok);

        let err = herdr.focus_pane("w9:p9").unwrap_err();

        assert!(matches!(err, PaneError::NonZeroExit(..)));
        assert!(err.to_string().contains("agent_not_found"));
    }

    #[test]
    fn missing_herdr_binary_is_a_spawn_error() {
        let herdr = herdr_at(Path::new("/definitely/not/a/real/herdr"));

        assert!(matches!(
            herdr.focus_pane("w1:p1"),
            Err(PaneError::Spawn(_))
        ));
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
    }

    #[test]
    fn parses_agent_with_null_session() {
        let json = r#"{"result":{"agents":[
            {"pane_id":"w1:p1","agent":null,"agent_session":null,"cwd":null}
        ]}}"#;

        let agents = parse_agents(json.as_bytes()).unwrap();

        assert_eq!(agents[0].pane_id, "w1:p1");
        assert!(agents[0].session_id().is_none());
    }

    #[test]
    fn rejects_non_json_agent_list_output() {
        assert!(matches!(
            parse_agents(b"not json"),
            Err(PaneError::Parse(_))
        ));
    }

    #[test]
    fn select_pane_finds_the_pane_running_the_session_id() {
        let agents = [
            agent("w1:p1", Some(("id", "other"))),
            agent("w1:p2", Some(("id", "abc"))),
        ];

        assert_eq!(
            select_pane(&agents, &session("abc")).as_deref(),
            Some("w1:p2")
        );
    }

    #[test]
    fn select_pane_ignores_path_kind_sessions() {
        let agents = [agent("w1:p1", Some(("path", "abc")))];

        assert_eq!(select_pane(&agents, &session("abc")), None);
    }

    #[test]
    fn select_pane_never_guesses_a_pane_without_a_session_id() {
        let agents = [agent("w1:p1", None)];

        assert_eq!(select_pane(&agents, &session("abc")), None);
    }
}
