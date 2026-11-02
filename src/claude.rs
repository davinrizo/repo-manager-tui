use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use rayon::prelude::*;

use crate::discover::display_name;
use crate::measure::size_of;
use crate::project::Project;

const PRESERVED: &[&str] = &["memory"];
const ACTIVE_WINDOW: Duration = Duration::from_secs(15 * 60);
const MAX_LINES_SCANNED: usize = 200;

#[derive(Clone, Debug)]
pub struct ClaudeHistory {
    pub dir: PathBuf,
    pub origin: PathBuf,
    pub size: u64,
    pub sessions: usize,
}

#[derive(Debug, Default)]
pub struct ClearReport {
    pub freed: u64,
    pub kept_active: usize,
}

pub fn default_projects_dir() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".claude")))
        .map(|config| config.join("projects"))
}

pub fn load(projects_dir: &Path) -> Vec<ClaudeHistory> {
    let Ok(entries) = fs::read_dir(projects_dir) else {
        return Vec::new();
    };
    let dirs: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    dirs.par_iter().filter_map(|dir| inspect(dir)).collect()
}

pub fn inspect(dir: &Path) -> Option<ClaudeHistory> {
    let name = dir.file_name()?.to_str()?;
    let origin = origin_of(dir, name)?;
    let (size, sessions) = cleanable_entries(dir)
        .iter()
        .fold((0, 0), |(size, sessions), path| {
            let is_session = path.extension().is_some_and(|e| e == "jsonl");
            (size + size_of(path), sessions + usize::from(is_session))
        });
    (size > 0).then(|| ClaudeHistory {
        dir: dir.to_path_buf(),
        origin,
        size,
        sessions,
    })
}

/// Projects own history started anywhere inside them. History for other folders
/// under the scan roots (parent folders, deleted repos) becomes its own row.
pub fn assign(projects: &mut Vec<Project>, histories: Vec<ClaudeHistory>, roots: &[PathBuf]) {
    let mut unowned: BTreeMap<PathBuf, Vec<ClaudeHistory>> = BTreeMap::new();
    for history in histories {
        let owner = projects
            .iter_mut()
            .filter(|p| history.origin.starts_with(&p.path))
            .max_by_key(|p| p.path.components().count());
        match owner {
            Some(project) => project.claude.push(history),
            None if roots.iter().any(|r| history.origin.starts_with(r)) => {
                unowned
                    .entry(history.origin.clone())
                    .or_default()
                    .push(history);
            }
            None => {}
        }
    }

    for (origin, histories) in unowned {
        let Some(root) = roots.iter().find(|r| origin.starts_with(r)) else {
            continue;
        };
        let name = display_name(root, &origin);
        let mut project = Project::claude_only(origin, name);
        project.claude = histories;
        projects.push(project);
    }
    projects.sort_by(|a, b| a.path.cmp(&b.path));
}

