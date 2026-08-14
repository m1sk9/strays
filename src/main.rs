mod action;
mod app;
mod model;
mod provider;
mod ui;

use std::io;
use std::path::PathBuf;
use std::process::Command;
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
        {
            match handle_key(app, key) {
                KeyOutcome::None => {}
                KeyOutcome::Attach => attach_or_fork(terminal, app, false)?,
                KeyOutcome::Fork => attach_or_fork(terminal, app, true)?,
                KeyOutcome::OpenDirectory(path) => open_new_session(terminal, app, path)?,
            }
        }
    }
    Ok(())
}

/// Hands the terminal to `command` via exec, restoring the TUI only if exec
/// itself failed to start `claude` at all.
fn exec_and_recover(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    command: Command,
) -> io::Result<()> {
    ratatui::restore();
    let err = action::exec_replace(command);

    *terminal = ratatui::try_init()?;
    app.status_message = Some(format!("failed to start claude: {err}"));
    Ok(())
}

#[derive(Debug)]
enum AttachOutcome {
    NoSelection,
    Unavailable(&'static str),
    Ready(std::process::Command),
}

/// Decides what attaching/forking the selected session should do, without
/// touching the terminal — kept separate from `attach_or_fork` so this can be
/// tested without a real `DefaultTerminal` (`ratatui::init()` needs a real TTY).
fn resolve_attach_or_fork(app: &App, fork: bool) -> AttachOutcome {
    let Some(session) = app.selected_session() else {
        return AttachOutcome::NoSelection;
    };

    let command = if fork {
        app.fork_command(session)
    } else {
        app.attach_command(session)
    };

    match command {
        Some(command) => AttachOutcome::Ready(command),
        None if fork => AttachOutcome::Unavailable("cannot fork: unrecognized session kind"),
        None => AttachOutcome::Unavailable(
            "cannot attach: session is interactive elsewhere (needs a pane manager like herdr, not yet supported)",
        ),
    }
}

/// Runs `claude` in place of the current process (attach when `fork` is false,
/// fork the resumed session otherwise). Only returns to the TUI if exec itself
/// failed to start `claude`, or if the session can't be opened this way at all.
fn attach_or_fork(terminal: &mut DefaultTerminal, app: &mut App, fork: bool) -> io::Result<()> {
    let command = match resolve_attach_or_fork(app, fork) {
        AttachOutcome::NoSelection => {
            app.status_message = Some("no session selected".to_string());
            return Ok(());
        }
        AttachOutcome::Unavailable(message) => {
            app.status_message = Some(message.to_string());
            return Ok(());
        }
        AttachOutcome::Ready(command) => command,
    };

    exec_and_recover(terminal, app, command)
}

/// Starts a brand-new `claude` session in `path` — no `--resume`, since this
/// isn't tied to any existing session.
fn open_new_session(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    path: PathBuf,
) -> io::Result<()> {
    let mut command = Command::new("claude");
    command.current_dir(path);

    exec_and_recover(terminal, app, command)
}

#[derive(Debug, PartialEq)]
enum KeyOutcome {
    None,
    Attach,
    Fork,
    OpenDirectory(PathBuf),
}

/// Decides what a key press should do; the caller owns any terminal handoff
/// (attach/fork/open all need it restored before exec).
fn handle_key(app: &mut App, key: KeyEvent) -> KeyOutcome {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.should_quit = true;
        return KeyOutcome::None;
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
        Mode::NewSession => return handle_new_session_key(app, key),
    }
    KeyOutcome::None
}

fn handle_normal_key(app: &mut App, key: KeyEvent) -> KeyOutcome {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Down | KeyCode::Char('j') => app.select_next(),
        KeyCode::Up | KeyCode::Char('k') => app.select_previous(),
        KeyCode::Char('r') => app.refresh(),
        KeyCode::Enter | KeyCode::Char('o') => return KeyOutcome::Attach,
        KeyCode::Char('f') => return KeyOutcome::Fork,
        KeyCode::Char('x') => app.request_kill(),
        KeyCode::Char('n') => app.start_new_session_input(),
        _ => {}
    }
    KeyOutcome::None
}

