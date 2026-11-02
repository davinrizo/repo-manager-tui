use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::widgets::{Block, Cell, Row, Table};

use super::theme;
use crate::app::{App, Filter, Summary, Task};
use crate::format::{bytes, percent};
use crate::project::{ArtifactKind, Project, ProjectKind};

const SIZE_WIDTH: u16 = 8;
const COLUMNS: [(&str, Constraint); 8] = [
    ("Name", Constraint::Fill(1)),
    ("Status", Constraint::Length(11)),
    ("Size", Constraint::Length(SIZE_WIDTH)),
    ("% Disk", Constraint::Length(6)),
    ("Junk", Constraint::Length(SIZE_WIDTH)),
    ("Deps", Constraint::Length(SIZE_WIDTH)),
    ("Cache", Constraint::Length(SIZE_WIDTH)),
    ("Claude", Constraint::Length(SIZE_WIDTH)),
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

    let mut parts = vec![format!("Projects · sorted by {}", app.sort_label())];
    if app.filter != Filter::All {
        parts.push(format!("showing {}", app.filter.label()));
    }
    if !app.query.is_empty() {
        parts.push(format!("matching \"{}\"", app.query));
    }
    if summary.marked_count > 0 {
        parts.push(format!(
            "{} marked ({})",
            summary.marked_count,
            bytes(summary.marked_size)
        ));
    }
    let title = format!(" {} ", parts.join(" · "));

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
    let status = match (task, project.kind, project.safety()) {
        (Some(task), _, _) => Cell::from(task.label()).fg(theme::ACCENT),
        (None, ProjectKind::Repo, Some(safety)) => {
            Cell::from(safety.label()).fg(theme::safety_color(safety))
        }
        (None, ProjectKind::Repo, None) => pending(),
        (None, kind, _) => Cell::from(kind.label()).fg(theme::MUTED),
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
    let junk = project.junk();
    let (size, share) = if project.is_repo() {
        (
            Cell::from(bytes(usage.total)),
            Cell::from(format!("{:.1}", percent(usage.total, disk_total))).fg(theme::MUTED),
        )
    } else {
        (Cell::from("-").fg(theme::MUTED), Cell::from(""))
    };
    Row::new([
        name,
        status,
        size,
        share,
        size_cell(junk).fg(if junk > 0 { theme::WARN } else { theme::MUTED }),
        size_cell(usage.junk_of(ArtifactKind::Deps)),
        size_cell(usage.junk_of(ArtifactKind::Cache)),
        size_cell(project.claude_size()),
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
