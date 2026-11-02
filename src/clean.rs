use std::fs;
use std::io;
use std::path::Path;

use crate::project::{Artifact, ArtifactKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanScope {
    Deps,
    Cache,
    All,
}

impl CleanScope {
    pub fn title(self) -> &'static str {
        match self {
            CleanScope::Deps => "Clean dependencies",
            CleanScope::Cache => "Clean caches & build output",
            CleanScope::All => "Clean all junk",
        }
    }

    pub fn noun(self) -> &'static str {
        match self {
            CleanScope::Deps => "dependencies",
            CleanScope::Cache => "caches",
            CleanScope::All => "junk",
        }
    }

    pub fn includes(self, kind: ArtifactKind) -> bool {
        match self {
            CleanScope::Deps => kind == ArtifactKind::Deps,
            CleanScope::Cache => kind == ArtifactKind::Cache,
            CleanScope::All => true,
        }
    }
}

#[derive(Debug, Default)]
pub struct CleanReport {
    pub freed: u64,
    pub failures: Vec<String>,
}

pub fn remove_artifacts(project: &Path, artifacts: &[Artifact]) -> CleanReport {
    let mut report = CleanReport::default();
    for artifact in artifacts {
        match remove_dir(project, &artifact.path) {
            Ok(()) => report.freed += artifact.size,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => report.failures.push(format!("{}: {err}", artifact.rel)),
        }
    }
    report
}

fn remove_dir(project: &Path, path: &Path) -> io::Result<()> {
    if path == project || !path.starts_with(project) {
        return Err(io::Error::other(
            "refusing to remove a path outside the project",
        ));
    }
    if !fs::symlink_metadata(path)?.is_dir() {
        return Err(io::Error::other("not a directory"));
    }
    fs::remove_dir_all(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_project(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("repo-manager-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
        fs::write(dir.join("node_modules/pkg/index.js"), "x").unwrap();
        fs::write(dir.join("package.json"), "{}").unwrap();
        dir
    }

    fn artifact(path: PathBuf, size: u64) -> Artifact {
        Artifact {
            rel: path.display().to_string(),
            path,
            size,
            kind: ArtifactKind::Deps,
        }
    }

    #[test]
    fn removes_artifacts_and_keeps_source() {
        let project = temp_project("removes");
        let report = remove_artifacts(&project, &[artifact(project.join("node_modules"), 42)]);

        assert_eq!(report.freed, 42);
        assert_eq!(report.failures, Vec::<String>::new());
        assert!(!project.join("node_modules").exists());
        assert!(project.join("package.json").exists());
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn refuses_paths_outside_project() {
        let project = temp_project("outside");
        let outside = project.parent().unwrap().to_path_buf();
        let report = remove_artifacts(&project.join("node_modules"), &[artifact(outside, 1)]);

        assert_eq!(report.freed, 0);
        assert_eq!(report.failures.len(), 1);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn refuses_to_follow_symlinks() {
        let project = temp_project("symlink");
        let link = project.join("dist");
        std::os::unix::fs::symlink(project.join("node_modules"), &link).unwrap();
        let report = remove_artifacts(&project, &[artifact(link, 1)]);

        assert_eq!(report.failures.len(), 1);
        assert!(project.join("node_modules/pkg/index.js").exists());
        fs::remove_dir_all(project).unwrap();
    }
}
