use std::path::PathBuf;

use crate::claude::ClaudeHistory;
use crate::git::{GitState, Safety};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactKind {
    Deps,
    Cache,
}

impl ArtifactKind {
    pub fn label(self) -> &'static str {
        match self {
            ArtifactKind::Deps => "deps",
            ArtifactKind::Cache => "cache",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Artifact {
    pub path: PathBuf,
    pub rel: String,
    pub size: u64,
    pub kind: ArtifactKind,
}

#[derive(Clone, Debug, Default)]
pub struct Usage {
    pub total: u64,
    pub artifacts: Vec<Artifact>,
}

impl Usage {
    pub fn junk(&self) -> u64 {
        self.artifacts.iter().map(|a| a.size).sum()
    }

    pub fn junk_of(&self, kind: ArtifactKind) -> u64 {
        self.artifacts
            .iter()
            .filter(|a| a.kind == kind)
            .map(|a| a.size)
            .sum()
    }

    pub fn source(&self) -> u64 {
        self.total.saturating_sub(self.junk())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectKind {
    Repo,
    Folder,
    Missing,
}

impl ProjectKind {
    pub fn label(self) -> &'static str {
        match self {
            ProjectKind::Repo => "REPO",
            ProjectKind::Folder => "FOLDER",
            ProjectKind::Missing => "GONE",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Project {
    pub path: PathBuf,
    pub name: String,
    pub kind: ProjectKind,
    pub usage: Option<Usage>,
    pub git: GitState,
    pub claude: Vec<ClaudeHistory>,
}

impl Project {
    pub fn new(path: PathBuf, name: String) -> Self {
        Self {
            git: GitState::initial(&path),
            path,
            name,
            kind: ProjectKind::Repo,
            usage: None,
            claude: Vec::new(),
        }
    }

    pub fn claude_only(path: PathBuf, name: String) -> Self {
        Self {
            kind: if path.is_dir() {
                ProjectKind::Folder
            } else {
                ProjectKind::Missing
            },
            git: GitState::Missing,
            path,
            name,
            usage: Some(Usage::default()),
            claude: Vec::new(),
        }
    }

    pub fn is_repo(&self) -> bool {
        self.kind == ProjectKind::Repo
    }

    pub fn safety(&self) -> Option<Safety> {
        if self.is_repo() {
            self.git.safety()
        } else {
            None
        }
    }

    pub fn is_inspected(&self) -> bool {
        !self.is_repo() || self.safety().is_some()
    }

    pub fn size(&self) -> u64 {
        self.usage.as_ref().map_or(0, |u| u.total)
    }

    pub fn claude_size(&self) -> u64 {
        self.claude.iter().map(|h| h.size).sum()
    }

    pub fn claude_sessions(&self) -> usize {
        self.claude.iter().map(|h| h.sessions).sum()
    }

    pub fn junk(&self) -> u64 {
        self.usage.as_ref().map_or(0, Usage::junk) + self.claude_size()
    }

    pub fn last_commit(&self) -> Option<i64> {
        match &self.git {
            GitState::Repo(status) => status.last_commit,
            _ => None,
        }
    }
}
