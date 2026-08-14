use std::path::Path;
use std::process::Command;

use crate::model::{Session, SessionKind};

use super::{AgentProvider, ProviderError};

pub struct ClaudeProvider;

impl AgentProvider for ClaudeProvider {
    fn list_sessions(&self) -> Result<Vec<Session>, ProviderError> {
        let output = Command::new("claude")
            .args(["agents", "--json"])
            .output()
            .map_err(ProviderError::Spawn)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            return Err(ProviderError::NonZeroExit(output.status, stderr));
        }

        parse_sessions(&output.stdout)
    }

    fn attach_command(&self, session: &Session) -> Option<Command> {
        // `claude --resume` refuses a session that's still running as a background
        // agent ("Session ... is currently running as a background agent (bg).
        // Use `claude agents` to find and attach to it, or add --fork-session to
        // branch off a copy." — observed on a live install). `claude attach <id>`
        // is the actual command for opening one in this terminal, and it's the
        // only kind this provider can safely open at all right now: an
        // `interactive` session is already running in some other terminal, and
        // jumping to that requires a pane manager like herdr, not implemented here.
        match session.kind {
            SessionKind::Background => {
                let mut command = Command::new("claude");
                command.args(["attach", &session.id]);
                Some(command)
            }
            SessionKind::Interactive | SessionKind::Unknown => None,
        }
    }

    fn fork_command(&self, session: &Session) -> Option<Command> {
        // Unlike attach, forking a still-running background session is allowed
        // (per the same error message), and there's no "already open elsewhere"
        // conflict for `interactive` either — a fork is a brand-new session, not
        // a takeover. `Unknown` is refused: forking a kind this provider doesn't
        // recognize at all isn't a call it should make silently.
        match session.kind {
            SessionKind::Background | SessionKind::Interactive => {
                let mut command = Command::new("claude");
                command.args(["--resume", &session.session_id, "--fork-session"]);
                Some(command)
            }
            SessionKind::Unknown => None,
        }
    }

    fn new_session_command(&self, path: &Path) -> Command {
        // No `--resume`: this isn't tied to any existing session.
        let mut command = Command::new("claude");
        command.current_dir(path);
        command
    }
}

