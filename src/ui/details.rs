use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};

use super::theme;
use crate::app::App;
use crate::format::{ago, bytes};
use crate::git::{GitState, GitStatus, Safety};
use crate::project::{Project, Usage};

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::bordered().title(" Details ");
    let lines = match app.selected() {
        Some(project) => project_lines(project),
        None if app.projects.is_empty() => {
            vec![Line::from("No projects found").fg(theme::MUTED)]
        }
        None => vec![Line::from("No projects match the search or filter").fg(theme::MUTED)],
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn project_lines(project: &Project) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(project.name.clone()).bold(),
        Line::from(project.path.display().to_string()).fg(theme::MUTED),
        Line::default(),
    ];
    lines.extend(git_lines(&project.git));
    lines.push(Line::default());
    match &project.usage {
        Some(usage) => lines.extend(usage_lines(usage)),
        None => lines.push(Line::from("Measuring…").fg(theme::MUTED)),
    }
    lines
}

fn git_lines(state: &GitState) -> Vec<Line<'static>> {
    match state {
        GitState::Pending => vec![Line::from("Checking git…").fg(theme::MUTED)],
        GitState::Missing => vec![
            check(false, "No git repository".into()),
            verdict(Safety::NoGit, "nothing here is backed up"),
        ],
        GitState::Broken => vec![
            check(false, "Git repository could not be read".into()),
            verdict(Safety::Broken, "inspect it manually"),
        ],
        GitState::Repo(status) => repo_lines(status),
    }
}

fn repo_lines(status: &GitStatus) -> Vec<Line<'static>> {
    let remote = match &status.remote {
        Some(r) => format!("Remote {} → {}", r.name, r.url),
        None => "No remote configured".into(),
    };
    let plural = |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });

    let mut lines = vec![
        check(true, "Git repository".into()),
        check(status.remote.is_some(), remote),
        check(
            status.uncommitted == 0,
            match status.uncommitted {
                0 => "Working tree clean".into(),
                n => plural(n, "uncommitted change"),
            },
        ),
        check(
            status.unpushed == 0,
            match status.unpushed {
                0 => "All commits pushed".into(),
                n => format!("{} not on any remote", plural(n, "commit")),
            },
        ),
        check(
            status.stashes == 0,
            match status.stashes {
                0 => "No stashes".into(),
                n => plural(n, "stash"),
            },
        ),
        Line::from(vec![
            Span::raw("  Branch ").fg(theme::MUTED),
            Span::raw(status.branch.clone().unwrap_or_else(|| "detached".into())),
            Span::raw("  ·  Last commit ").fg(theme::MUTED),
            Span::raw(status.last_commit.map_or_else(|| "never".into(), ago)),
        ]),
    ];

    let safety = status.safety();
    let reason = if safety.is_safe() {
        "everything is on the remote"
    } else {
        "local work would be lost"
    };
    lines.push(Line::default());
    lines.push(verdict(safety, reason));
    lines
}

fn usage_lines(usage: &Usage) -> Vec<Line<'static>> {
    let row = |label: &'static str, value: u64, color: Option<Color>| {
        let value = Span::raw(bytes(value));
        Line::from(vec![
            Span::raw(label).fg(theme::MUTED),
            color.map_or(value.clone(), |c| value.fg(c)),
        ])
    };
    let mut lines = vec![
        row("Total   ", usage.total, None),
        row("Source  ", usage.source(), None),
        row("Junk    ", usage.junk(), Some(theme::WARN)),
        Line::default(),
    ];

    if usage.artifacts.is_empty() {
        lines.push(Line::from("No junk folders").fg(theme::MUTED));
        return lines;
    }
    lines.push(Line::from("Junk folders").bold());
    lines.extend(usage.artifacts.iter().map(|a| {
        Line::from(vec![
            Span::raw(format!("{:>9}  ", bytes(a.size))),
            Span::raw(format!("{:<6}", a.kind.label())).fg(theme::ACCENT),
            Span::raw(a.rel.clone()),
        ])
    }));
    lines
}

fn check(ok: bool, text: String) -> Line<'static> {
    let (mark, color) = if ok {
        ("✓ ", theme::GOOD)
    } else {
        ("✗ ", theme::BAD)
    };
    Line::from(vec![Span::raw(mark).fg(color), Span::raw(text)])
}

fn verdict(safety: Safety, reason: &str) -> Line<'static> {
    let headline = if safety.is_safe() {
        "Safe to delete"
    } else {
        "Not safe to delete"
    };
    Line::from(vec![
        Span::raw(headline).bold().fg(theme::safety_color(safety)),
        Span::raw(format!(" — {reason}")).fg(theme::MUTED),
    ])
}
