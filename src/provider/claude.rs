use std::process::Command;

use crate::model::Session;

use super::{AgentProvider, ProviderError};

pub struct ClaudeProvider;

impl AgentProvider for ClaudeProvider {
    fn list_sessions(&self) -> Result<Vec<Session>, ProviderError> {
        let output = Command::new("claude")
            .args(["agents", "--json"])
            .output()
            .map_err(ProviderError::Spawn)?;

        if !output.status.success() {
            return Err(ProviderError::NonZeroExit(output.status));
        }

        serde_json::from_slice(&output.stdout).map_err(ProviderError::Parse)
    }

    fn resume_command(&self, session: &Session, fork: bool) -> Command {
        let mut command = Command::new("claude");
        command.args(["--resume", &session.session_id]);
        if fork {
            command.arg("--fork-session");
        }
        command
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{SessionKind, SessionState};

    const BRIEF_SAMPLE: &str = r#"[
      {
        "id": "c0a63a21",
        "cwd": "/Users/m1sk9/Repositories/github.com/m1sk9/infra",
        "kind": "background",
        "startedAt": 1784482154257,
        "sessionId": "c0a63a21-9725-42da-bc35-b940c8d8c342",
        "name": "tailscale-acl-gitops-infra ⑂ 手順書を作成して",
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
    fn resume_command_builds_expected_args() {
        let session = Session {
            id: "id".into(),
            session_id: "session-id".into(),
            cwd: "/tmp".into(),
            kind: SessionKind::Background,
            started_at: 0,
            name: "name".into(),
            state: None,
            pid: None,
            status: None,
        };

        let provider = ClaudeProvider;

        let resume = provider.resume_command(&session, false);
        let resume_args: Vec<&str> = resume.get_args().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(resume_args, ["--resume", "session-id"]);

        let fork = provider.resume_command(&session, true);
        let fork_args: Vec<&str> = fork.get_args().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(fork_args, ["--resume", "session-id", "--fork-session"]);
    }
}
