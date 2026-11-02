use std::cmp::Reverse;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::TableState;
use rayon::prelude::*;

use crate::clean::{self, CleanReport, CleanScope};
use crate::discover::discover;
use crate::disk::DiskSpace;
use crate::format::bytes;
use crate::git::{self, GitState, Safety};
use crate::measure::measure;
use crate::project::{Artifact, Project, Usage};

const NOTICE_DURATION: Duration = Duration::from_secs(6);

enum Msg {
    Discovered(Vec<Project>),
    GitInspected(PathBuf, GitState),
    Measured(PathBuf, Usage),
    Cleaned(PathBuf, CleanReport),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Size,
    Junk,
    Status,
    LastCommit,
    Name,
}

impl SortKey {
    pub fn label(self) -> &'static str {
        match self {
            SortKey::Size => "size",
            SortKey::Junk => "junk",
            SortKey::Status => "status",
            SortKey::LastCommit => "oldest commit",
            SortKey::Name => "name",
        }
    }

    fn next(self) -> Self {
        match self {
            SortKey::Size => SortKey::Junk,
            SortKey::Junk => SortKey::Status,
            SortKey::Status => SortKey::LastCommit,
            SortKey::LastCommit => SortKey::Name,
            SortKey::Name => SortKey::Size,
        }
    }
}

#[derive(Default)]
pub struct Summary {
    pub projects: usize,
    pub measured: usize,
    pub inspected: usize,
    pub total_size: u64,
    pub junk: u64,
    pub safe_count: usize,
    pub safe_size: u64,
}

impl Summary {
    pub fn is_complete(&self) -> bool {
        self.measured == self.projects && self.inspected == self.projects
    }
}

pub struct CleanRequest {
    pub project: PathBuf,
    pub name: String,
    pub scope: CleanScope,
    pub artifacts: Vec<Artifact>,
}

impl CleanRequest {
    pub fn size(&self) -> u64 {
        self.artifacts.iter().map(|a| a.size).sum()
    }
}

pub enum Mode {
    Browse,
    ConfirmClean(CleanRequest),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NoticeLevel {
    Info,
    Success,
    Error,
}

pub struct Notice {
    pub text: String,
    pub level: NoticeLevel,
    shown_at: Instant,
}

pub struct App {
    pub roots: Vec<PathBuf>,
    pub projects: Vec<Project>,
    pub view: Vec<usize>,
    pub table: TableState,
    pub sort: SortKey,
    pub mode: Mode,
    pub notice: Option<Notice>,
    pub disk: Option<DiskSpace>,
    pub freed: u64,
    pub discovering: bool,
    pub should_quit: bool,
    busy: HashSet<PathBuf>,
    max_depth: usize,
    user_moved: bool,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
}

impl App {
    pub fn new(roots: Vec<PathBuf>, max_depth: usize) -> Self {
        let (tx, rx) = mpsc::channel();
        let mut app = Self {
            roots,
            projects: Vec::new(),
            view: Vec::new(),
            table: TableState::default(),
            sort: SortKey::Size,
            mode: Mode::Browse,
            notice: None,
            disk: None,
            freed: 0,
            discovering: false,
            should_quit: false,
            busy: HashSet::new(),
            max_depth,
            user_moved: false,
            tx,
            rx,
        };
        app.rescan();
        app
    }

    pub fn rescan(&mut self) {
        self.discovering = true;
        self.user_moved = false;
        self.projects.clear();
        self.view.clear();
        self.refresh_disk();

        let tx = self.tx.clone();
        let roots = self.roots.clone();
        let max_depth = self.max_depth;
        thread::spawn(move || {
            let projects = discover(&roots, max_depth);
            let paths: Vec<PathBuf> = projects.iter().map(|p| p.path.clone()).collect();
            let repos: Vec<PathBuf> = projects
                .iter()
                .filter(|p| matches!(p.git, GitState::Pending))
                .map(|p| p.path.clone())
                .collect();
            if tx.send(Msg::Discovered(projects)).is_err() {
                return;
            }
            repos.par_iter().for_each(|path| {
                let _ = tx.send(Msg::GitInspected(path.clone(), git::inspect(path)));
            });
            paths.par_iter().for_each(|path| {
                let _ = tx.send(Msg::Measured(path.clone(), measure(path)));
            });
        });
    }

