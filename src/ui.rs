use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};

use crate::app::App;
use crate::model::{SessionKind, SessionState, SessionStatus};

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(frame.area());

    let rows = app.sessions.iter().map(|session| {
        let state_cell = match (&session.state, &session.status) {
            (Some(SessionState::Blocked), _) => {
                Cell::from("blocked").style(Style::default().fg(Color::Yellow))
            }
            (Some(SessionState::Other(other)), _) => Cell::from(other.clone()),
            (None, Some(SessionStatus::Busy)) => {
                Cell::from("busy").style(Style::default().fg(Color::Cyan))
            }
            (None, Some(SessionStatus::Idle)) => Cell::from("idle"),
            (None, Some(SessionStatus::Other(other))) => Cell::from(other.clone()),
            (None, None) => Cell::from("-"),
        };
        let kind = match session.kind {
            SessionKind::Background => "background",
            SessionKind::Interactive => "interactive",
            SessionKind::Unknown => "unknown",
        };

        Row::new(vec![
            state_cell,
            Cell::from(kind),
            Cell::from(format_elapsed(session.started_at)),
            Cell::from(session.cwd.display().to_string()),
            Cell::from(session.name.clone()),
        ])
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

    let status = app
        .status_message
        .clone()
        .unwrap_or_else(|| "q: quit  j/k: move  r: refresh".to_string());
    frame.render_widget(Paragraph::new(status), chunks[1]);
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
    use super::*;

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
