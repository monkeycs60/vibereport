use std::path::Path;

#[derive(Debug, Default)]
pub struct SecurityInfo {
    pub env_in_git: bool,
    pub has_env_example: bool,
    pub hardcoded_secrets_hints: usize,
}

/// Check for common security issues.
pub fn check_security(path: &Path) -> SecurityInfo {
    let mut info = SecurityInfo::default();

    // Check if .env is tracked in git (exists and not in .gitignore effectively)
    info.env_in_git = path.join(".env").exists() && !is_gitignored(path, ".env");
    info.has_env_example =
        path.join(".env.example").exists() || path.join(".env.local.example").exists();

    info
}

fn is_gitignored(repo_path: &Path, file: &str) -> bool {
    let gitignore = repo_path.join(".gitignore");
    if let Ok(content) = std::fs::read_to_string(gitignore) {
        content.lines().any(|line| {
            let line = line.trim();
            line == file || line == format!("/{}", file)
        })
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn detects_env_in_git_no_gitignore() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".env"), "SECRET=abc123").unwrap();

        let info = check_security(dir.path());
        assert!(info.env_in_git);
    }

    #[test]
    fn env_not_flagged_when_gitignored() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".env"), "SECRET=abc123").unwrap();
        fs::write(dir.path().join(".gitignore"), ".env\n").unwrap();

        let info = check_security(dir.path());
        assert!(!info.env_in_git);
    }

    #[test]
    fn env_not_flagged_when_gitignored_with_slash() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".env"), "SECRET=abc123").unwrap();
        fs::write(dir.path().join(".gitignore"), "/.env\n").unwrap();

        let info = check_security(dir.path());
        assert!(!info.env_in_git);
    }

    #[test]
    fn detects_env_example() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".env.example"), "SECRET=\n").unwrap();

        let info = check_security(dir.path());
        assert!(info.has_env_example);
    }

    #[test]
    fn no_security_issues_in_clean_dir() {
        let dir = TempDir::new().unwrap();
        let info = check_security(dir.path());
        assert!(!info.env_in_git);
        assert!(!info.has_env_example);
    }

    #[test]
    fn no_env_file_means_no_issue() {
        let dir = TempDir::new().unwrap();
        // No .env file at all
        let info = check_security(dir.path());
        assert!(!info.env_in_git);
    }
}
