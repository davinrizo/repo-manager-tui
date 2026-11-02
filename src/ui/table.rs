use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::widgets::{Block, Cell, Row, Table};

use super::theme;
use crate::app::App;
use crate::format::{bytes, percent};
use crate::project::{ArtifactKind, Project};

const COLUMNS: [(&str, Constraint); 7] = [
    ("Name", Constraint::Fill(1)),
    ("Status", Constraint::Length(11)),
    ("Size", Constraint::Length(9)),
    ("% Disk", Constraint::Length(6)),
    ("Junk", Constraint::Length(9)),
    ("Deps", Constraint::Length(9)),
    ("Cache", Constraint::Length(9)),
];

pub fn render(frame: &mut Frame, app: &mut App, area: Rect) {
    let disk_total = app.disk.map_or(0, |d| d.total);
    let rows = app.view.iter().map(|&i| {
        project_row(
            &app.projects[i],
            disk_total,
            app.is_busy(&app.projects[i].path),
        )
    });

    let table = Table::new(rows, COLUMNS.map(|(_, width)| width))
        .header(Row::new(COLUMNS.map(|(title, _)| title)).style(Style::new().bold()))
        .block(Block::bordered().title(format!(" Projects · sorted by {} ", app.sort.label())))
        .row_highlight_style(theme::selected_row())
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(table, area, &mut app.table);
}

fn project_row(project: &Project, disk_total: u64, busy: bool) -> Row<'static> {
    let status = match project.safety() {
        _ if busy => Cell::from("CLEANING…").fg(theme::ACCENT),
        Some(safety) => Cell::from(safety.label()).fg(theme::safety_color(safety)),
        None => pending(),
    };
    let name = Cell::from(project.name.clone());

    let Some(usage) = &project.usage else {
        return Row::new([name, status, pending()]);
    };
    let junk = usage.junk();
    Row::new([
        name,
        status,
        Cell::from(bytes(usage.total)),
        Cell::from(format!("{:.1}", percent(usage.total, disk_total))).fg(theme::MUTED),
        size_cell(junk).fg(if junk > 0 { theme::WARN } else { theme::MUTED }),
        size_cell(usage.junk_of(ArtifactKind::Deps)),
        size_cell(usage.junk_of(ArtifactKind::Cache)),
    ])
}

fn size_cell(size: u64) -> Cell<'static> {
    if size == 0 {
        Cell::from("-").fg(theme::MUTED)
    } else {
        Cell::from(bytes(size))
    }
}

fn pending() -> Cell<'static> {
    Cell::from("…").fg(theme::MUTED)
}
