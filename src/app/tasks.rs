use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::thread;

use super::{App, Mode, Msg, NoticeLevel};
use crate::clean::{self, CleanScope};
use crate::delete;
use crate::format::bytes;
use crate::git::{self, Safety};
use crate::measure::measure;
use crate::project::Artifact;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Task {
    Cleaning,
    Deleting,
}

impl Task {
    pub fn label(self) -> &'static str {
        match self {
            Task::Cleaning => "CLEANING…",
            Task::Deleting => "DELETING…",
        }
    }
}

pub struct CleanTarget {
    pub path: PathBuf,
    pub name: String,
    pub artifacts: Vec<Artifact>,
}

impl CleanTarget {
    pub fn size(&self) -> u64 {
        self.artifacts.iter().map(|a| a.size).sum()
    }
}

pub struct CleanRequest {
    pub scope: CleanScope,
    pub targets: Vec<CleanTarget>,
}

impl CleanRequest {
    pub fn size(&self) -> u64 {
        self.targets.iter().map(CleanTarget::size).sum()
    }
}

pub struct DeleteTarget {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub safety: Safety,
}

pub struct DeleteRequest {
    pub targets: Vec<DeleteTarget>,
}

impl DeleteRequest {
    pub const CONFIRM_WORD: &'static str = "delete";

    pub fn size(&self) -> u64 {
        self.targets.iter().map(|t| t.size).sum()
    }

    pub fn unsafe_count(&self) -> usize {
        self.targets.iter().filter(|t| !t.safety.is_safe()).count()
    }

    pub fn requires_typing(&self) -> bool {
        self.unsafe_count() > 0
    }
}

impl App {
    pub(super) fn request_clean(&mut self, scope: CleanScope) {
        let mut targets = Vec::new();
        let mut skipped = Vec::new();
        for project in self.targets() {
            if self.task(&project.path).is_some() {
                skipped.push(format!("{} is busy", project.name));
                continue;
            }
            let Some(usage) = &project.usage else {
                skipped.push(format!("{} is still being measured", project.name));
                continue;
            };
            let artifacts: Vec<Artifact> = usage
                .artifacts
                .iter()
                .filter(|a| scope.includes(a.kind))
                .cloned()
                .collect();
            if !artifacts.is_empty() {
                targets.push(CleanTarget {
                    path: project.path.clone(),
                    name: project.name.clone(),
                    artifacts,
                });
            }
        }

        if targets.is_empty() {
            let reason = skipped
                .into_iter()
                .next()
                .unwrap_or_else(|| format!("No {} to clean", scope.noun()));
            self.notify(NoticeLevel::Info, reason);
        } else {
            self.mode = Mode::ConfirmClean(CleanRequest { scope, targets });
        }
    }

    pub(super) fn request_delete(&mut self) {
        let mut targets = Vec::new();
        let mut skipped = Vec::new();
        for project in self.targets() {
            let refusal = if self.task(&project.path).is_some() {
                Some("it is busy")
            } else if project.safety().is_none() {
                Some("git is still being checked")
            } else {
                delete::guard(&project.path, &self.roots).err()
            };
            match (refusal, project.safety()) {
                (None, Some(safety)) => targets.push(DeleteTarget {
                    path: project.path.clone(),
                    name: project.name.clone(),
                    size: project.size(),
                    safety,
                }),
                (reason, _) => skipped.push(format!(
                    "Can't delete {}: {}",
                    project.name,
                    reason.unwrap_or("unknown state")
                )),
            }
        }

        if let Some(first) = skipped.first() {
            let text = match skipped.len() {
                1 => first.clone(),
                n => format!("{first} (+{} more skipped)", n - 1),
            };
            self.notify(NoticeLevel::Info, text);
        }
        if !targets.is_empty() {
            self.mode = Mode::ConfirmDelete {
                request: DeleteRequest { targets },
                input: String::new(),
            };
        }
    }

    pub(super) fn start_clean(&mut self, request: CleanRequest) {
        for target in &request.targets {
            self.busy.insert(target.path.clone(), Task::Cleaning);
        }
        self.notify(
            NoticeLevel::Info,
            format!("Cleaning {}…", request.scope.noun()),
        );

        let tx = self.tx.clone();
        thread::spawn(move || run_clean(&tx, request));
    }

    pub(super) fn start_delete(&mut self, request: DeleteRequest, confirmed_unsafe: bool) {
        for target in &request.targets {
            self.busy.insert(target.path.clone(), Task::Deleting);
        }
        self.marked.clear();
        self.notify(NoticeLevel::Info, "Deleting…".into());

        let tx = self.tx.clone();
        let roots = self.roots.clone();
        thread::spawn(move || run_delete(&tx, request, &roots, confirmed_unsafe));
    }
}

fn run_clean(tx: &Sender<Msg>, request: CleanRequest) {
    let count = request.targets.len();
    let mut freed = 0;
    let mut failures = Vec::new();
    let mut last_name = String::new();

    for target in request.targets {
        let report = clean::remove_artifacts(&target.path, &target.artifacts);
        freed += report.freed;
        failures.extend(
            report
                .failures
                .into_iter()
                .map(|f| format!("{}/{f}", target.name)),
        );
        let _ = tx.send(Msg::Measured(target.path.clone(), measure(&target.path)));
        if target.path.join(".git").exists() {
            let _ = tx.send(Msg::GitInspected(
                target.path.clone(),
                git::inspect(&target.path),
            ));
        }
        let _ = tx.send(Msg::Cleaned(target.path, report.freed));
        last_name = target.name;
    }

    let subject = if count == 1 {
        last_name
    } else {
        format!("{count} projects")
    };
    let _ = tx.send(outcome(
        format!("Freed {} from {subject}", bytes(freed)),
        &failures,
    ));
}

fn run_delete(tx: &Sender<Msg>, request: DeleteRequest, roots: &[PathBuf], confirmed_unsafe: bool) {
    let mut deleted = 0;
    let mut freed = 0;
    let mut failures = Vec::new();

    for target in request.targets {
        let still_safe = || git::inspect(&target.path).safety() == Some(Safety::Safe);
        if !confirmed_unsafe && !still_safe() {
            failures.push(format!("{}: no longer safe, skipped", target.name));
            let _ = tx.send(Msg::Released(target.path));
            continue;
        }
        match delete::delete_project(&target.path, roots) {
            Ok(()) => {
                deleted += 1;
                freed += target.size;
                let _ = tx.send(Msg::Deleted(target.path, target.size));
            }
            Err(err) => {
                failures.push(format!("{}: {err}", target.name));
                let _ = tx.send(Msg::Released(target.path));
            }
        }
    }

    let noun = if deleted == 1 { "project" } else { "projects" };
    let _ = tx.send(outcome(
        format!("Deleted {deleted} {noun}, freed {}", bytes(freed)),
        &failures,
    ));
}

fn outcome(summary: String, failures: &[String]) -> Msg {
    match failures {
        [] => Msg::Notice(NoticeLevel::Success, summary),
        [first] => Msg::Notice(NoticeLevel::Error, format!("{summary} · failed {first}")),
        [first, rest @ ..] => Msg::Notice(
            NoticeLevel::Error,
            format!("{summary} · {} failed, e.g. {first}", rest.len() + 1),
        ),
    }
}
