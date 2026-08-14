mod app;
mod model;
mod provider;
mod ui;

use std::io;
use std::time::Duration;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use app::App;

fn main() -> io::Result<()> {
    let mut terminal = ratatui::try_init()?;
    let mut app = App::new();
    app.refresh();

    let result = run(&mut terminal, &mut app);

    ratatui::restore();
    result
}

fn run(terminal: &mut DefaultTerminal, app: &mut App) -> io::Result<()> {
    while !app.should_quit {
        terminal.draw(|frame| ui::draw(frame, app))?;

        if event::poll(Duration::from_millis(250))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            handle_key(app, key);
        }
    }
    Ok(())
}

fn handle_key(app: &mut App, key: KeyEvent) {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.should_quit = true;
        return;
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Down | KeyCode::Char('j') => app.select_next(),
        KeyCode::Up | KeyCode::Char('k') => app.select_previous(),
        KeyCode::Char('r') => app.refresh(),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::model::{Session, SessionKind};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn session(id: &str) -> Session {
        Session {
            id: id.to_string(),
            session_id: id.to_string(),
            cwd: PathBuf::from("/tmp"),
            kind: SessionKind::Background,
            started_at: 0,
            name: "name".to_string(),
            state: None,
            pid: None,
            status: None,
        }
    }

    #[test]
    fn q_and_esc_quit() {
        let mut app = App::new();
        handle_key(&mut app, key(KeyCode::Char('q')));
        assert!(app.should_quit);

        let mut app = App::new();
        handle_key(&mut app, key(KeyCode::Esc));
        assert!(app.should_quit);
    }

    #[test]
    fn ctrl_c_quits() {
        let mut app = App::new();
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        );
        assert!(app.should_quit);
    }

    #[test]
    fn plain_c_does_not_quit() {
        let mut app = App::new();
        handle_key(&mut app, key(KeyCode::Char('c')));
        assert!(!app.should_quit);
    }

    #[test]
    fn j_and_down_advance_selection() {
        let mut app = App::new();
        app.sessions = vec![session("a"), session("b"), session("c")];

        handle_key(&mut app, key(KeyCode::Char('j')));
        assert_eq!(app.selected(), Some(0));

        handle_key(&mut app, key(KeyCode::Down));
        assert_eq!(app.selected(), Some(1));
    }

    #[test]
    fn k_and_up_retreat_selection() {
        let mut app = App::new();
        app.sessions = vec![session("a"), session("b"), session("c")];
        handle_key(&mut app, key(KeyCode::Char('j')));

        handle_key(&mut app, key(KeyCode::Char('k')));
        assert_eq!(app.selected(), Some(2));

        handle_key(&mut app, key(KeyCode::Up));
        assert_eq!(app.selected(), Some(1));
    }
}
