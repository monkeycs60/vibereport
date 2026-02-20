use std::path::Path;

#[derive(Debug, Default)]
pub struct SecurityInfo {
    pub env_in_git: bool,
    pub has_env_example: bool,
    pub hardcoded_secrets_hints: usize,
    /// Number of unignored .env* files found (for granular scoring)
    pub env_files_count: usize,
}

/// Common env file patterns that should never be committed.
const ENV_PATTERNS: &[&str] = &[
    ".env",
    ".env.local",
    ".env.development",
    ".env.production",
    ".env.staging",
    ".env.test",
    ".env.dev",
    ".env.prod",
];

/// Patterns that hint at hardcoded secrets in source code.
const SECRET_PATTERNS: &[&str] = &[
    "sk-",           // OpenAI keys
    "sk_live_",      // Stripe live keys
    "sk_test_",      // Stripe test keys
    "AKIA",          // AWS access key prefix
    "ghp_",          // GitHub personal access token
    "gho_",          // GitHub OAuth token
    "glpat-",        // GitLab personal access token
    "xoxb-",         // Slack bot token
    "xoxp-",         // Slack user token
    "Bearer eyJ",    // JWT in code
];

/// Check for common security issues.
pub fn check_security(path: &Path) -> SecurityInfo {
    let mut info = SecurityInfo::default();

    let gitignore_content = std::fs::read_to_string(path.join(".gitignore"))
        .unwrap_or_default();

    // Check all .env* patterns
    for pattern in ENV_PATTERNS {
        let env_path = path.join(pattern);
        if env_path.exists() && !is_ignored_by(&gitignore_content, pattern) {
            info.env_files_count += 1;
        }
    }

    info.env_in_git = info.env_files_count > 0;
    info.has_env_example =
        path.join(".env.example").exists() || path.join(".env.local.example").exists();

    // Scan for hardcoded secrets in common config files
    info.hardcoded_secrets_hints = count_secret_hints(path);

    info
}

/// Check if a file is covered by gitignore patterns.
fn is_ignored_by(gitignore_content: &str, file: &str) -> bool {
    gitignore_content.lines().any(|line| {
        let line = line.trim();
        // Exact match: .env
        if line == file {
            return true;
        }
        // With leading slash: /.env
        if line == format!("/{}", file) {
            return true;
        }
        // Glob pattern: .env* or .env.*
        if let Some(prefix) = line.strip_suffix('*') {
            if file.starts_with(prefix) {
                return true;
            }
        }
        false
    })
}

/// Scan common config files for patterns that look like hardcoded secrets.
fn count_secret_hints(path: &Path) -> usize {
    let candidates = [
        "src/config.ts",
        "src/config.js",
        "config.ts",
        "config.js",
        "src/constants.ts",
        "src/constants.js",
        "docker-compose.yml",
        "docker-compose.yaml",
        ".github/workflows/ci.yml",
    ];

    let mut count = 0;
    for candidate in &candidates {
        let file_path = path.join(candidate);
        if let Ok(content) = std::fs::read_to_string(&file_path) {
            for pattern in SECRET_PATTERNS {
                count += content.matches(pattern).count();
            }
        }
    }
    count
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
        assert_eq!(info.env_files_count, 1);
    }

    #[test]
    fn env_not_flagged_when_gitignored() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".env"), "SECRET=abc123").unwrap();
        fs::write(dir.path().join(".gitignore"), ".env\n").unwrap();

        let info = check_security(dir.path());
        assert!(!info.env_in_git);
        assert_eq!(info.env_files_count, 0);
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
        assert_eq!(info.env_files_count, 0);
    }

    #[test]
    fn no_env_file_means_no_issue() {
        let dir = TempDir::new().unwrap();
        let info = check_security(dir.path());
        assert!(!info.env_in_git);
    }

    // ── New tests ──

    #[test]
    fn detects_multiple_env_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".env"), "SECRET=abc").unwrap();
        fs::write(dir.path().join(".env.local"), "LOCAL=xyz").unwrap();
        fs::write(dir.path().join(".env.production"), "PROD=123").unwrap();

        let info = check_security(dir.path());
        assert!(info.env_in_git);
        assert_eq!(info.env_files_count, 3);
    }

    #[test]
    fn glob_gitignore_catches_env_variants() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".env"), "A=1").unwrap();
        fs::write(dir.path().join(".env.local"), "B=2").unwrap();
        fs::write(dir.path().join(".env.production"), "C=3").unwrap();
        fs::write(dir.path().join(".gitignore"), ".env*\n").unwrap();

        let info = check_security(dir.path());
        assert!(!info.env_in_git);
        assert_eq!(info.env_files_count, 0);
    }

    #[test]
    fn detects_hardcoded_secrets() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(
            dir.path().join("src/config.ts"),
            "const API_KEY = \"sk-abc123def456\";\nconst STRIPE = \"sk_live_test\";\n",
        )
        .unwrap();

        let info = check_security(dir.path());
        assert_eq!(info.hardcoded_secrets_hints, 2);
    }

    #[test]
    fn no_secrets_in_clean_config() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(
            dir.path().join("src/config.ts"),
            "const API_URL = \"https://api.example.com\";\n",
        )
        .unwrap();

        let info = check_security(dir.path());
        assert_eq!(info.hardcoded_secrets_hints, 0);
    }
}