pub fn clear(projects_dir: &Path, dir: &Path) -> io::Result<ClearReport> {
    if dir.parent() != Some(projects_dir) || !fs::symlink_metadata(dir)?.is_dir() {
        return Err(io::Error::other("not a Claude project history folder"));
    }

    let active = active_sessions(dir);
    let mut report = ClearReport::default();
    for path in cleanable_entries(dir) {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if active.contains(stem) {
            report.kept_active += usize::from(path.extension().is_some_and(|e| e == "jsonl"));
            continue;
        }
        let size = size_of(&path);
        if fs::symlink_metadata(&path)?.is_dir() {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
        report.freed += size;
    }
    Ok(report)
}

pub fn encode(path: &Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn cleanable_entries(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| !PRESERVED.contains(&e.file_name().to_string_lossy().as_ref()))
        .map(|e| e.path())
        .collect()
}

/// Folder names are a lossy encoding of the launch directory, so the real path is
/// recovered from the working directories recorded inside the session logs.
fn origin_of(dir: &Path, name: &str) -> Option<PathBuf> {
    sessions(dir).into_iter().find_map(|log| {
        recorded_cwds(&log).find_map(|cwd| {
            cwd.ancestors()
                .find(|a| encode(a) == name)
                .map(Path::to_path_buf)
        })
    })
}

fn recorded_cwds(log: &Path) -> impl Iterator<Item = PathBuf> {
    File::open(log)
        .map(BufReader::new)
        .into_iter()
        .flat_map(BufRead::lines)
        .take(MAX_LINES_SCANNED)
        .map_while(Result::ok)
        .filter_map(|line| {
            let value: serde_json::Value = serde_json::from_str(&line).ok()?;
            value.get("cwd")?.as_str().map(PathBuf::from)
        })
}

fn sessions(dir: &Path) -> Vec<PathBuf> {
    cleanable_entries(dir)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .collect()
}

fn active_sessions(dir: &Path) -> HashSet<String> {
    let now = SystemTime::now();
    sessions(dir)
        .into_iter()
        .filter(|log| {
            fs::metadata(log)
                .and_then(|m| m.modified())
                .is_ok_and(|modified| {
                    now.duration_since(modified).unwrap_or_default() < ACTIVE_WINDOW
                })
        })
        .filter_map(|log| log.file_stem()?.to_str().map(str::to_owned))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Sandbox(PathBuf);

    impl Sandbox {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("repo-manager-claude-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn history(&self, origin: &Path, cwd: &Path) -> PathBuf {
            let dir = self.0.join(encode(origin));
            fs::create_dir_all(dir.join("memory")).unwrap();
            fs::create_dir_all(dir.join("old-session")).unwrap();
            fs::write(dir.join("memory/MEMORY.md"), "keep me").unwrap();
            fs::write(dir.join("old-session/tool.txt"), "x".repeat(5000)).unwrap();
            let line = serde_json::json!({ "type": "user", "cwd": cwd }).to_string();
            fs::write(
                dir.join("old-session.jsonl"),
                format!("{{\"type\":\"summary\"}}\n{line}\n"),
            )
            .unwrap();
            age(&dir.join("old-session.jsonl"));
            dir
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn age(path: &Path) {
        let old = SystemTime::now() - Duration::from_secs(3600);
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(old)
            .unwrap();
    }

    #[test]
    fn encodes_like_claude_code() {
        assert_eq!(
            encode(Path::new("/Users/me/work/tenant_management")),
            "-Users-me-work-tenant-management"
        );
    }

    #[test]
    fn resolves_origin_from_session_started_in_subfolder() {
        let sandbox = Sandbox::new("origin");
        let origin = Path::new("/work/ai/his");
        sandbox.history(origin, &origin.join("his-be/src"));

        let histories = load(&sandbox.0);
        assert_eq!(histories.len(), 1);
        assert_eq!(histories[0].origin, origin);
        assert_eq!(histories[0].sessions, 1);
    }

    #[test]
    fn clear_keeps_memory_and_active_sessions() {
        let sandbox = Sandbox::new("clear");
        let dir = sandbox.history(Path::new("/work/app"), Path::new("/work/app"));
        fs::write(dir.join("live.jsonl"), "{}").unwrap();

        let report = clear(&sandbox.0, &dir).unwrap();

        assert!(report.freed > 0);
        assert_eq!(report.kept_active, 1);
        assert!(dir.join("memory/MEMORY.md").exists());
        assert!(dir.join("live.jsonl").exists());
        assert!(!dir.join("old-session.jsonl").exists());
        assert!(!dir.join("old-session").exists());
    }

    #[test]
    fn clear_refuses_folders_outside_projects_dir() {
        let sandbox = Sandbox::new("guard");
        let dir = sandbox.history(Path::new("/work/app"), Path::new("/work/app"));

        assert!(clear(&sandbox.0, &dir.join("old-session")).is_err());
        assert!(dir.join("old-session/tool.txt").exists());
    }
}
