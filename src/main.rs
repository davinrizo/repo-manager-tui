mod app;
mod discover;
mod disk;
mod format;
mod git;
mod measure;
mod project;
mod ui;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use ratatui::crossterm::event::{self, Event, KeyEventKind};

use app::App;

/// Find code repositories and reclaim the disk space they use.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Folders to scan [default: current folder]
    paths: Vec<PathBuf>,

    /// Maximum folder depth to search for projects
    #[arg(short, long, default_value_t = 4)]
    depth: usize,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let paths = if args.paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        args.paths
    };
    let roots = paths
        .iter()
        .map(|p| {
            p.canonicalize()
                .with_context(|| format!("can't open {}", p.display()))
        })
        .collect::<Result<Vec<_>>>()?;

    let mut app = App::new(roots, args.depth);
    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    let mut dirty = true;
    while !app.should_quit {
        dirty |= app.process_messages();
        if dirty {
            terminal.draw(|f| ui::draw(f, app))?;
            dirty = false;
        }
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    app.on_key(key);
                    dirty = true;
                }
                Event::Resize(..) => dirty = true,
                _ => {}
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    #[ignore = "scans a real folder; run with SCAN_DIR=<path> cargo test render_snapshot -- --ignored --nocapture"]
    fn render_snapshot() {
        let dir = std::env::var("SCAN_DIR").unwrap_or_else(|_| ".".into());
        let mut app = App::new(vec![PathBuf::from(dir).canonicalize().unwrap()], 4);
        for _ in 0..600 {
            app.process_messages();
            if !app.discovering && app.summary().is_complete() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
        terminal.draw(|f| ui::draw(f, &mut app)).unwrap();
        let buf = terminal.backend().buffer();
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            println!("{}", line.trim_end());
        }
    }
}
