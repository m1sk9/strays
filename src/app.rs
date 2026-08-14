use crate::model::Session;
use crate::provider::{AgentProvider, ClaudeProvider};

pub enum Mode {
    Normal,
}

pub struct App {
    pub sessions: Vec<Session>,
    pub selected: usize,
    pub mode: Mode,
    pub status_message: Option<String>,
    pub should_quit: bool,
    provider: ClaudeProvider,
}

impl App {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            selected: 0,
            mode: Mode::Normal,
            status_message: None,
            should_quit: false,
            provider: ClaudeProvider,
        }
    }

    pub fn refresh(&mut self) {
        match self.provider.list_sessions() {
            Ok(sessions) => {
                self.selected = self.selected.min(sessions.len().saturating_sub(1));
                self.sessions = sessions;
                self.status_message = None;
            }
            Err(err) => {
                self.status_message = Some(format!("failed to refresh: {err}"));
            }
        }
    }

    pub fn select_next(&mut self) {
        if !self.sessions.is_empty() {
            self.selected = (self.selected + 1) % self.sessions.len();
        }
    }

    pub fn select_previous(&mut self) {
        if !self.sessions.is_empty() {
            self.selected = (self.selected + self.sessions.len() - 1) % self.sessions.len();
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
