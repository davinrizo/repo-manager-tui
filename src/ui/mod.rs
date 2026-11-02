mod confirm;
mod details;
mod header;
mod help;
mod popup;
mod table;
mod theme;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};

use crate::app::{App, Mode};

const WIDE_LAYOUT_MIN_WIDTH: u16 = 120;
const KEYS: &[(&str, &str)] = &[
    ("↑↓", "move"),
    ("space", "mark"),
    ("d", "delete"),
    ("n/c/x", "clean deps/cache/all"),
    ("/", "search"),
    ("f", "filter"),
    ("s", "sort"),
    ("?", "help"),
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
    table::render(frame, app, &summary, table_area);
    details::render(frame, app, details_area);
    render_footer(frame, app, footer);

    match app.mode {
        Mode::Help => help::render(frame),
        _ => confirm::render(frame, &app.mode),
    }
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    if matches!(app.mode, Mode::Search) {
        let line = Line::from(vec![
            Span::raw(" / ").bold().fg(theme::ACCENT),
            Span::raw(app.query.clone()),
            Span::raw("█").fg(theme::MUTED),
            Span::raw("   Enter keep · Esc clear").fg(theme::MUTED),
        ]);
        frame.render_widget(line, area);
        return;
    }
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
