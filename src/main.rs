mod app;
mod clean;
mod delete;
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
mod tests;
