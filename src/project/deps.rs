use std::path::Path;

#[derive(Debug, Default)]
pub struct DepsInfo {
    pub total: usize,
    pub manager: String,
}

/// Count dependencies by looking for package.json, Cargo.toml, requirements.txt, etc.
pub fn count_deps(path: &Path) -> DepsInfo {
    // Try package.json (npm/yarn/pnpm)
    let pkg_json = path.join("package.json");
    if pkg_json.exists() {
        if let Ok(content) = std::fs::read_to_string(&pkg_json) {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                let deps = parsed
                    .get("dependencies")
                    .and_then(|d| d.as_object())
                    .map(|d| d.len())
                    .unwrap_or(0);
                let dev_deps = parsed
                    .get("devDependencies")
                    .and_then(|d| d.as_object())
                    .map(|d| d.len())
                    .unwrap_or(0);
                return DepsInfo {
                    total: deps + dev_deps,
                    manager: "npm".to_string(),
                };
            }
        }
    }

    // Try Cargo.toml (Rust)
    let cargo_toml = path.join("Cargo.toml");
    if cargo_toml.exists() {
        if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
            if let Ok(parsed) = content.parse::<toml::Table>() {
                let deps = parsed
                    .get("dependencies")
                    .and_then(|d| d.as_table())
                    .map(|d| d.len())
                    .unwrap_or(0);
                let dev_deps = parsed
                    .get("dev-dependencies")
                    .and_then(|d| d.as_table())
                    .map(|d| d.len())
                    .unwrap_or(0);
                return DepsInfo {
                    total: deps + dev_deps,
                    manager: "cargo".to_string(),
                };
            }
        }
    }

    // Try requirements.txt (Python)
    let requirements = path.join("requirements.txt");
    if requirements.exists() {
        if let Ok(content) = std::fs::read_to_string(&requirements) {
            let count = content
                .lines()
                .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
                .count();
            return DepsInfo {
                total: count,
                manager: "pip".to_string(),
            };
        }
    }

    DepsInfo::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn counts_npm_deps() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{
            "dependencies": { "react": "^18", "next": "^15" },
            "devDependencies": { "typescript": "^5" }
        }"#,
        )
        .unwrap();
        let info = count_deps(dir.path());
        assert_eq!(info.total, 3);
        assert_eq!(info.manager, "npm");
    }

    #[test]
    fn counts_cargo_deps() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
serde = "1"
tokio = "1"

[dev-dependencies]
tempfile = "3"
"#,
        )
        .unwrap();
        let info = count_deps(dir.path());
        assert_eq!(info.total, 3);
        assert_eq!(info.manager, "cargo");
    }

    #[test]
    fn counts_pip_deps() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("requirements.txt"),
            "# comment\nflask==2.0\nrequests\n\ndjango>=4.0\n",
        )
        .unwrap();
        let info = count_deps(dir.path());
        assert_eq!(info.total, 3);
        assert_eq!(info.manager, "pip");
    }

    #[test]
    fn returns_default_for_empty_dir() {
        let dir = TempDir::new().unwrap();
        let info = count_deps(dir.path());
        assert_eq!(info.total, 0);
    }
}
