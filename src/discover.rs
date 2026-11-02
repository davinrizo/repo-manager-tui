use std::fs;
use std::path::{Path, PathBuf};

use crate::project::Project;

const PROJECT_MARKERS: &[&str] = &[
    ".git",
    "package.json",
    "Cargo.toml",
    "go.mod",
    "pyproject.toml",
    "requirements.txt",
    "setup.py",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "composer.json",
    "Gemfile",
    "CMakeLists.txt",
    "Makefile",
    "deno.json",
    "pubspec.yaml",
    "mix.exs",
    "Package.swift",
    "build.zig",
];

const IGNORED_DIRS: &[&str] = &["node_modules", "Library", "target", "vendor"];

pub fn discover(roots: &[PathBuf], max_depth: usize) -> Vec<Project> {
    let mut found = Vec::new();
    for root in roots {
        if is_project(root) {
            found.push((root.as_path(), root.clone()));
        } else {
            search(root, root, 1, max_depth, &mut found);
        }
    }
    found.sort_by(|a, b| a.1.cmp(&b.1));
    found.dedup_by(|a, b| a.1 == b.1);

    found
        .into_iter()
        .map(|(root, path)| {
            let name = display_name(root, &path);
            Project::new(path, name)
        })
        .collect()
}

fn is_project(dir: &Path) -> bool {
    PROJECT_MARKERS.iter().any(|m| dir.join(m).exists())
}

/// Top-level folders without any project inside are still reported, so loose
/// code that has no recognised marker file is never hidden from the user.
fn search<'a>(
    root: &'a Path,
    dir: &Path,
    depth: usize,
    max_depth: usize,
    found: &mut Vec<(&'a Path, PathBuf)>,
) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    let mut any = false;
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || IGNORED_DIRS.contains(&name.as_ref()) {
            continue;
        }
        let path = entry.path();
        if is_project(&path) {
            found.push((root, path));
            any = true;
        } else if depth < max_depth && search(root, &path, depth + 1, max_depth, found) {
            any = true;
        } else if depth == 1 {
            found.push((root, path));
            any = true;
        }
    }
    any
}

fn display_name(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(rel) if !rel.as_os_str().is_empty() => rel.display().to_string(),
        _ => path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        ),
    }
}
