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

    struct Fixture {
        root: PathBuf,
        remotes: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let base =
                std::env::temp_dir().join(format!("repo-manager-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&base);
            let fixture = Self {
                root: base.join("root"),
                remotes: base.join("remotes"),
            };
            fs::create_dir_all(&fixture.root).unwrap();
            fs::create_dir_all(&fixture.remotes).unwrap();
            fixture
        }

        fn plain_project(&self, name: &str) -> PathBuf {
            let path = self.root.join(name);
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("package.json"), "{}").unwrap();
            path
        }

        fn pushed_repo(&self, name: &str) -> PathBuf {
            let path = self.plain_project(name);
            let remote = self.remotes.join(format!("{name}.git"));
            git(
                &self.remotes,
                &["init", "-q", "--bare", remote.to_str().unwrap()],
            );
            git(&path, &["init", "-q", "-b", "main"]);
            git(&path, &["add", "."]);
            git(
                &path,
                &[
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@t",
                    "commit",
                    "-qm",
                    "init",
                ],
            );
            git(
                &path,
                &["remote", "add", "origin", remote.to_str().unwrap()],
            );
            git(&path, &["push", "-q", "-u", "origin", "main"]);
            path
        }

        fn app(&self) -> App {
            let mut app = App::new(vec![self.root.canonicalize().unwrap()], 4);
            wait_until(&mut app, scan_finished);
            app
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(self.root.parent().unwrap());
        }
    }

    fn git(dir: &std::path::Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    fn select(app: &mut App, name: &str) {
        let index = app
            .view
            .iter()
            .position(|&i| app.projects[i].name == name)
            .unwrap();
        app.table.select(Some(index));
    }

    fn type_text(app: &mut App, text: &str) {
        text.chars().for_each(|c| press(app, c));
    }

    fn delete_request(app: &App) -> &app::DeleteRequest {
        match &app.mode {
            app::Mode::ConfirmDelete { request, .. } => request,
            _ => panic!("delete confirmation is not open"),
        }
    }

    #[test]
    fn deletes_safe_repo_with_single_key() {
        let fixture = Fixture::new("delete-safe");
        let repo = fixture.pushed_repo("api");
        let mut app = fixture.app();

        select(&mut app, "api");
        press(&mut app, 'd');
        assert!(!delete_request(&app).requires_typing());

        press(&mut app, 'y');
        wait_until(&mut app, |a| a.projects.is_empty());
        assert!(!repo.exists());
    }

    #[test]
    fn unsafe_repo_requires_typing_confirmation() {
        let fixture = Fixture::new("delete-unsafe");
        let project = fixture.plain_project("scratch");
        let mut app = fixture.app();

        select(&mut app, "scratch");
        press(&mut app, 'd');
        assert!(delete_request(&app).requires_typing());
        assert!(render(&mut app).contains("NOT fully backed up"));

        press(&mut app, 'y');
        app.on_key(KeyEvent::from(KeyCode::Enter));
        assert!(matches!(app.mode, app::Mode::ConfirmDelete { .. }));
        assert!(project.exists());

        app.on_key(KeyEvent::from(KeyCode::Backspace));
        type_text(&mut app, "delete");
        app.on_key(KeyEvent::from(KeyCode::Enter));
        wait_until(&mut app, |a| a.projects.is_empty());
        assert!(!project.exists());
    }

    #[test]
    fn skips_repo_that_changed_after_confirmation_opened() {
        let fixture = Fixture::new("delete-changed");
        let repo = fixture.pushed_repo("web");
        let mut app = fixture.app();

        select(&mut app, "web");
        press(&mut app, 'd');
        fs::write(repo.join("notes.txt"), "unsaved work").unwrap();
        press(&mut app, 'y');

        wait_until(&mut app, |a| a.task(&a.projects[0].path).is_none());
        assert!(repo.join("notes.txt").exists());
        assert!(app.notice.as_ref().unwrap().text.contains("no longer safe"));
    }

    #[test]
    fn deletes_all_marked_projects() {
        let fixture = Fixture::new("delete-marked");
        fixture.pushed_repo("one");
        fixture.pushed_repo("two");
        let keep = fixture.pushed_repo("three");
        let mut app = fixture.app();

        select(&mut app, "one");
        press(&mut app, ' ');
        select(&mut app, "two");
        press(&mut app, ' ');
        assert_eq!(app.summary().marked_count, 2);

        press(&mut app, 'd');
        assert_eq!(delete_request(&app).targets.len(), 2);
        press(&mut app, 'y');
        wait_until(&mut app, |a| a.projects.len() == 1);

        assert_eq!(app.projects[0].name, "three");
        assert!(keep.exists());
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
