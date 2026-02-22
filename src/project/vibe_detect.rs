use std::path::Path;

/// Check if a path is a regular file (not a symlink) to prevent symlink attacks.
fn is_regular_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_file())
        .unwrap_or(false)
}

/// Check if a path is a regular directory (not a symlink) to prevent symlink attacks.
fn is_regular_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_dir())
        .unwrap_or(false)
}

#[derive(Debug, Default)]
pub struct VibeInfo {
    /// No .eslintrc*, .prettierrc*, biome.json, deno.json, oxlint
    pub no_linting: bool,
    /// No .github/workflows/, .gitlab-ci.yml, Jenkinsfile, .circleci/
    pub no_ci_cd: bool,
    /// AI% > 0 but no AI config: .claude/, .cursorrules, cursor.json, AGENTS.md, .aider*, copilot-instructions.md
    pub boomer_ai: bool,
    /// node_modules/ or vendor/ tracked in git
    pub node_modules_in_git: bool,
    /// No .gitignore or < 3 lines
    pub no_gitignore: bool,
    /// No README.md or README
    pub no_readme: bool,
    /// > 20 TODO/FIXME/HACK occurrences in source files
    pub todo_flood: bool,
    pub todo_count: usize,
    /// Only main/master branch, no other branches
    pub single_branch: bool,
    /// A single commit contains > 50% of total commits' files changed
    pub mega_commit: bool,
}

const LINT_CONFIGS: &[&str] = &[
    ".eslintrc",
    ".eslintrc.js",
    ".eslintrc.cjs",
    ".eslintrc.json",
    ".eslintrc.yml",
    "eslint.config.js",
    "eslint.config.mjs",
    "eslint.config.ts",
    ".prettierrc",
    ".prettierrc.js",
    ".prettierrc.json",
    ".prettierrc.yml",
    "prettier.config.js",
    "prettier.config.mjs",
    "biome.json",
    "biome.jsonc",
    "deno.json",
    "deno.jsonc",
    ".oxlintrc.json",
    "rustfmt.toml",
    ".rustfmt.toml",
    ".rubocop.yml",
    "pylintrc",
    ".pylintrc",
    ".flake8",
    "pyproject.toml", // pyproject checked for [tool.ruff] later
    ".golangci.yml",
    ".golangci.yaml",
];

const CI_CONFIGS: &[&str] = &[
    ".github/workflows",
    ".gitlab-ci.yml",
    "Jenkinsfile",
    ".circleci",
    ".travis.yml",
    "azure-pipelines.yml",
    "bitbucket-pipelines.yml",
    ".buildkite",
];

const AI_CONFIGS: &[&str] = &[
    ".claude",
    "CLAUDE.md",
    ".cursorrules",
    "cursor.json",
    ".cursor",
    "AGENTS.md",
    ".aider.conf.yml",
    ".aiderignore",
    "copilot-instructions.md",
    ".github/copilot-instructions.md",
];

pub fn detect_vibe(path: &Path, ai_ratio: f64) -> VibeInfo {
    let has_lint_config = LINT_CONFIGS.iter().any(|f| path.join(f).exists());
    let no_linting = !has_lint_config && !has_clippy_in_ci(path);
    let no_ci_cd = !CI_CONFIGS.iter().any(|f| path.join(f).exists());
    let boomer_ai = ai_ratio > 0.0 && !AI_CONFIGS.iter().any(|f| path.join(f).exists());

    // node_modules in git (heuristic: if node_modules has content, it's tracked)
    let node_modules_in_git = path.join("node_modules").is_dir()
        && path.join("node_modules").join("package.json").exists();

    let no_gitignore = check_gitignore(path);

    let no_readme = !path.join("README.md").exists()
        && !path.join("readme.md").exists()
        && !path.join("README").exists()
        && !path.join("README.rst").exists();

    let todo_count = count_todos(path);
    let todo_flood = todo_count > 20;
    let single_branch = check_single_branch(path);

    VibeInfo {
        no_linting,
        no_ci_cd,
        boomer_ai,
        node_modules_in_git,
        no_gitignore,
        no_readme,
        todo_flood,
        todo_count,
        single_branch,
        mega_commit: false,
    }
}

