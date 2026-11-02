use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::widgets::{Block, Cell, Row, Table};

use super::theme;
use crate::app::{App, Summary, Task};
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

pub fn render(frame: &mut Frame, app: &mut App, summary: &Summary, area: Rect) {
    let disk_total = app.disk.map_or(0, |d| d.total);
    let rows = app.view.iter().map(|&i| {
        let project = &app.projects[i];
        project_row(
            project,
            disk_total,
            app.is_marked(&project.path),
            app.task(&project.path),
        )
    });

    let marked = if summary.marked_count > 0 {
        format!(
            "· {} marked ({}) ",
            summary.marked_count,
            bytes(summary.marked_size)
        )
    } else {
        String::new()
    };
    let title = format!(" Projects · sorted by {} {marked}", app.sort.label());

    let table = Table::new(rows, COLUMNS.map(|(_, width)| width))
        .header(Row::new(COLUMNS.map(|(title, _)| title)).style(Style::new().bold()))
        .block(Block::bordered().title(title))
        .row_highlight_style(theme::selected_row())
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(table, area, &mut app.table);
}

fn project_row(
    project: &Project,
    disk_total: u64,
    marked: bool,
    task: Option<Task>,
) -> Row<'static> {
    let status = match (task, project.safety()) {
        (Some(task), _) => Cell::from(task.label()).fg(theme::ACCENT),
        (None, Some(safety)) => Cell::from(safety.label()).fg(theme::safety_color(safety)),
        (None, None) => pending(),
    };
    let name = if marked {
        Cell::from(format!("● {}", project.name))
            .fg(theme::ACCENT)
            .bold()
    } else {
        Cell::from(format!("  {}", project.name))
    };

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
