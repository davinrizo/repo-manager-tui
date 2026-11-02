use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub fn guard(path: &Path, roots: &[PathBuf]) -> Result<(), &'static str> {
    if roots.iter().any(|root| root.starts_with(path)) {
        return Err("it contains a scanned folder");
    }
    if !roots.iter().any(|root| path.starts_with(root)) {
        return Err("it is outside the scanned folders");
    }
    if std::env::current_dir().is_ok_and(|cwd| cwd.starts_with(path)) {
        return Err("the terminal is currently inside it");
    }
    Ok(())
}

pub fn delete_project(path: &Path, roots: &[PathBuf]) -> io::Result<()> {
    guard(path, roots).map_err(io::Error::other)?;
    if !fs::symlink_metadata(path)?.is_dir() {
        return Err(io::Error::other("not a directory"));
    }
    fs::remove_dir_all(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("repo-manager-del-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("project/src")).unwrap();
        fs::write(root.join("project/src/main.rs"), "fn main() {}").unwrap();
        root
    }

    #[test]
    fn deletes_project_inside_root() {
        let root = temp_root("inside");
        delete_project(&root.join("project"), std::slice::from_ref(&root)).unwrap();

        assert!(!root.join("project").exists());
        assert!(root.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_root_and_its_ancestors() {
        let root = temp_root("root");
        let roots = std::slice::from_ref(&root);

        assert!(delete_project(&root, roots).is_err());
        assert!(delete_project(root.parent().unwrap(), roots).is_err());
        assert!(root.join("project").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_paths_outside_roots() {
        let root = temp_root("outside");
        let other = temp_root("outside-other");

        assert!(delete_project(&other.join("project"), std::slice::from_ref(&root)).is_err());
        assert!(other.join("project").exists());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(other).unwrap();
    }

    #[test]
    fn refuses_symlinked_projects() {
        let root = temp_root("symlink");
        let link = root.join("link");
        std::os::unix::fs::symlink(root.join("project"), &link).unwrap();

        assert!(delete_project(&link, std::slice::from_ref(&root)).is_err());
        assert!(root.join("project/src/main.rs").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