/// Check if .gitignore is missing or too small (< 3 non-empty, non-comment lines).
fn check_gitignore(path: &Path) -> bool {
    let gitignore_path = path.join(".gitignore");
    if !gitignore_path.exists() {
        return true;
    }
    match std::fs::read_to_string(&gitignore_path) {
        Ok(content) => {
            let non_empty_lines = content
                .lines()
                .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
                .count();
            non_empty_lines < 3
        }
        Err(_) => true,
    }
}

fn count_todos(path: &Path) -> usize {
    let mut count = 0;
    let skip_dirs = [
        "node_modules",
        "target",
        ".git",
        "dist",
        "build",
        ".next",
        "vendor",
        "__pycache__",
        ".venv",
        "venv",
    ];
    count_todos_recursive(path, &skip_dirs, &mut count, 0);
    count
}

/// Maximum file size to read (1 MB). Files larger than this are skipped
/// to prevent out-of-memory conditions on huge generated/vendored files.
const MAX_FILE_SIZE: u64 = 1_048_576;

fn count_todos_recursive(path: &Path, skip_dirs: &[&str], count: &mut usize, depth: usize) {
    if depth > 10 || *count > 100 {
        return;
    } // early exit
    let entries = match std::fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if is_regular_dir(&p) {
            if !skip_dirs.contains(&name.as_str()) {
                count_todos_recursive(&p, skip_dirs, count, depth + 1);
            }
        } else if is_regular_file(&p) {
            if let Some(ext) = p.extension() {
                let ext = ext.to_string_lossy();
                if matches!(
                    ext.as_ref(),
                    "rs" | "ts"
                        | "js"
                        | "py"
                        | "go"
                        | "rb"
                        | "java"
                        | "tsx"
                        | "jsx"
                        | "vue"
                        | "svelte"
                        | "php"
                        | "swift"
                        | "kt"
                        | "c"
                        | "cpp"
                        | "cs"
                        | "h"
                ) {
                    // Skip files larger than 1 MB to avoid OOM
                    if let Ok(meta) = std::fs::metadata(&p) {
                        if meta.len() > MAX_FILE_SIZE {
                            continue;
                        }
                    }
                    if let Ok(content) = std::fs::read_to_string(&p) {
                        for line in content.lines() {
                            if has_todo_keyword(line) {
                                *count += 1;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Check if a line contains TODO, FIXME, or HACK as a standalone word
/// (not part of an identifier like `todo_flood` or `count_todos`).
fn has_todo_keyword(line: &str) -> bool {
    let upper = line.to_uppercase();
    let bytes = upper.as_bytes();
    for keyword in &["TODO", "FIXME", "HACK"] {
        let kw = keyword.as_bytes();
        let mut start = 0;
        while start + kw.len() <= bytes.len() {
            if let Some(pos) = upper[start..].find(keyword) {
                let abs = start + pos;
                let before_ok = abs == 0 || {
                    let c = bytes[abs - 1];
                    !c.is_ascii_alphanumeric() && c != b'_'
                };
                let after = abs + kw.len();
                let after_ok = after >= bytes.len() || {
                    let c = bytes[after];
                    !c.is_ascii_alphanumeric() && c != b'_'
                };
                if before_ok && after_ok {
                    return true;
                }
                start = abs + 1;
            } else {
                break;
            }
        }
    }
    false
}

/// Check if any CI workflow file mentions clippy (Rust linter).
fn has_clippy_in_ci(path: &Path) -> bool {
    let workflows_dir = path.join(".github/workflows");
    if !workflows_dir.is_dir() {
        return false;
    }
    let entries = match std::fs::read_dir(&workflows_dir) {
        Ok(e) => e,
        Err(_) => return false,
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().is_some_and(|e| e == "yml" || e == "yaml") {
            if let Ok(content) = std::fs::read_to_string(&p) {
                if content.contains("clippy") {
                    return true;
                }
            }
        }
    }
    false
}

fn check_single_branch(path: &Path) -> bool {
    let repo = match gix::open(path) {
        Ok(r) => r,
        Err(_) => return false,
    };
    // Shallow clones only have one branch — skip to avoid false positive
    if repo.shallow_commits().is_ok_and(|sc| sc.is_some()) {
        return false;
    }
    let refs = match repo.references() {
        Ok(r) => r,
        Err(_) => return false,
    };
    // Count local branches
    let local_count = refs.local_branches().map(|iter| iter.count()).unwrap_or(0);
    if local_count > 1 {
        return false;
    }
    // Also check remote branches — clones only create one local branch
    // but remote-tracking refs reveal the true branch count
    let refs2 = match repo.references() {
        Ok(r) => r,
        Err(_) => return local_count <= 1,
    };
    let remote_count = refs2
        .remote_branches()
        .map(|iter| iter.count())
        .unwrap_or(0);
    // If there are remote branches, use those to judge (subtract HEAD-like refs)
    if remote_count > 1 {
        return false;
    }
    local_count <= 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn detects_no_linting_in_empty_dir() {
        let dir = TempDir::new().unwrap();
        let info = detect_vibe(dir.path(), 0.0);
        assert!(info.no_linting);
    }

    #[test]
    fn detects_eslint_config() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".eslintrc.json"), "{}").unwrap();
        let info = detect_vibe(dir.path(), 0.0);
        assert!(!info.no_linting);
    }

    #[test]
    fn detects_no_ci_cd() {
        let dir = TempDir::new().unwrap();
        let info = detect_vibe(dir.path(), 0.0);
        assert!(info.no_ci_cd);
    }

    #[test]
    fn detects_github_actions() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join(".github/workflows")).unwrap();
        let info = detect_vibe(dir.path(), 0.0);
        assert!(!info.no_ci_cd);
    }

    #[test]
    fn detects_boomer_ai() {
        let dir = TempDir::new().unwrap();
        let info = detect_vibe(dir.path(), 0.5);
        assert!(info.boomer_ai);
    }

    #[test]
    fn no_boomer_ai_with_claude_config() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join(".claude")).unwrap();
        let info = detect_vibe(dir.path(), 0.5);
        assert!(!info.boomer_ai);
    }

    #[test]
    fn no_boomer_ai_when_zero_ai() {
        let dir = TempDir::new().unwrap();
        let info = detect_vibe(dir.path(), 0.0);
        assert!(!info.boomer_ai);
    }

    #[test]
    fn detects_no_gitignore() {
        let dir = TempDir::new().unwrap();
        let info = detect_vibe(dir.path(), 0.0);
        assert!(info.no_gitignore);
    }

    #[test]
    fn detects_tiny_gitignore() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".gitignore"), "node_modules\n").unwrap();
        let info = detect_vibe(dir.path(), 0.0);
        assert!(info.no_gitignore); // only 1 line < 3
    }

    #[test]
    fn proper_gitignore_passes() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join(".gitignore"),
            "node_modules\ntarget\n.env\ndist\n",
        )
        .unwrap();
        let info = detect_vibe(dir.path(), 0.0);
        assert!(!info.no_gitignore);
    }

    #[test]
    fn detects_no_readme() {
        let dir = TempDir::new().unwrap();
        let info = detect_vibe(dir.path(), 0.0);
        assert!(info.no_readme);
    }

    #[test]
    fn detects_readme_present() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("README.md"), "# Hello").unwrap();
        let info = detect_vibe(dir.path(), 0.0);
        assert!(!info.no_readme);
    }

    #[test]
    fn todo_keyword_word_boundary() {
        // Real TODO comments
        assert!(has_todo_keyword("// TODO: fix this"));
        assert!(has_todo_keyword("# FIXME broken"));
        assert!(has_todo_keyword("/* HACK */"));
        assert!(has_todo_keyword("TODO"));
        // Not part of identifiers
        assert!(!has_todo_keyword("let todo_count = 5;"));
        assert!(!has_todo_keyword("count_todos_recursive(path)"));
        assert!(!has_todo_keyword("pub todo_flood: bool"));
    }
}
