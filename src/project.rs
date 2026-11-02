use std::path::PathBuf;

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

#[derive(Clone, Debug)]
pub struct Project {
    pub path: PathBuf,
    pub name: String,
    pub usage: Option<Usage>,
    pub git: GitState,
}

impl Project {
    pub fn new(path: PathBuf, name: String) -> Self {
        Self {
            git: GitState::initial(&path),
            path,
            name,
            usage: None,
        }
    }

    pub fn safety(&self) -> Option<Safety> {
        self.git.safety()
    }

    pub fn size(&self) -> u64 {
        self.usage.as_ref().map_or(0, |u| u.total)
    }

    pub fn junk(&self) -> u64 {
        self.usage.as_ref().map_or(0, Usage::junk)
    }

    pub fn last_commit(&self) -> Option<i64> {
        match &self.git {
            GitState::Repo(status) => status.last_commit,
            _ => None,
        }
    }
}
