use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};

use crate::app::{App, Mode};
use crate::model::{Session, SessionKind, SessionState, SessionStatus};

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(frame.area());

    let rows = app.sessions.iter().map(|session| {
        let (state_label, state_style) = state_display(session);

        let row = Row::new(vec![
            Cell::from(state_label).style(state_style),
            Cell::from(kind_label(session.kind)),
            Cell::from(format_elapsed(session.started_at)),
            Cell::from(session.cwd.display().to_string()),
            Cell::from(session.name.clone()),
        ]);

        // No pid means kill has nothing to signal; dim the row so that's visible
        // at a glance rather than only surfacing as a message after pressing 'x'.
        if session.pid.is_none() {
            row.style(Style::default().add_modifier(Modifier::DIM))
        } else {
            row
        }
    });

    let widths = [
        Constraint::Length(10),
        Constraint::Length(11),
        Constraint::Length(8),
        Constraint::Percentage(35),
        Constraint::Percentage(45),
    ];

    let header = Row::new(vec!["state", "kind", "elapsed", "cwd", "name"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let table = Table::new(rows, widths)
        .header(header)
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .block(Block::default().borders(Borders::ALL).title("strays"));

    let mut table_state = TableState::default();
    table_state.select(app.selected());

    frame.render_stateful_widget(table, chunks[0], &mut table_state);

    let status = match app.mode {
        Mode::ConfirmKill => {
            let target = app
                .selected_session()
                .map(|s| s.name.as_str())
                .unwrap_or("session");
            format!("kill \"{target}\"? (y/n)")
        }
        Mode::NewSession => new_session_status(&app.input, app.status_message.as_deref()),
        Mode::Normal => app.status_message.clone().unwrap_or_else(|| {
            "q: quit  j/k: move  r: refresh  enter/o: attach  f: fork  x: kill  n: new".to_string()
        }),
    };
    frame.render_widget(Paragraph::new(status), chunks[1]);

    if app.mode == Mode::NewSession {
        let column = new_session_cursor_column(chunks[1].x, &app.input, app.input_cursor);
        frame.set_cursor_position((column, chunks[1].y));
    }
}

const NEW_SESSION_PREFIX: &str = "open new session in: ";

fn new_session_status(input: &str, status_message: Option<&str>) -> String {
    let mut status = format!("{NEW_SESSION_PREFIX}{input}");
    if let Some(message) = status_message {
        status.push_str("  (");
        status.push_str(message);
        status.push(')');
    }
    status
}

/// Column, not byte/char offset, so wide (CJK/emoji) characters before the
/// cursor don't leave it misaligned with the actual edit position.
fn new_session_cursor_column(base_x: u16, input: &str, cursor: usize) -> u16 {
    base_x
        + Span::raw(NEW_SESSION_PREFIX).width() as u16
        + Span::raw(&input[..cursor]).width() as u16
}

/// Interactive sessions carry `status` (busy/idle) instead of `state`, so this
/// falls back to it rather than showing a bare "-" for every interactive row.
fn state_display(session: &Session) -> (String, Style) {
    match (&session.state, &session.status) {
        (Some(SessionState::Blocked), _) => {
            ("blocked".to_string(), Style::default().fg(Color::Yellow))
        }
        (Some(SessionState::Other(other)), _) => (other.clone(), Style::default()),
        (None, Some(SessionStatus::Busy)) => ("busy".to_string(), Style::default().fg(Color::Cyan)),
        (None, Some(SessionStatus::Idle)) => ("idle".to_string(), Style::default()),
        (None, Some(SessionStatus::Other(other))) => (other.clone(), Style::default()),
        (None, None) => ("-".to_string(), Style::default()),
    }
}

fn kind_label(kind: SessionKind) -> &'static str {
    match kind {
        SessionKind::Background => "background",
        SessionKind::Interactive => "interactive",
        SessionKind::Unknown => "unknown",
    }
}

fn format_elapsed(started_at_ms: u64) -> String {
    let started = UNIX_EPOCH + Duration::from_millis(started_at_ms);
    let elapsed = SystemTime::now()
        .duration_since(started)
        .unwrap_or_default();

    let secs = elapsed.as_secs();
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3600;
    let minutes = (secs % 3600) / 60;

    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn session(state: Option<SessionState>, status: Option<SessionStatus>) -> Session {
        Session {
            id: "id".to_string(),
            session_id: "session-id".to_string(),
            cwd: PathBuf::from("/tmp"),
            kind: SessionKind::Background,
            started_at: 0,
            name: "name".to_string(),
            state,
            pid: None,
            status,
        }
    }

    #[test]
    fn state_display_prefers_state_over_status() {
        let (label, _) = state_display(&session(
            Some(SessionState::Blocked),
            Some(SessionStatus::Busy),
        ));
        assert_eq!(label, "blocked");
    }

    #[test]
    fn state_display_falls_back_to_status_when_state_is_absent() {
        let (label, _) = state_display(&session(None, Some(SessionStatus::Busy)));
        assert_eq!(label, "busy");

        let (label, _) = state_display(&session(None, Some(SessionStatus::Idle)));
        assert_eq!(label, "idle");
    }

    #[test]
    fn state_display_shows_dash_when_both_are_absent() {
        let (label, _) = state_display(&session(None, None));
        assert_eq!(label, "-");
    }

    #[test]
    fn kind_label_covers_every_variant() {
        assert_eq!(kind_label(SessionKind::Background), "background");
        assert_eq!(kind_label(SessionKind::Interactive), "interactive");
        assert_eq!(kind_label(SessionKind::Unknown), "unknown");
    }

    #[test]
    fn new_session_status_shows_prefix_and_input() {
        assert_eq!(
            new_session_status("/tmp", None),
            "open new session in: /tmp"
        );
    }

    #[test]
    fn new_session_status_appends_the_error_message() {
        assert_eq!(
            new_session_status("/bad", Some("not a directory: /bad")),
            "open new session in: /bad  (not a directory: /bad)"
        );
    }

    #[test]
    fn new_session_cursor_column_starts_right_after_the_prefix() {
        let prefix_width = Span::raw(NEW_SESSION_PREFIX).width() as u16;
        assert_eq!(new_session_cursor_column(0, "abc", 0), prefix_width);
    }

    #[test]
    fn new_session_cursor_column_counts_display_width_not_bytes() {
        let prefix_width = Span::raw(NEW_SESSION_PREFIX).width() as u16;
        // "日" is a 3-byte, double-width character; the cursor after it should
        // advance by its display width (2), not its byte length (3).
        let cursor_after_char = "日".len();
        assert_eq!(
            new_session_cursor_column(0, "日", cursor_after_char),
            prefix_width + 2
        );
    }

    #[test]
    fn formats_days_and_hours_for_old_sessions() {
        let eight_days_ago = SystemTime::now() - Duration::from_secs(8 * 86_400 + 3 * 3600);
        let started_at_ms = eight_days_ago
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        assert_eq!(format_elapsed(started_at_ms), "8d 3h");
    }

    #[test]
    fn formats_minutes_for_recent_sessions() {
        let five_minutes_ago = SystemTime::now() - Duration::from_secs(5 * 60);
        let started_at_ms = five_minutes_ago
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        assert_eq!(format_elapsed(started_at_ms), "5m");
    }
}
