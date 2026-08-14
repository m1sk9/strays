use crate::model::Session;
use crate::provider::{AgentProvider, ClaudeProvider};

pub enum Mode {
    Normal,
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
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