fn parse_sessions(stdout: &[u8]) -> Result<Vec<Session>, ProviderError> {
    // `claude agents --json` is only documented to print `[]` for zero sessions, but
    // treat blank stdout the same way rather than surfacing it as a parse error.
    if stdout.iter().all(u8::is_ascii_whitespace) {
        return Ok(Vec::new());
    }
    serde_json::from_slice(stdout).map_err(ProviderError::Parse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{SessionKind, SessionState, SessionStatus};

    const BRIEF_SAMPLE: &str = r#"[
      {
        "id": "c0a63a21",
        "cwd": "/Users/m1sk9/Repositories/github.com/m1sk9/infra",
        "kind": "background",
        "startedAt": 1784482154257,
        "sessionId": "c0a63a21-9725-42da-bc35-b940c8d8c342",
        "name": "tailscale-acl-gitops-infra ⑂ Please create document",
        "state": "blocked"
      }
    ]"#;

    const MISSING_FIELDS_SAMPLE: &str = r#"[
      {
        "id": "703cf41c",
        "cwd": "/Users/m1sk9/Repositories/github.com/mk-system/lazy-tracker",
        "kind": "background",
        "startedAt": 1784529113398,
        "sessionId": "703cf41c-341c-409b-b4bd-0d6f503619b9",
        "name": "email-delivery-alert-banner"
      }
    ]"#;

    const UNKNOWN_STATE_SAMPLE: &str = r#"[
      {
        "id": "aaaaaaaa",
        "cwd": "/tmp/example",
        "kind": "interactive",
        "startedAt": 1,
        "sessionId": "aaaaaaaa-0000-0000-0000-000000000000",
        "name": "example",
        "state": "working"
      }
    ]"#;

    const UNKNOWN_STATUS_SAMPLE: &str = r#"[
      {
        "pid": 1,
        "cwd": "/tmp/example",
        "kind": "interactive",
        "startedAt": 1,
        "sessionId": "aaaaaaaa-0000-0000-0000-000000000000",
        "name": "example",
        "status": "thinking"
      }
    ]"#;

    const UNKNOWN_KIND_SAMPLE: &str = r#"[
      {
        "id": "aaaaaaaa",
        "cwd": "/tmp/example",
        "kind": "remote",
        "startedAt": 1,
        "sessionId": "aaaaaaaa-0000-0000-0000-000000000000",
        "name": "example"
      }
    ]"#;

    // Real `claude agents --json` output for an interactive session: no `id`, has `pid`/`status`.
    const INTERACTIVE_MISSING_ID_SAMPLE: &str = r#"[
      {
        "pid": 82425,
        "cwd": "/Users/m1sk9/Repositories/github.com/m1sk9/strays",
        "kind": "interactive",
        "startedAt": 1786537558762,
        "sessionId": "f7209409-4258-462c-8e87-56f695e89bc0",
        "name": "strays-tui-implementation",
        "status": "busy"
      }
    ]"#;

    #[test]
    fn missing_id_is_derived_from_session_id() {
        let sessions: Vec<Session> = serde_json::from_str(INTERACTIVE_MISSING_ID_SAMPLE).unwrap();
        let session = &sessions[0];
        assert_eq!(session.id, "f7209409");
        assert_eq!(session.kind, SessionKind::Interactive);
        assert_eq!(session.pid, Some(82425));
        assert_eq!(session.state, None);
        assert_eq!(session.status, Some(SessionStatus::Busy));
    }

    #[test]
    fn parses_session_with_state() {
        let sessions: Vec<Session> = serde_json::from_str(BRIEF_SAMPLE).unwrap();
        let session = &sessions[0];
        assert_eq!(session.kind, SessionKind::Background);
        assert_eq!(session.state, Some(SessionState::Blocked));
        assert_eq!(session.pid, None);
        assert_eq!(session.status, None);
    }

    #[test]
    fn missing_state_pid_status_deserialize_to_none() {
        let sessions: Vec<Session> = serde_json::from_str(MISSING_FIELDS_SAMPLE).unwrap();
        let session = &sessions[0];
        assert_eq!(session.state, None);
        assert_eq!(session.pid, None);
        assert_eq!(session.status, None);
    }

    #[test]
    fn unknown_state_falls_back_to_other() {
        let sessions: Vec<Session> = serde_json::from_str(UNKNOWN_STATE_SAMPLE).unwrap();
        assert_eq!(
            sessions[0].state,
            Some(SessionState::Other("working".to_string()))
        );
    }

    #[test]
    fn unknown_kind_falls_back_to_other() {
        let sessions: Vec<Session> = serde_json::from_str(UNKNOWN_KIND_SAMPLE).unwrap();
        assert_eq!(sessions[0].kind, SessionKind::Unknown);
    }

    #[test]
    fn unknown_status_falls_back_to_other() {
        let sessions: Vec<Session> = serde_json::from_str(UNKNOWN_STATUS_SAMPLE).unwrap();
        assert_eq!(
            sessions[0].status,
            Some(SessionStatus::Other("thinking".to_string()))
        );
    }

    #[test]
    fn blank_stdout_is_treated_as_no_sessions() {
        assert!(parse_sessions(b"").unwrap().is_empty());
        assert!(parse_sessions(b"   \n").unwrap().is_empty());
    }

    fn session(kind: SessionKind) -> Session {
        Session {
            id: "id".into(),
            session_id: "session-id".into(),
            cwd: "/tmp".into(),
            kind,
            started_at: 0,
            name: "name".into(),
            state: None,
            pid: None,
            status: None,
        }
    }

    #[test]
    fn attach_command_opens_background_sessions_by_short_id() {
        let provider = ClaudeProvider;
        let command = provider
            .attach_command(&session(SessionKind::Background))
            .expect("background sessions should be attachable");

        let args: Vec<&str> = command.get_args().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(args, ["attach", "id"]);
    }

    #[test]
    fn attach_command_refuses_interactive_and_unknown_sessions() {
        let provider = ClaudeProvider;
        assert!(
            provider
                .attach_command(&session(SessionKind::Interactive))
                .is_none()
        );
        assert!(
            provider
                .attach_command(&session(SessionKind::Unknown))
                .is_none()
        );
    }

    #[test]
    fn fork_command_builds_expected_args_for_background_and_interactive() {
        let provider = ClaudeProvider;

        for kind in [SessionKind::Background, SessionKind::Interactive] {
            let command = provider
                .fork_command(&session(kind))
                .unwrap_or_else(|| panic!("{kind:?} sessions should be forkable"));
            let args: Vec<&str> = command.get_args().map(|a| a.to_str().unwrap()).collect();
            assert_eq!(args, ["--resume", "session-id", "--fork-session"]);
        }
    }

    #[test]
    fn fork_command_refuses_unknown_sessions() {
        let provider = ClaudeProvider;
        assert!(
            provider
                .fork_command(&session(SessionKind::Unknown))
                .is_none()
        );
    }

    #[test]
    fn new_session_command_has_no_resume_flag_and_sets_the_working_directory() {
        let provider = ClaudeProvider;
        let command = provider.new_session_command(Path::new("/tmp"));

        assert!(command.get_args().next().is_none());
        assert_eq!(command.get_current_dir(), Some(Path::new("/tmp")));
    }
}
