use std::cmp::Reverse;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::TableState;
use rayon::prelude::*;

use crate::discover::discover;
use crate::disk::DiskSpace;
use crate::git::{self, GitState, Safety};
use crate::measure::measure;
use crate::project::{Project, Usage};

enum Msg {
    Discovered(Vec<Project>),
    GitInspected(PathBuf, GitState),
    Measured(PathBuf, Usage),
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

pub struct App {
    pub roots: Vec<PathBuf>,
    pub projects: Vec<Project>,
    pub view: Vec<usize>,
    pub table: TableState,
    pub sort: SortKey,
    pub disk: Option<DiskSpace>,
    pub discovering: bool,
    pub should_quit: bool,
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
            disk: None,
            discovering: false,
            should_quit: false,
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
        self.disk = self.roots.first().and_then(|r| DiskSpace::of(r));

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
        let mut changed = false;
        while let Ok(msg) = self.rx.try_recv() {
            changed = true;
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
            }
        }
        if changed {
            self.rebuild_view();
        }
        changed
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

    pub fn on_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
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
            _ => {}
        }
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

    fn project_mut(&mut self, path: &PathBuf) -> Option<&mut Project> {
        self.projects.iter_mut().find(|p| &p.path == path)
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
