use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Gauge, Paragraph};

use super::theme;
use crate::app::{App, Summary};
use crate::format::{bytes, percent};

pub fn render(frame: &mut Frame, app: &App, summary: &Summary, area: Rect) {
    let block = Block::bordered()
        .title(" repo-manager ".bold())
        .title_bottom(Line::from(format!(" {} ", roots_label(app))).fg(theme::MUTED));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [stats, disk, gauge] = Layout::vertical([Constraint::Length(1); 3]).areas(inner);
    frame.render_widget(stats_line(app, summary), stats);

    let Some(space) = app.disk else {
        frame.render_widget(Paragraph::new(" Disk: unavailable").fg(theme::MUTED), disk);
        return;
    };
    let used = percent(space.used(), space.total);
    frame.render_widget(
        Line::from(vec![
            Span::raw(" Disk  "),
            Span::raw(format!("{} free", bytes(space.available)))
                .bold()
                .fg(theme::GOOD),
            Span::raw(format!(
                " of {}  ·  {} used  ·  repos take {:.1}% of the disk",
                bytes(space.total),
                bytes(space.used()),
                percent(summary.total_size, space.total)
            )),
        ]),
        disk,
    );
    frame.render_widget(
        Gauge::default()
            .gauge_style(Style::new().fg(theme::usage_color(used)).bg(theme::MUTED))
            .ratio((used / 100.0).clamp(0.0, 1.0))
            .label(format!("{used:.0}% used")),
        gauge,
    );
}

fn stats_line(app: &App, summary: &Summary) -> Line<'static> {
    let progress = if app.discovering {
        "discovering projects…".to_owned()
    } else if !summary.is_complete() {
        format!("scanning {}/{}…", summary.measured, summary.projects)
    } else {
        String::new()
    };

    Line::from(vec![
        Span::raw(" Projects "),
        Span::raw(summary.projects.to_string()).bold(),
        Span::raw("   Size "),
        Span::raw(bytes(summary.total_size)).bold(),
        Span::raw("   Junk "),
        Span::raw(bytes(summary.junk)).bold().fg(theme::WARN),
        Span::raw("   Safe to delete "),
        Span::raw(format!(
            "{} ({})",
            bytes(summary.safe_size),
            summary.safe_count
        ))
        .bold()
        .fg(theme::GOOD),
        Span::raw(if app.freed > 0 { "   Freed " } else { "" }),
        Span::raw(if app.freed > 0 {
            bytes(app.freed)
        } else {
            String::new()
        })
        .bold()
        .fg(theme::ACCENT),
        Span::raw("   "),
        Span::raw(progress).fg(theme::MUTED),
    ])
}

fn roots_label(app: &App) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    app.roots
        .iter()
        .map(|root| {
            let path = root.display().to_string();
            match path.strip_prefix(&home) {
                Some(rest) if !home.is_empty() => format!("~{rest}"),
                _ => path,
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}