    pub fn process_messages(&mut self) -> bool {
        let mut received = false;
        while let Ok(msg) = self.rx.try_recv() {
            received = true;
            match msg {
                Msg::Discovered(projects) => {
                    self.discovering = false;
                    self.projects = projects;
                }
                Msg::GitInspected(path, state) => {
                    if let Some(project) = self.project_mut(&path) {
                        project.git = state;
                    }
                }
                Msg::Measured(path, usage) => {
                    if let Some(project) = self.project_mut(&path) {
                        project.usage = Some(usage);
                    }
                }
                Msg::Cleaned(path, report) => self.finish_clean(&path, &report),
            }
        }
        if received {
            self.rebuild_view();
        }
        received | self.expire_notice()
    }

    pub fn summary(&self) -> Summary {
        self.projects.iter().fold(
            Summary {
                projects: self.projects.len(),
                ..Summary::default()
            },
            |mut s, p| {
                if let Some(usage) = &p.usage {
                    s.measured += 1;
                    s.total_size += usage.total;
                    s.junk += usage.junk();
                }
                if let Some(safety) = p.safety() {
                    s.inspected += 1;
                    if safety.is_safe() {
                        s.safe_count += 1;
                        s.safe_size += p.size();
                    }
                }
                s
            },
        )
    }

    pub fn selected(&self) -> Option<&Project> {
        let index = *self.view.get(self.table.selected()?)?;
        self.projects.get(index)
    }

    pub fn is_busy(&self, path: &Path) -> bool {
        self.busy.contains(path)
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return;
        }
        match self.mode {
            Mode::Browse => self.on_browse_key(key),
            Mode::ConfirmClean(_) => self.on_confirm_key(key),
        }
    }

