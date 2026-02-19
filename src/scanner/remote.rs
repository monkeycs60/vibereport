use std::path::PathBuf;
use std::process::Command;

/// Parse "github:user/repo" format and return (user, repo).
/// Also accepts "https://github.com/user/repo" and "github.com/user/repo"
pub fn parse_github_ref(input: &str) -> Option<(String, String)> {
    let stripped = input
        .strip_prefix("github:")
        .or_else(|| input.strip_prefix("https://github.com/"))
        .or_else(|| input.strip_prefix("github.com/"))?;
    let parts: Vec<&str> = stripped.trim_end_matches('/').splitn(2, '/').collect();
    if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
        Some((parts[0].to_string(), parts[1].to_string()))
    } else {
        None
    }
}

/// Shallow-clone a GitHub repo into a temp directory for analysis.
/// Uses --depth 500 to get enough commit history for meaningful AI detection.
/// NOTE: Uses system `git` instead of `gix` because gix does not support
/// shallow clone (--depth) which is critical for performance on large repos.
pub fn clone_for_analysis(user: &str, repo: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let tmp_dir = std::env::temp_dir().join(format!("vibereport-{}-{}", user, repo));

    // Clean up previous clone if exists
    if tmp_dir.exists() {
        std::fs::remove_dir_all(&tmp_dir)?;
    }

    let url = format!("https://github.com/{}/{}.git", user, repo);
    let dest = tmp_dir.to_string_lossy().to_string();
    let output = Command::new("git")
        .args(["clone", "--depth", "500", &url, &dest])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to clone {}/{}: {}", user, repo, stderr.trim()).into());
    }

    Ok(tmp_dir)
}

/// Clean up the temp directory after analysis.
pub fn cleanup(path: &std::path::Path) {
    let _ = std::fs::remove_dir_all(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_github_colon_format() {
        let result = parse_github_ref("github:vercel/next.js");
        assert_eq!(result, Some(("vercel".into(), "next.js".into())));
    }

    #[test]
    fn parses_github_url_format() {
        let result = parse_github_ref("https://github.com/anthropics/claude-code");
        assert_eq!(result, Some(("anthropics".into(), "claude-code".into())));
    }

    #[test]
    fn parses_github_com_format() {
        let result = parse_github_ref("github.com/rust-lang/rust");
        assert_eq!(result, Some(("rust-lang".into(), "rust".into())));
    }

    #[test]
    fn strips_trailing_slash() {
        let result = parse_github_ref("github:user/repo/");
        assert_eq!(result, Some(("user".into(), "repo".into())));
    }

    #[test]
    fn returns_none_for_local_path() {
        assert_eq!(parse_github_ref("/some/local/path"), None);
        assert_eq!(parse_github_ref("."), None);
        assert_eq!(parse_github_ref("./my-project"), None);
    }

    #[test]
    fn returns_none_for_incomplete() {
        assert_eq!(parse_github_ref("github:"), None);
        assert_eq!(parse_github_ref("github:user"), None);
        assert_eq!(parse_github_ref("github:/repo"), None);
    }
}
