use ratatui::style::{Color, Modifier, Style};

use crate::app::NoticeLevel;
use crate::git::Safety;

pub const ACCENT: Color = Color::Cyan;
pub const GOOD: Color = Color::Green;
pub const WARN: Color = Color::Yellow;
pub const BAD: Color = Color::Red;
pub const MUTED: Color = Color::DarkGray;

pub fn selected_row() -> Style {
    Style::new()
        .bg(Color::Blue)
        .fg(Color::White)
        .add_modifier(Modifier::BOLD)
}

pub fn safety_color(safety: Safety) -> Color {
    match safety {
        Safety::Safe => GOOD,
        Safety::Stashed(_) | Safety::Unpushed(_) | Safety::Uncommitted(_) => WARN,
        Safety::NoRemote | Safety::NoGit => BAD,
        Safety::Broken => Color::Magenta,
    }
}

pub fn usage_color(used_percent: f64) -> Color {
    if used_percent > 90.0 {
        BAD
    } else if used_percent > 75.0 {
        WARN
    } else {
        GOOD
    }
}

pub fn notice_color(level: NoticeLevel) -> Color {
    match level {
        NoticeLevel::Info => ACCENT,
        NoticeLevel::Success => GOOD,
        NoticeLevel::Error => BAD,
    }
}
