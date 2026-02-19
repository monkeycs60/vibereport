use std::path::{Path, PathBuf};

/// Directories that should be skipped during repo discovery.
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    ".git",
    "vendor",
    "dist",
    "build",
    ".next",
];

/// Recursively find all directories containing a `.git` folder.
/// Stops descending into a directory once a `.git` is found (doesn't look for nested repos).
/// Skips: node_modules, target, .git, vendor, dist, build, .next, and hidden directories.
pub fn find_git_repos(root: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut repos = Vec::new();
    walk_for_repos(root, &mut repos, 0, max_depth);
    repos
}

fn walk_for_repos(dir: &Path, repos: &mut Vec<PathBuf>, depth: usize, max_depth: usize) {
    if depth > max_depth {
        return;
    }

    // If this directory contains .git, it's a repo — add it and stop descending.
    if dir.join(".git").is_dir() {
        repos.push(dir.to_path_buf());
        return;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !SKIP_DIRS.contains(&name.as_str()) && !name.starts_with('.') {
                walk_for_repos(&path, repos, depth + 1, max_depth);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn finds_git_repos() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create two repos
        let repo_a = root.join("project-a");
        fs::create_dir_all(repo_a.join(".git")).unwrap();

        let repo_b = root.join("org").join("project-b");
        fs::create_dir_all(repo_b.join(".git")).unwrap();

        let repos = find_git_repos(root, 5);
        assert_eq!(repos.len(), 2);
        assert!(repos.contains(&repo_a));
        assert!(repos.contains(&repo_b));
    }

    #[test]
    fn skips_node_modules() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Real repo
        let real = root.join("real-project");
        fs::create_dir_all(real.join(".git")).unwrap();

        // Repo inside node_modules — should be skipped
        let nm_repo = root.join("node_modules").join("some-pkg");
        fs::create_dir_all(nm_repo.join(".git")).unwrap();

        let repos = find_git_repos(root, 5);
        assert_eq!(repos.len(), 1);
        assert!(repos.contains(&real));
    }

    #[test]
    fn respects_max_depth() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Shallow repo (depth 1)
        let shallow = root.join("shallow-project");
        fs::create_dir_all(shallow.join(".git")).unwrap();

        // Deep repo (depth 4)
        let deep = root.join("a").join("b").join("c").join("deep-project");
        fs::create_dir_all(deep.join(".git")).unwrap();

        // max_depth=2 should only find the shallow one
        let repos = find_git_repos(root, 2);
        assert_eq!(repos.len(), 1);
        assert!(repos.contains(&shallow));

        // max_depth=5 should find both
        let repos = find_git_repos(root, 5);
        assert_eq!(repos.len(), 2);
    }

    #[test]
    fn does_not_descend_into_found_repo() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // A repo with a nested sub-repo (like a submodule directory)
        let outer = root.join("outer");
        fs::create_dir_all(outer.join(".git")).unwrap();

        // Nested repo inside outer — should NOT be found because we stop at outer
        let inner = outer.join("submodules").join("inner");
        fs::create_dir_all(inner.join(".git")).unwrap();

        let repos = find_git_repos(root, 5);
        assert_eq!(repos.len(), 1);
        assert!(repos.contains(&outer));
    }

    #[test]
    fn skips_hidden_directories() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Visible repo
        let visible = root.join("visible-project");
        fs::create_dir_all(visible.join(".git")).unwrap();

        // Hidden directory containing a repo — should be skipped
        let hidden = root.join(".hidden-dir").join("secret-project");
        fs::create_dir_all(hidden.join(".git")).unwrap();

        let repos = find_git_repos(root, 5);
        assert_eq!(repos.len(), 1);
        assert!(repos.contains(&visible));
    }
}