    fn on_browse_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Down | KeyCode::Char('j') => self.move_by(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_by(-1),
            KeyCode::PageDown => self.move_by(10),
            KeyCode::PageUp => self.move_by(-10),
            KeyCode::Home | KeyCode::Char('g') => self.move_to(0),
            KeyCode::End | KeyCode::Char('G') => self.move_to(usize::MAX),
            KeyCode::Char('s') => {
                self.sort = self.sort.next();
                self.rebuild_view();
            }
            KeyCode::Char('r') => self.rescan(),
            KeyCode::Char('n') => self.request_clean(CleanScope::Deps),
            KeyCode::Char('c') => self.request_clean(CleanScope::Cache),
            KeyCode::Char('x') => self.request_clean(CleanScope::All),
            _ => {}
        }
    }

    fn on_confirm_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => {
                if let Mode::ConfirmClean(request) = std::mem::replace(&mut self.mode, Mode::Browse)
                {
                    self.start_clean(request);
                }
            }
            KeyCode::Char('n' | 'q') | KeyCode::Esc => self.mode = Mode::Browse,
            _ => {}
        }
    }

    fn request_clean(&mut self, scope: CleanScope) {
        let Some(project) = self.selected() else {
            return;
        };
        let name = project.name.clone();
        let outcome = if self.is_busy(&project.path) {
            Err(format!("{name} is already being cleaned"))
        } else if let Some(usage) = &project.usage {
            let artifacts: Vec<Artifact> = usage
                .artifacts
                .iter()
                .filter(|a| scope.includes(a.kind))
                .cloned()
                .collect();
            if artifacts.is_empty() {
                Err(format!("No {} to clean in {name}", scope.noun()))
            } else {
                Ok(CleanRequest {
                    project: project.path.clone(),
                    name,
                    scope,
                    artifacts,
                })
            }
        } else {
            Err(format!("Still measuring {name}"))
        };

        match outcome {
            Ok(request) => self.mode = Mode::ConfirmClean(request),
            Err(text) => self.notify(NoticeLevel::Info, text),
        }
    }

    fn start_clean(&mut self, request: CleanRequest) {
        self.busy.insert(request.project.clone());
        self.notify(
            NoticeLevel::Info,
            format!("Cleaning {} in {}…", request.scope.noun(), request.name),
        );

        let tx = self.tx.clone();
        thread::spawn(move || {
            let path = request.project;
            let report = clean::remove_artifacts(&path, &request.artifacts);
            let _ = tx.send(Msg::Measured(path.clone(), measure(&path)));
            if path.join(".git").exists() {
                let _ = tx.send(Msg::GitInspected(path.clone(), git::inspect(&path)));
            }
            let _ = tx.send(Msg::Cleaned(path, report));
        });
    }

    fn finish_clean(&mut self, path: &Path, report: &CleanReport) {
        self.busy.remove(path);
        self.freed += report.freed;
        self.refresh_disk();

        let name = self
            .projects
            .iter()
            .find(|p| p.path == path)
            .map_or_else(|| path.display().to_string(), |p| p.name.clone());
        let freed = bytes(report.freed);
        match report.failures.as_slice() {
            [] => self.notify(NoticeLevel::Success, format!("Freed {freed} from {name}")),
            [first, rest @ ..] => self.notify(
                NoticeLevel::Error,
                format!(
                    "Freed {freed} from {name}; {} failed: {first}",
                    rest.len() + 1
                ),
            ),
        }
    }

    fn notify(&mut self, level: NoticeLevel, text: String) {
        self.notice = Some(Notice {
            text,
            level,
            shown_at: Instant::now(),
        });
    }

    fn expire_notice(&mut self) -> bool {
        let expired = self
            .notice
            .as_ref()
            .is_some_and(|n| n.shown_at.elapsed() > NOTICE_DURATION);
        if expired {
            self.notice = None;
        }
        expired
    }

    fn refresh_disk(&mut self) {
        self.disk = self.roots.first().and_then(|r| DiskSpace::of(r));
    }

    fn move_by(&mut self, delta: isize) {
        let current = self.table.selected().unwrap_or(0);
        self.move_to(current.saturating_add_signed(delta));
    }

    fn move_to(&mut self, index: usize) {
        self.user_moved = true;
        if !self.view.is_empty() {
            self.table.select(Some(index.min(self.view.len() - 1)));
        }
    }

    fn project_mut(&mut self, path: &Path) -> Option<&mut Project> {
        self.projects.iter_mut().find(|p| p.path == path)
    }

    fn rebuild_view(&mut self) {
        let selected = self
            .selected()
            .filter(|_| self.user_moved)
            .map(|p| p.path.clone());

        let projects = &self.projects;
        let mut view: Vec<usize> = (0..projects.len()).collect();
        match self.sort {
            SortKey::Size => view.sort_by_key(|&i| Reverse(projects[i].size())),
            SortKey::Junk => view.sort_by_key(|&i| Reverse(projects[i].junk())),
            SortKey::Status => view.sort_by_key(|&i| {
                let p = &projects[i];
                (p.safety().map_or(u8::MAX, Safety::rank), Reverse(p.size()))
            }),
            SortKey::LastCommit => {
                view.sort_by_key(|&i| projects[i].last_commit().unwrap_or(i64::MAX));
            }
            SortKey::Name => view.sort_by_cached_key(|&i| projects[i].name.to_lowercase()),
        }
        self.view = view;

        let index = selected
            .and_then(|path| {
                self.view
                    .iter()
                    .position(|&i| self.projects[i].path == path)
            })
            .or((!self.view.is_empty()).then_some(0));
        self.table.select(index);
    }
}
