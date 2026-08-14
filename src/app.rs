use std::process::Command;

use crate::action;
use crate::model::Session;
use crate::provider::{AgentProvider, ClaudeProvider};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    ConfirmKill,
}

pub struct App {
    pub sessions: Vec<Session>,
    pub mode: Mode,
    pub status_message: Option<String>,
    pub should_quit: bool,
    selected_session_id: Option<String>,
    provider: Box<dyn AgentProvider>,
}

impl App {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            mode: Mode::Normal,
            status_message: None,
            should_quit: false,
            selected_session_id: None,
            provider: Box::new(ClaudeProvider),
        }
    }

    pub fn refresh(&mut self) {
        match self.provider.list_sessions() {
            Ok(sessions) => {
                if self.selected_session_id.is_none() {
                    self.selected_session_id = sessions.first().map(|s| s.session_id.clone());
                }
                self.sessions = sessions;
                self.status_message = None;
            }
            Err(err) => {
                self.status_message = Some(format!("failed to refresh: {err}"));
            }
        }
    }

    /// Resolved by session identity rather than a raw position, so a session list
    /// that gets reordered or shrunk on refresh doesn't silently highlight the
    /// wrong row.
    pub fn selected(&self) -> Option<usize> {
        let id = self.selected_session_id.as_deref()?;
        self.sessions.iter().position(|s| s.session_id == id)
    }

    pub fn selected_session(&self) -> Option<&Session> {
        self.selected().map(|i| &self.sessions[i])
    }

    pub fn attach_command(&self, session: &Session) -> Option<Command> {
        self.provider.attach_command(session)
    }

    pub fn fork_command(&self, session: &Session) -> Option<Command> {
        self.provider.fork_command(session)
    }

    pub fn select_next(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        let next = self.selected().map_or(0, |i| (i + 1) % self.sessions.len());
        self.selected_session_id = Some(self.sessions[next].session_id.clone());
    }

    pub fn select_previous(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        let previous = self
            .selected()
            .map_or(0, |i| (i + self.sessions.len() - 1) % self.sessions.len());
        self.selected_session_id = Some(self.sessions[previous].session_id.clone());
    }

    /// Enters the kill confirmation mode, unless the selected session has no
    /// `pid` (nothing to send a signal to).
    pub fn request_kill(&mut self) {
        match self.selected_session().and_then(|s| s.pid) {
            Some(_) => self.mode = Mode::ConfirmKill,
            None => {
                self.status_message = Some("cannot kill: no pid for this session".to_string());
            }
        }
    }

    pub fn cancel_kill(&mut self) {
        self.mode = Mode::Normal;
    }

    pub fn confirm_kill(&mut self) {
        self.mode = Mode::Normal;
        let Some(pid) = self.selected_session().and_then(|s| s.pid) else {
            return;
        };

        match action::kill(pid) {
            Ok(status) if status.success() => {
                // A successful exit only means the signal was delivered, not that
                // the process has actually exited yet (it may ignore SIGTERM or
                // take time to clean up), so this can't claim the session is gone.
                self.status_message = Some(format!("sent SIGTERM to pid {pid}"));
                self.refresh();
            }
            Ok(status) => {
                self.status_message = Some(format!("kill exited with {status}"));
            }
            Err(err) => {
                self.status_message = Some(format!("failed to kill pid {pid}: {err}"));
            }
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::model::SessionKind;
    use crate::provider::ProviderError;

    struct FakeProvider {
        sessions: Vec<Session>,
    }

    impl AgentProvider for FakeProvider {
        fn list_sessions(&self) -> Result<Vec<Session>, ProviderError> {
            Ok(self.sessions.clone())
        }

        fn attach_command(&self, _session: &Session) -> Option<Command> {
            Some(Command::new("true"))
        }

        fn fork_command(&self, _session: &Session) -> Option<Command> {
            Some(Command::new("true"))
        }
    }

    fn session(id: &str) -> Session {
        session_with_pid(id, None)
    }

    fn session_with_pid(id: &str, pid: Option<u32>) -> Session {
        Session {
            id: id.to_string(),
            session_id: id.to_string(),
            cwd: PathBuf::from("/tmp"),
            kind: SessionKind::Background,
            started_at: 0,
            name: "name".to_string(),
            state: None,
            pid,
            status: None,
        }
    }

    #[test]
    fn select_next_and_previous_wrap_around() {
        let mut app = App::new();
        app.sessions = vec![session("a"), session("b"), session("c")];
        app.selected_session_id = Some("a".to_string());

        app.select_next();
        assert_eq!(app.selected(), Some(1));
        app.select_next();
        assert_eq!(app.selected(), Some(2));
        app.select_next();
        assert_eq!(app.selected(), Some(0));

        app.select_previous();
        assert_eq!(app.selected(), Some(2));
    }

    #[test]
    fn select_on_empty_sessions_does_nothing() {
        let mut app = App::new();
        app.select_next();
        assert_eq!(app.selected(), None);
        app.select_previous();
        assert_eq!(app.selected(), None);
    }

    #[test]
    fn selected_returns_none_when_session_vanishes() {
        let mut app = App::new();
        app.sessions = vec![session("a")];
        app.selected_session_id = Some("gone".to_string());

        assert_eq!(app.selected(), None);
    }

    #[test]
    fn refresh_tracks_selection_by_id_across_reorder() {
        let mut app = App {
            provider: Box::new(FakeProvider {
                sessions: vec![session("a"), session("b")],
            }),
            ..App::new()
        };

        app.refresh();
        app.selected_session_id = Some("b".to_string());
        assert_eq!(app.selected(), Some(1));

        app.provider = Box::new(FakeProvider {
            sessions: vec![session("b"), session("a")],
        });
        app.refresh();

        assert_eq!(app.selected(), Some(0));
    }

    #[test]
    fn request_kill_enters_confirm_mode_when_pid_present() {
        let mut app = App::new();
        app.sessions = vec![session_with_pid("a", Some(123))];
        app.selected_session_id = Some("a".to_string());

        app.request_kill();

        assert_eq!(app.mode, Mode::ConfirmKill);
    }

    #[test]
    fn request_kill_is_refused_without_pid() {
        let mut app = App::new();
        app.sessions = vec![session("a")];
        app.selected_session_id = Some("a".to_string());

        app.request_kill();

        assert_eq!(app.mode, Mode::Normal);
        assert!(app.status_message.is_some());
    }

    #[test]
    fn confirm_kill_without_pid_does_nothing() {
        let mut app = App::new();
        app.sessions = vec![session("a")];
        app.selected_session_id = Some("a".to_string());
        app.mode = Mode::ConfirmKill;

        app.confirm_kill();

        assert_eq!(app.mode, Mode::Normal);
        assert!(app.status_message.is_none());
    }

    #[test]
    fn cancel_kill_returns_to_normal_mode() {
        let mut app = App::new();
        app.mode = Mode::ConfirmKill;

        app.cancel_kill();

        assert_eq!(app.mode, Mode::Normal);
    }
}
