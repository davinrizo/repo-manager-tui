mod confirm;
mod details;
mod header;
mod table;
mod theme;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};

use crate::app::{App, Mode};

const WIDE_LAYOUT_MIN_WIDTH: u16 = 120;
const KEYS: &[(&str, &str)] = &[
    ("↑↓/jk", "move"),
    ("n", "clean deps"),
    ("c", "clean cache"),
    ("x", "clean all"),
    ("s", "sort"),
    ("r", "rescan"),
    ("q", "quit"),
];

pub fn draw(frame: &mut Frame, app: &mut App) {
    let summary = app.summary();
    let [header_area, body, footer] = Layout::vertical([
        Constraint::Length(5),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    let direction = if body.width >= WIDE_LAYOUT_MIN_WIDTH {
        Direction::Horizontal
    } else {
        Direction::Vertical
    };
    let [table_area, details_area] = Layout::default()
        .direction(direction)
        .constraints([Constraint::Percentage(64), Constraint::Percentage(36)])
        .areas(body);

    header::render(frame, app, &summary, header_area);
    table::render(frame, app, table_area);
    details::render(frame, app, details_area);
    render_footer(frame, app, footer);

    if let Mode::ConfirmClean(request) = &app.mode {
        confirm::render(frame, request);
    }
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    if let Some(notice) = &app.notice {
        let line = Line::from(format!(" {}", notice.text)).fg(theme::notice_color(notice.level));
        frame.render_widget(line, area);
        return;
    }
    let spans = KEYS.iter().flat_map(|(key, action)| {
        [
            Span::raw(format!(" {key}")).bold().fg(theme::ACCENT),
            Span::raw(format!(" {action} ")),
        ]
    });
    frame.render_widget(Line::from_iter(spans), area);
}