fn handle_new_session_key(app: &mut App, key: KeyEvent) -> KeyOutcome {
    match key.code {
        KeyCode::Esc => app.cancel_new_session_input(),
        KeyCode::Enter => {
            if let Some(path) = app.confirm_new_session_input() {
                return KeyOutcome::OpenDirectory(path);
            }
        }
        KeyCode::Backspace => app.input_backspace(),
        KeyCode::Left => app.input_move_left(),
        KeyCode::Right => app.input_move_right(),
        KeyCode::Char(c) => app.input_insert(c),
        _ => {}
    }
    KeyOutcome::None
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
        assert_eq!(
            handle_key(&mut app, key(KeyCode::Enter)),
            KeyOutcome::Attach
        );
        assert_eq!(
            handle_key(&mut app, key(KeyCode::Char('o'))),
            KeyOutcome::Attach
        );
    }

    #[test]
    fn f_requests_fork() {
        let mut app = App::new();
        assert_eq!(
            handle_key(&mut app, key(KeyCode::Char('f'))),
            KeyOutcome::Fork
        );
    }

    #[test]
    fn navigation_keys_do_not_request_attach() {
        let mut app = App::new();
        assert_eq!(
            handle_key(&mut app, key(KeyCode::Char('j'))),
            KeyOutcome::None
        );
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

    fn session_with_kind(id: &str, kind: SessionKind) -> Session {
        Session {
            id: id.to_string(),
            session_id: id.to_string(),
            cwd: PathBuf::from("/tmp"),
            kind,
            started_at: 0,
            name: "name".to_string(),
            state: None,
            pid: None,
            status: None,
        }
    }

    fn args_of(command: &std::process::Command) -> Vec<&str> {
        command.get_args().map(|a| a.to_str().unwrap()).collect()
    }

    #[test]
    fn resolve_attach_or_fork_has_no_selection_without_sessions() {
        let app = App::new();
        assert!(matches!(
            resolve_attach_or_fork(&app, false),
            AttachOutcome::NoSelection
        ));
    }

    #[test]
    fn resolve_attach_or_fork_attaches_background_sessions() {
        let mut app = App::new();
        app.sessions = vec![session_with_kind("a", SessionKind::Background)];
        handle_key(&mut app, key(KeyCode::Char('j')));

        match resolve_attach_or_fork(&app, false) {
            AttachOutcome::Ready(command) => assert_eq!(args_of(&command), ["attach", "a"]),
            other => panic!("expected Ready, got a different outcome: {other:?}"),
        }
    }

    #[test]
    fn resolve_attach_or_fork_refuses_attach_for_interactive_sessions() {
        let mut app = App::new();
        app.sessions = vec![session_with_kind("a", SessionKind::Interactive)];
        handle_key(&mut app, key(KeyCode::Char('j')));

        assert!(matches!(
            resolve_attach_or_fork(&app, false),
            AttachOutcome::Unavailable(_)
        ));
    }

    #[test]
    fn resolve_attach_or_fork_refuses_fork_for_unknown_kind() {
        let mut app = App::new();
        app.sessions = vec![session_with_kind("a", SessionKind::Unknown)];
        handle_key(&mut app, key(KeyCode::Char('j')));

        assert!(matches!(
            resolve_attach_or_fork(&app, true),
            AttachOutcome::Unavailable(_)
        ));
    }

    #[test]
    fn resolve_attach_or_fork_forks_regardless_of_kind_when_supported() {
        let mut app = App::new();
        app.sessions = vec![session_with_kind("a", SessionKind::Background)];
        handle_key(&mut app, key(KeyCode::Char('j')));

        match resolve_attach_or_fork(&app, true) {
            AttachOutcome::Ready(command) => {
                assert_eq!(args_of(&command), ["--resume", "a", "--fork-session"]);
            }
            other => panic!("expected Ready, got a different outcome: {other:?}"),
        }
    }

    #[test]
    fn n_enters_new_session_input_mode_prefilled_with_selected_cwd() {
        let mut app = App::new();
        app.sessions = vec![session_with_kind("a", SessionKind::Background)];
        handle_key(&mut app, key(KeyCode::Char('j')));

        handle_key(&mut app, key(KeyCode::Char('n')));

        assert_eq!(app.mode, Mode::NewSession);
        assert_eq!(app.input, "/tmp");
    }

    #[test]
    fn typing_and_backspace_edit_the_input() {
        let mut app = App::new();
        app.mode = Mode::NewSession;

        handle_key(&mut app, key(KeyCode::Char('/')));
        handle_key(&mut app, key(KeyCode::Char('x')));
        assert_eq!(app.input, "/x");

        handle_key(&mut app, key(KeyCode::Backspace));
        assert_eq!(app.input, "/");
    }

    #[test]
    fn enter_opens_a_valid_directory_and_returns_to_normal_mode() {
        let mut app = App::new();
        app.mode = Mode::NewSession;
        app.input = "/tmp".to_string();
        app.input_cursor = app.input.len();

        let outcome = handle_key(&mut app, key(KeyCode::Enter));

        assert_eq!(outcome, KeyOutcome::OpenDirectory(PathBuf::from("/tmp")));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn enter_on_a_missing_directory_stays_in_input_mode() {
        let mut app = App::new();
        app.mode = Mode::NewSession;
        app.input = "/definitely/not/a/real/path".to_string();
        app.input_cursor = app.input.len();

        let outcome = handle_key(&mut app, key(KeyCode::Enter));

        assert_eq!(outcome, KeyOutcome::None);
        assert_eq!(app.mode, Mode::NewSession);
        assert!(app.status_message.is_some());
    }

    #[test]
    fn esc_cancels_new_session_input() {
        let mut app = App::new();
        app.mode = Mode::NewSession;
        app.input = "/tmp".to_string();

        handle_key(&mut app, key(KeyCode::Esc));

        assert_eq!(app.mode, Mode::Normal);
        assert!(app.input.is_empty());
    }
}
