use super::*;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::{Terminal, backend::TestBackend};
use std::fs;
use std::path::Path;
use std::time::SystemTime;

fn start(root: &Path, claude_dir: Option<PathBuf>) -> App {
    let mut app = App::new(Config {
        roots: vec![root.canonicalize().unwrap()],
        max_depth: 4,
        claude_dir,
    });
    wait_until(&mut app, scan_finished);
    app
}

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

    let mut app = start(&root, None);

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

    let mut app = start(&root, None);
    press(&mut app, 'x');
    press(&mut app, 'n');

    assert!(matches!(app.mode, app::Mode::Browse));
    assert!(project.join("node_modules").exists());
    fs::remove_dir_all(root).unwrap();
}

struct Fixture {
    root: PathBuf,
    remotes: PathBuf,
    claude: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let base = std::env::temp_dir().join(format!("repo-manager-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let fixture = Self {
            root: base.join("root"),
            remotes: base.join("remotes"),
            claude: base.join("claude-projects"),
        };
        fs::create_dir_all(&fixture.root).unwrap();
        fs::create_dir_all(&fixture.remotes).unwrap();
        fs::create_dir_all(&fixture.claude).unwrap();
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

    fn claude_history(&self, relative: &str) -> PathBuf {
        let origin = self.root.canonicalize().unwrap().join(relative);
        let dir = self.claude.join(claude::encode(&origin));
        fs::create_dir_all(dir.join("memory")).unwrap();
        fs::write(dir.join("memory/MEMORY.md"), "remember this").unwrap();
        let session = dir.join("old.jsonl");
        let line = serde_json::json!({ "cwd": origin }).to_string();
        fs::write(&session, format!("{line}\n{}", "x".repeat(20_000))).unwrap();
        let hour_ago = SystemTime::now() - Duration::from_secs(3600);
        fs::File::options()
            .write(true)
            .open(&session)
            .unwrap()
            .set_modified(hour_ago)
            .unwrap();
        dir
    }

    fn app(&self) -> App {
        start(&self.root, Some(self.claude.clone()))
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

fn visible_names(app: &App) -> Vec<&str> {
    let mut names: Vec<&str> = app
        .view
        .iter()
        .map(|&i| app.projects[i].name.as_str())
        .collect();
    names.sort_unstable();
    names
}

#[test]
fn filters_by_status_and_junk() {
    let fixture = Fixture::new("filter");
    fixture.pushed_repo("backed-up");
    let scratch = fixture.plain_project("scratch");
    fs::create_dir_all(scratch.join("node_modules/a")).unwrap();
    fs::write(scratch.join("node_modules/a/index.js"), "x").unwrap();
    let mut app = fixture.app();

    press(&mut app, 'f');
    assert_eq!(visible_names(&app), ["backed-up"]);
    press(&mut app, 'f');
    assert_eq!(visible_names(&app), ["scratch"]);
    press(&mut app, 'f');
    assert_eq!(visible_names(&app), ["scratch"]);
    press(&mut app, 'f');
    assert_eq!(visible_names(&app), ["backed-up", "scratch"]);
}

#[test]
fn search_narrows_list_and_escape_clears_it() {
    let fixture = Fixture::new("search");
    fixture.plain_project("alpha-web");
    fixture.plain_project("alpha-api");
    fixture.plain_project("beta");
    let mut app = fixture.app();

    press(&mut app, '/');
    type_text(&mut app, "ALPHA");
    assert_eq!(visible_names(&app), ["alpha-api", "alpha-web"]);

    app.on_key(KeyEvent::from(KeyCode::Enter));
    assert!(matches!(app.mode, app::Mode::Browse));
    assert!(render(&mut app).contains("matching \"ALPHA\""));

    app.on_key(KeyEvent::from(KeyCode::Esc));
    assert_eq!(visible_names(&app).len(), 3);
}

#[test]
fn marked_projects_hidden_by_search_are_not_acted_on() {
    let fixture = Fixture::new("hidden-marks");
    fixture.plain_project("keep");
    fixture.plain_project("other");
    let mut app = fixture.app();

    select(&mut app, "keep");
    press(&mut app, ' ');
    press(&mut app, '/');
    type_text(&mut app, "other");
    app.on_key(KeyEvent::from(KeyCode::Enter));

    assert_eq!(app.summary().marked_count, 0);
    press(&mut app, 'd');
    assert!(matches!(app.mode, app::Mode::Browse));
    assert!(app.notice.as_ref().unwrap().text.contains("hidden"));
}

#[test]
fn help_opens_and_closes_with_any_key() {
    let fixture = Fixture::new("help");
    fixture.plain_project("p");
    let mut app = fixture.app();

    press(&mut app, '?');
    assert!(render(&mut app).contains("Press any key to close"));
    press(&mut app, 'd');
    assert!(matches!(app.mode, app::Mode::Browse));
}

fn row<'a>(app: &'a App, name: &str) -> &'a project::Project {
    app.projects.iter().find(|p| p.name == name).unwrap()
}

#[test]
fn attributes_claude_history_to_repos_parent_folders_and_deleted_paths() {
    let fixture = Fixture::new("claude-assign");
    fixture.plain_project("his/his-be");
    fixture.plain_project("his/his-fe");
    fixture.claude_history("his/his-be");
    fixture.claude_history("his");
    fixture.claude_history("old-repo");
    let app = fixture.app();

    assert!(row(&app, "his/his-be").claude_size() > 0);
    assert_eq!(row(&app, "his/his-fe").claude_size(), 0);
    assert_eq!(row(&app, "his").kind, project::ProjectKind::Folder);
    assert_eq!(row(&app, "old-repo").kind, project::ProjectKind::Missing);
    assert!(app.summary().junk > 0);
}

#[test]
fn cleans_claude_history_but_keeps_memory() {
    let fixture = Fixture::new("claude-clean");
    fixture.plain_project("his/his-be");
    let history = fixture.claude_history("his");
    let mut app = fixture.app();

    select(&mut app, "his");
    press(&mut app, 'h');
    assert!(render(&mut app).contains("Clean Claude history"));
    press(&mut app, 'y');
    wait_until(&mut app, |a| a.freed > 0);

    assert!(!history.join("old.jsonl").exists());
    assert!(history.join("memory/MEMORY.md").exists());
    assert!(fixture.root.join("his/his-be/package.json").exists());
    assert_eq!(row(&app, "his").claude_size(), 0);
}

#[test]
fn folder_rows_cannot_be_deleted() {
    let fixture = Fixture::new("claude-nodelete");
    fixture.plain_project("his/his-be");
    fixture.claude_history("his");
    let mut app = fixture.app();

    select(&mut app, "his");
    press(&mut app, 'd');

    assert!(matches!(app.mode, app::Mode::Browse));
    assert!(fixture.root.join("his/his-be").exists());
    assert!(app.notice.as_ref().unwrap().text.contains("Claude history"));
}

fn selected_name(app: &App) -> &str {
    &app.selected().unwrap().name
}

fn sorted_by_name(fixture: &Fixture, count: usize) -> App {
    for i in 1..=count {
        fixture.plain_project(&format!("p{i:02}"));
    }
    let mut app = fixture.app();
    while app.sort.key != app::SortKey::Name {
        press(&mut app, 's');
    }
    app
}

#[test]
fn vim_counts_move_by_many_rows() {
    let fixture = Fixture::new("vim-count");
    let mut app = sorted_by_name(&fixture, 20);

    type_text(&mut app, "10");
    assert_eq!(app.pending_keys(), "10");
    press(&mut app, 'j');
    assert_eq!(selected_name(&app), "p11");
    assert_eq!(app.pending_keys(), "");

    type_text(&mut app, "3k");
    assert_eq!(selected_name(&app), "p08");

    type_text(&mut app, "99j");
    assert_eq!(selected_name(&app), "p20");
}

#[test]
fn vim_jumps_to_top_bottom_and_line() {
    let fixture = Fixture::new("vim-jump");
    let mut app = sorted_by_name(&fixture, 20);

    press(&mut app, 'G');
    assert_eq!(selected_name(&app), "p20");
    press(&mut app, 'g');
    assert_eq!(app.pending_keys(), "g");
    assert_eq!(selected_name(&app), "p20");
    press(&mut app, 'g');
    assert_eq!(selected_name(&app), "p01");

    type_text(&mut app, "5G");
    assert_eq!(selected_name(&app), "p05");
    type_text(&mut app, "12gg");
    assert_eq!(selected_name(&app), "p12");
}

#[test]
fn sort_direction_can_be_reversed() {
    let fixture = Fixture::new("sort-reverse");
    let mut app = sorted_by_name(&fixture, 3);
    assert_eq!(app.sort.label(), "name ↑");
    press(&mut app, 'g');
    press(&mut app, 'g');
    assert_eq!(selected_name(&app), "p01");

    press(&mut app, 'S');
    assert_eq!(app.sort.label(), "name ↓");
    press(&mut app, 'g');
    press(&mut app, 'g');
    assert_eq!(selected_name(&app), "p03");

    press(&mut app, 's');
    assert_eq!(app.sort.label(), "size ↓");
}

#[test]
#[ignore = "scans a real folder; run with SCAN_DIR=<path> cargo test render_snapshot -- --ignored --nocapture"]
fn render_snapshot() {
    let dir = std::env::var("SCAN_DIR").unwrap_or_else(|_| ".".into());
    let mut app = start(Path::new(&dir), claude::default_projects_dir());
    if let Ok(key) = std::env::var("PRESS") {
        key.chars().for_each(|c| press(&mut app, c));
    }
    println!("{}", render(&mut app));
}
