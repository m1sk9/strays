use std::path::PathBuf;
use std::process::Command;

use crate::action;
use crate::model::Session;
use crate::provider::{AgentProvider, ClaudeProvider};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    ConfirmKill,
    NewSession,
}

pub struct App {
    pub sessions: Vec<Session>,
    pub mode: Mode,
    pub status_message: Option<String>,
    pub should_quit: bool,
    /// Path being typed in `Mode::NewSession`; byte offset into it, always kept
    /// on a UTF-8 char boundary.
    pub input: String,
    pub input_cursor: usize,
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
            input: String::new(),
            input_cursor: 0,
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

    /// Enters the new-session input mode, prefilled with the selected row's
    /// `cwd` so opening another session in the same project is one keystroke
    /// away rather than retyping the whole path.
    pub fn start_new_session_input(&mut self) {
        self.mode = Mode::NewSession;
        self.status_message = None;
        self.input = self
            .selected_session()
            .map(|s| s.cwd.display().to_string())
            .unwrap_or_default();
        self.input_cursor = self.input.len();
    }

    pub fn cancel_new_session_input(&mut self) {
        self.mode = Mode::Normal;
        self.status_message = None;
        self.input.clear();
        self.input_cursor = 0;
    }

    pub fn input_insert(&mut self, c: char) {
        self.input.insert(self.input_cursor, c);
        self.input_cursor += c.len_utf8();
    }

    pub fn input_backspace(&mut self) {
        let Some(previous) = self.prev_char_boundary() else {
            return;
        };
        self.input.drain(previous..self.input_cursor);
        self.input_cursor = previous;
    }

    pub fn input_move_left(&mut self) {
        if let Some(previous) = self.prev_char_boundary() {
            self.input_cursor = previous;
        }
    }

    pub fn input_move_right(&mut self) {
        if let Some(next) = self.next_char_boundary() {
            self.input_cursor = next;
        }
    }

    fn prev_char_boundary(&self) -> Option<usize> {
        self.input[..self.input_cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
    }

    fn next_char_boundary(&self) -> Option<usize> {
        self.input[self.input_cursor..]
            .char_indices()
            .nth(1)
            .map(|(i, _)| self.input_cursor + i)
            .or_else(|| (self.input_cursor < self.input.len()).then_some(self.input.len()))
    }

    /// Validates the typed path and returns to normal mode on success. Leaves
    /// `Mode::NewSession` (and the typed input) untouched on failure so the
    /// user can fix a typo instead of retyping the whole path.
    pub fn confirm_new_session_input(&mut self) -> Option<PathBuf> {
        let path = PathBuf::from(self.input.trim());
        if !path.is_dir() {
            self.status_message = Some(format!("not a directory: {}", path.display()));
            return None;
        }

        self.mode = Mode::Normal;
        self.status_message = None;
        self.input.clear();
        self.input_cursor = 0;
        Some(path)
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

    #[test]
    fn input_insert_advances_cursor_by_char_len_not_one() {
        let mut app = App::new();
        app.input = "ab".to_string();
        app.input_cursor = 1;

        app.input_insert('日');

        assert_eq!(app.input, "a日b");
        assert_eq!(app.input_cursor, 1 + '日'.len_utf8());
    }

    #[test]
    fn input_backspace_removes_a_whole_multibyte_char() {
        let mut app = App::new();
        app.input = "a日".to_string();
        app.input_cursor = app.input.len();

        app.input_backspace();

        assert_eq!(app.input, "a");
        assert_eq!(app.input_cursor, 1);
    }

    #[test]
    fn input_backspace_at_start_does_nothing() {
        let mut app = App::new();
        app.input = "abc".to_string();
        app.input_cursor = 0;

        app.input_backspace();

        assert_eq!(app.input, "abc");
        assert_eq!(app.input_cursor, 0);
    }

    #[test]
    fn input_move_left_and_right_skip_whole_chars() {
        let mut app = App::new();
        app.input = "a日b".to_string();
        app.input_cursor = app.input.len();

        app.input_move_left();
        assert_eq!(app.input_cursor, 1 + '日'.len_utf8());
        app.input_move_left();
        assert_eq!(app.input_cursor, 1);
        app.input_move_left();
        assert_eq!(app.input_cursor, 0);
        app.input_move_left();
        assert_eq!(app.input_cursor, 0, "already at start");

        app.input_move_right();
        assert_eq!(app.input_cursor, 1);
        app.input_move_right();
        assert_eq!(app.input_cursor, 1 + '日'.len_utf8());
        app.input_move_right();
        assert_eq!(app.input_cursor, app.input.len());
        app.input_move_right();
        assert_eq!(app.input_cursor, app.input.len(), "already at end");
    }
}
