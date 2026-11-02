mod keys;
mod tasks;

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use ratatui::widgets::TableState;
use rayon::prelude::*;

use crate::discover::discover;
use crate::disk::DiskSpace;
use crate::git::{self, GitState, Safety};
use crate::measure::measure;
use crate::project::{Project, Usage};

pub use tasks::{CleanRequest, DeleteRequest, Task};

const NOTICE_DURATION: Duration = Duration::from_secs(6);

enum Msg {
    Discovered(Vec<Project>),
    GitInspected(PathBuf, GitState),
    Measured(PathBuf, Usage),
    Cleaned(PathBuf, u64),
    Deleted(PathBuf, u64),
    Released(PathBuf),
    Notice(NoticeLevel, String),
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
    pub marked_count: usize,
    pub marked_size: u64,
}

impl Summary {
    pub fn is_complete(&self) -> bool {
        self.measured == self.projects && self.inspected == self.projects
    }
}

pub enum Mode {
    Browse,
    ConfirmClean(CleanRequest),
    ConfirmDelete {
        request: DeleteRequest,
        input: String,
    },
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
    marked: HashSet<PathBuf>,
    busy: HashMap<PathBuf, Task>,
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
            marked: HashSet::new(),
            busy: HashMap::new(),
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
        self.marked.clear();
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
            self.apply(msg);
        }
        if received {
            self.rebuild_view();
        }
        received | self.expire_notice()
    }

    fn apply(&mut self, msg: Msg) {
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
            Msg::Cleaned(path, freed) => {
                self.busy.remove(&path);
                self.freed += freed;
                self.refresh_disk();
            }
            Msg::Deleted(path, freed) => {
                self.busy.remove(&path);
                self.marked.remove(&path);
                self.projects.retain(|p| p.path != path);
                self.freed += freed;
                self.refresh_disk();
            }
            Msg::Released(path) => {
                self.busy.remove(&path);
            }
            Msg::Notice(level, text) => self.notify(level, text),
        }
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
                if self.marked.contains(&p.path) {
                    s.marked_count += 1;
                    s.marked_size += p.size();
                }
                s
            },
        )
    }

    pub fn selected(&self) -> Option<&Project> {
        let index = *self.view.get(self.table.selected()?)?;
        self.projects.get(index)
    }

    pub fn is_marked(&self, path: &Path) -> bool {
        self.marked.contains(path)
    }

    pub fn task(&self, path: &Path) -> Option<Task> {
        self.busy.get(path).copied()
    }

    fn targets(&self) -> Vec<&Project> {
        if self.marked.is_empty() {
            self.selected().into_iter().collect()
        } else {
            self.view
                .iter()
                .map(|&i| &self.projects[i])
                .filter(|p| self.marked.contains(&p.path))
                .collect()
        }
    }

    fn toggle_mark(&mut self) {
        if let Some(path) = self.selected().map(|p| p.path.clone()) {
            if !self.marked.remove(&path) {
                self.marked.insert(path);
            }
            self.move_by(1);
        }
    }

    fn toggle_mark_all(&mut self) {
        if self.marked.len() == self.projects.len() {
            self.marked.clear();
        } else {
            self.marked = self.projects.iter().map(|p| p.path.clone()).collect();
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
        let previous_index = self.table.selected();
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

        let last = self.view.len().checked_sub(1);
        let index = selected
            .and_then(|path| {
                self.view
                    .iter()
                    .position(|&i| self.projects[i].path == path)
            })
            .or_else(|| Some(previous_index.unwrap_or(0).min(last?)));
        self.table.select(index);
    }
}
