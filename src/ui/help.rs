use ratatui::Frame;
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};

use super::popup::popup;
use super::theme;

const WIDTH: u16 = 56;
const SECTIONS: &[(&str, &[(&str, &str)])] = &[
    (
        "Navigate",
        &[
            ("↑ ↓  j k", "move"),
            ("PgUp PgDn", "move by page"),
            ("g G", "first / last"),
        ],
    ),
    (
        "Select",
        &[
            ("space", "mark / unmark"),
            ("a", "mark / unmark all visible"),
            ("Esc", "clear search, then marks"),
        ],
    ),
    (
        "Act on marked, or the current project",
        &[
            ("n", "clean dependencies"),
            ("c", "clean caches & build output"),
            ("x", "clean all junk"),
            ("d", "delete projects"),
            ("o", "open in file manager"),
        ],
    ),
    (
        "View",
        &[
            ("/", "search by name"),
            ("f", "cycle filter"),
            ("s", "cycle sort"),
            ("r", "rescan"),
            ("q", "quit"),
        ],
    ),
];

pub fn render(frame: &mut Frame) {
    let mut lines = Vec::new();
    for (title, keys) in SECTIONS {
        lines.push(Line::from(*title).bold());
        lines.extend(keys.iter().map(|(key, action)| {
            Line::from(vec![
                Span::raw(format!("  {key:<12}")).fg(theme::ACCENT),
                Span::raw(*action),
            ])
        }));
        lines.push(Line::default());
    }
    lines.push(Line::from("Press any key to close").fg(theme::MUTED));
    popup(frame, "Keys", theme::ACCENT, WIDTH, lines);
}
