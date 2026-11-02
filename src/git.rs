use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Clone, Debug)]
pub struct Remote {
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug)]
pub struct GitStatus {
    pub branch: Option<String>,
    pub remote: Option<Remote>,
    pub uncommitted: usize,
    pub unpushed: usize,
    pub stashes: usize,
    pub last_commit: Option<i64>,
}

#[derive(Clone, Debug)]
pub enum GitState {
    Pending,
    Missing,
    Broken,
    Repo(GitStatus),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Safety {
    Safe,
    Stashed(usize),
    Unpushed(usize),
    Uncommitted(usize),
    NoRemote,
    Broken,
    NoGit,
}

impl Safety {
    pub fn label(self) -> &'static str {
        match self {
            Safety::Safe => "SAFE",
            Safety::Stashed(_) => "STASHED",
            Safety::Unpushed(_) => "UNPUSHED",
            Safety::Uncommitted(_) => "UNCOMMITTED",
            Safety::NoRemote => "NO REMOTE",
            Safety::Broken => "GIT ERROR",
            Safety::NoGit => "NO GIT",
        }
    }

    pub fn rank(self) -> u8 {
        match self {
            Safety::Safe => 0,
            Safety::Stashed(_) => 1,
            Safety::Unpushed(_) => 2,
            Safety::Uncommitted(_) => 3,
            Safety::NoRemote => 4,
            Safety::Broken => 5,
            Safety::NoGit => 6,
        }
    }

    pub fn is_safe(self) -> bool {
        self == Safety::Safe
    }
}

impl GitStatus {
    pub fn safety(&self) -> Safety {
        if self.remote.is_none() {
            Safety::NoRemote
        } else if self.uncommitted > 0 {
            Safety::Uncommitted(self.uncommitted)
        } else if self.unpushed > 0 {
            Safety::Unpushed(self.unpushed)
        } else if self.stashes > 0 {
            Safety::Stashed(self.stashes)
        } else {
            Safety::Safe
        }
    }
}

impl GitState {
    pub fn initial(dir: &Path) -> Self {
        if dir.join(".git").exists() {
            GitState::Pending
        } else {
            GitState::Missing
        }
    }

    pub fn safety(&self) -> Option<Safety> {
        match self {
            GitState::Pending => None,
            GitState::Missing => Some(Safety::NoGit),
            GitState::Broken => Some(Safety::Broken),
            GitState::Repo(status) => Some(status.safety()),
        }
    }
}

pub fn inspect(dir: &Path) -> GitState {
    match read_status(dir) {
        Some(status) => GitState::Repo(status),
        None => GitState::Broken,
    }
}

fn read_status(dir: &Path) -> Option<GitStatus> {
    git(dir, &["rev-parse", "--git-dir"])?;
    let uncommitted = line_count(&git(dir, &["status", "--porcelain"])?);
    let unpushed = git(
        dir,
        &["rev-list", "--count", "--branches", "--not", "--remotes"],
    )?
    .parse()
    .ok()?;

    Some(GitStatus {
        branch: git(dir, &["symbolic-ref", "--short", "-q", "HEAD"]).filter(|b| !b.is_empty()),
        remote: remote(dir),
        uncommitted,
        unpushed,
        stashes: git(dir, &["stash", "list"]).map_or(0, |s| line_count(&s)),
        last_commit: git(dir, &["log", "-1", "--format=%ct"]).and_then(|s| s.parse().ok()),
    })
}

fn remote(dir: &Path) -> Option<Remote> {
    let names = git(dir, &["remote"])?;
    let name = names
        .lines()
        .find(|n| *n == "origin")
        .or_else(|| names.lines().next())?
        .to_owned();
    let url = git(dir, &["remote", "get-url", &name]).unwrap_or_default();
    Some(Remote { name, url })
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        // Read-only inspection must not take index locks that could block the user's own git.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output.status.success().then(|| {
        String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_owned()
    })
}

fn line_count(s: &str) -> usize {
    s.lines().filter(|l| !l.is_empty()).count()
}
