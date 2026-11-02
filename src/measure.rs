use std::collections::HashSet;
use std::fs::{self, Metadata};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use crate::project::{Artifact, ArtifactKind, Usage};

type InodeSet = HashSet<(u64, u64)>;

pub fn measure(project: &Path) -> Usage {
    let mut usage = Usage::default();
    let mut seen = InodeSet::new();
    usage.total = walk(project, project, &mut usage.artifacts, &mut seen);
    usage.artifacts.sort_by_key(|a| std::cmp::Reverse(a.size));
    usage
}

fn walk(root: &Path, dir: &Path, artifacts: &mut Vec<Artifact>, seen: &mut InodeSet) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let mut total = 0;
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_dir() {
            total += disk_usage(&meta, seen);
            continue;
        }
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == ".git" {
            total += dir_size(&path, seen);
        } else if let Some(kind) = classify(&name, dir) {
            let size = dir_size(&path, seen);
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .display()
                .to_string();
            artifacts.push(Artifact {
                path,
                rel,
                size,
                kind,
            });
            total += size;
        } else {
            total += walk(root, &path, artifacts, seen);
        }
    }
    total
}

fn dir_size(dir: &Path, seen: &mut InodeSet) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter_map(|entry| Some((entry.metadata().ok()?, entry)))
        .map(|(meta, entry)| {
            if meta.is_dir() {
                dir_size(&entry.path(), seen)
            } else {
                disk_usage(&meta, seen)
            }
        })
        .sum()
}

/// Allocated blocks rather than file length, with hard links counted once,
/// so totals match what deleting would actually free.
fn disk_usage(meta: &Metadata, seen: &mut InodeSet) -> u64 {
    if meta.nlink() > 1 && !seen.insert((meta.dev(), meta.ino())) {
        return 0;
    }
    meta.blocks() * 512
}

fn classify(name: &str, parent: &Path) -> Option<ArtifactKind> {
    use ArtifactKind::{Cache, Deps};
    let has = |file: &str| parent.join(file).exists();
    match name {
        "node_modules" => Some(Deps),
        "Pods" if has("Podfile") => Some(Deps),
        ".venv" | "venv" | "env" if parent.join(name).join("pyvenv.cfg").exists() => Some(Deps),

        ".next" | ".nuxt" | ".svelte-kit" | ".turbo" | ".parcel-cache" | ".angular" | ".expo"
        | ".vite" | ".docusaurus" | ".astro" | ".dart_tool" | "__pycache__" | ".pytest_cache"
        | ".mypy_cache" | ".ruff_cache" | ".tox" | ".gradle" | ".zig-cache" | "zig-cache"
        | "DerivedData" => Some(Cache),

        ".cache" if has("package.json") => Some(Cache),
        "target" if has("Cargo.toml") || has("pom.xml") => Some(Cache),
        "build"
            if [
                "package.json",
                "build.gradle",
                "build.gradle.kts",
                "CMakeLists.txt",
                "pubspec.yaml",
                "setup.py",
            ]
            .iter()
            .any(|f| has(f)) =>
        {
            Some(Cache)
        }
        "dist" | "out" | "coverage" if has("package.json") => Some(Cache),
        "dist" if has("setup.py") || has("pyproject.toml") => Some(Cache),
        "zig-out" if has("build.zig") => Some(Cache),
        _ => None,
    }
}
