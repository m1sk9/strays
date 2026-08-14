mod action;
mod app;
mod model;
mod provider;
mod ui;

use std::io;
use std::time::Duration;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use app::{App, Mode};

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
            && let Some(fork) = handle_key(app, key)
        {
            attach_or_fork(terminal, app, fork)?;
        }
    }
    Ok(())
}

/// Runs `claude` in place of the current process (attach when `fork` is false,
/// fork the resumed session otherwise). Only returns to the TUI if exec itself
/// failed to start `claude`, or if the session can't be opened this way at all.
fn attach_or_fork(terminal: &mut DefaultTerminal, app: &mut App, fork: bool) -> io::Result<()> {
    let Some(session) = app.selected_session() else {
        app.status_message = Some("no session selected".to_string());
        return Ok(());
    };
    let command = if fork {
        Some(app.fork_command(session))
    } else {
        app.attach_command(session)
    };
    let Some(command) = command else {
        app.status_message = Some(
            "cannot attach: session is interactive elsewhere (needs a pane manager like herdr, not yet supported)"
                .to_string(),
        );
        return Ok(());
    };

    ratatui::restore();
    let err = action::exec_replace(command);

    *terminal = ratatui::try_init()?;
    app.status_message = Some(format!("failed to start claude: {err}"));
    Ok(())
}

/// Returns `Some(fork)` when the key requests attaching to (or forking) the
/// selected session; the caller then owns the terminal handoff since exec
/// needs it restored first.
fn handle_key(app: &mut App, key: KeyEvent) -> Option<bool> {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.should_quit = true;
        return None;
    }

    match app.mode {
        Mode::Normal => return handle_normal_key(app, key),
        Mode::ConfirmKill => {
            if key.code == KeyCode::Char('y') {
                app.confirm_kill();
            } else {
                app.cancel_kill();
            }
        }
    }
    None
}

fn handle_normal_key(app: &mut App, key: KeyEvent) -> Option<bool> {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Down | KeyCode::Char('j') => app.select_next(),
        KeyCode::Up | KeyCode::Char('k') => app.select_previous(),
        KeyCode::Char('r') => app.refresh(),
        KeyCode::Enter | KeyCode::Char('o') => return Some(false),
        KeyCode::Char('f') => return Some(true),
        KeyCode::Char('x') => app.request_kill(),
        _ => {}
    }
    None
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

    #[test]
    fn enter_and_o_request_attach() {
        let mut app = App::new();
        assert_eq!(handle_key(&mut app, key(KeyCode::Enter)), Some(false));
        assert_eq!(handle_key(&mut app, key(KeyCode::Char('o'))), Some(false));
    }

    #[test]
    fn f_requests_fork() {
        let mut app = App::new();
        assert_eq!(handle_key(&mut app, key(KeyCode::Char('f'))), Some(true));
    }

    #[test]
    fn navigation_keys_do_not_request_attach() {
        let mut app = App::new();
        assert_eq!(handle_key(&mut app, key(KeyCode::Char('j'))), None);
    }

    #[test]
    fn x_enters_confirm_kill_mode_when_pid_present() {
        let mut app = App::new();
        app.sessions = vec![session_with_pid("a", Some(123))];
        handle_key(&mut app, key(KeyCode::Char('j')));

        handle_key(&mut app, key(KeyCode::Char('x')));

        assert_eq!(app.mode, Mode::ConfirmKill);
    }

    #[test]
    fn x_is_refused_without_pid() {
        let mut app = App::new();
        app.sessions = vec![session("a")];
        handle_key(&mut app, key(KeyCode::Char('j')));

        handle_key(&mut app, key(KeyCode::Char('x')));

        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn non_y_key_cancels_confirm_kill_mode() {
        let mut app = App::new();
        app.mode = Mode::ConfirmKill;

        handle_key(&mut app, key(KeyCode::Esc));

        assert_eq!(app.mode, Mode::Normal);
    }
}
