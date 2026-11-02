mod app;
mod clean;
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
    use ratatui::crossterm::event::{KeyCode, KeyEvent};
    use ratatui::{Terminal, backend::TestBackend};
    use std::fs;

    fn wait_until(app: &mut App, done: impl Fn(&App) -> bool) {
        for _ in 0..600 {
            app.process_messages();
            if done(app) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("timed out waiting for background work");
    }

    fn scan_finished(app: &App) -> bool {
        !app.discovering && app.summary().is_complete()
    }

    fn render(app: &mut App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
        terminal.draw(|f| ui::draw(f, app)).unwrap();
        let buf = terminal.backend().buffer();
        (0..buf.area.height)
            .map(|y| {
                let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
                line.trim_end().to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn press(app: &mut App, c: char) {
        app.on_key(KeyEvent::from(KeyCode::Char(c)));
    }

    #[test]
    fn cleans_dependencies_after_confirmation() {
        let root = std::env::temp_dir().join(format!("repo-manager-e2e-{}", std::process::id()));
        let project = root.join("web");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(project.join("node_modules/left-pad")).unwrap();
        fs::write(project.join("package.json"), "{}").unwrap();
        fs::write(
            project.join("node_modules/left-pad/index.js"),
            "x".repeat(10_000),
        )
        .unwrap();

        let mut app = App::new(vec![root.canonicalize().unwrap()], 4);
        wait_until(&mut app, scan_finished);

        press(&mut app, 'n');
        assert!(matches!(app.mode, app::Mode::ConfirmClean(_)));
        let screen = render(&mut app);
        assert!(screen.contains("Clean dependencies"), "{screen}");
        assert!(project.join("node_modules").exists());

        press(&mut app, 'y');
        wait_until(&mut app, |a| a.freed > 0);

        assert!(!project.join("node_modules").exists());
        assert!(project.join("package.json").exists());
        assert_eq!(app.selected().unwrap().junk(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancelling_keeps_files() {
        let root = std::env::temp_dir().join(format!("repo-manager-cancel-{}", std::process::id()));
        let project = root.join("web");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(project.join("node_modules/a")).unwrap();
        fs::write(project.join("package.json"), "{}").unwrap();

        let mut app = App::new(vec![root.canonicalize().unwrap()], 4);
        wait_until(&mut app, scan_finished);
        press(&mut app, 'x');
        press(&mut app, 'n');

        assert!(matches!(app.mode, app::Mode::Browse));
        assert!(project.join("node_modules").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "scans a real folder; run with SCAN_DIR=<path> cargo test render_snapshot -- --ignored --nocapture"]
    fn render_snapshot() {
        let dir = std::env::var("SCAN_DIR").unwrap_or_else(|_| ".".into());
        let mut app = App::new(vec![PathBuf::from(dir).canonicalize().unwrap()], 4);
        wait_until(&mut app, scan_finished);
        if let Ok(key) = std::env::var("PRESS") {
            key.chars().for_each(|c| press(&mut app, c));
        }
        println!("{}", render(&mut app));
    }
}
