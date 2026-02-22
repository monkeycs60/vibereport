use std::path::Path;

#[derive(Debug, Default)]
pub struct TestsInfo {
    pub has_tests: bool,
    pub test_files_count: usize,
    pub frameworks: Vec<String>,
}

/// Detect presence of tests by looking for common test directories and config files.
pub fn detect_tests(path: &Path) -> TestsInfo {
    let mut info = TestsInfo::default();

    // Check common test directories
    let test_dirs = ["tests", "test", "__tests__", "spec", "src/test"];
    for dir in test_dirs {
        let test_path = path.join(dir);
        if test_path.is_dir() {
            info.has_tests = true;
            info.test_files_count += count_files_recursive(&test_path);
        }
    }

    // Check for test config files
    let test_configs = [
        ("jest.config.js", "Jest"),
        ("jest.config.ts", "Jest"),
        ("vitest.config.ts", "Vitest"),
        ("vitest.config.js", "Vitest"),
        ("pytest.ini", "pytest"),
        ("pyproject.toml", "pytest"),
        (".mocharc.yml", "Mocha"),
    ];
    for (file, framework) in test_configs {
        if path.join(file).exists() {
            info.has_tests = true;
            if !info.frameworks.contains(&framework.to_string()) {
                info.frameworks.push(framework.to_string());
            }
        }
    }

    // For Rust: check if tests/ dir exists or if Cargo.toml present with tests/
    if path.join("Cargo.toml").exists() && path.join("tests").is_dir() {
        info.has_tests = true;
        if !info.frameworks.contains(&"cargo test".to_string()) {
            info.frameworks.push("cargo test".to_string());
        }
    }

    // For Rust: check for inline #[test] or #[cfg(test)] in .rs files
    if path.join("Cargo.toml").exists() && !info.has_tests && has_rust_inline_tests(path) {
        info.has_tests = true;
        info.test_files_count = info.test_files_count.max(count_rs_test_files(path));
        if !info.frameworks.contains(&"cargo test".to_string()) {
            info.frameworks.push("cargo test".to_string());
        }
    }

    info
}

/// Check if any .rs file contains #[test] or #[cfg(test)] (scan src/ up to 50 files).
fn has_rust_inline_tests(path: &Path) -> bool {
    let src_dir = path.join("src");
    if !src_dir.is_dir() {
        return false;
    }
    let mut found = false;
    scan_rs_for_tests(&src_dir, &mut found, 0);
    found
}

fn scan_rs_for_tests(path: &Path, found: &mut bool, depth: usize) {
    if *found || depth > 5 {
        return;
    }
    let entries = match std::fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        if *found {
            return;
        }
        let p = entry.path();
        if p.is_dir() {
            scan_rs_for_tests(&p, found, depth + 1);
        } else if p.extension().is_some_and(|e| e == "rs") {
            if let Ok(content) = std::fs::read_to_string(&p) {
                if content.contains("#[test]") || content.contains("#[cfg(test)]") {
                    *found = true;
                    return;
                }
            }
        }
    }
}

/// Count .rs files that contain #[test] in src/.
fn count_rs_test_files(path: &Path) -> usize {
    let src_dir = path.join("src");
    if !src_dir.is_dir() {
        return 0;
    }
    let mut count = 0;
    count_rs_test_files_recursive(&src_dir, &mut count, 0);
    count
}

fn count_rs_test_files_recursive(path: &Path, count: &mut usize, depth: usize) {
    if depth > 5 {
        return;
    }
    let entries = match std::fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            count_rs_test_files_recursive(&p, count, depth + 1);
        } else if p.extension().is_some_and(|e| e == "rs") {
            if let Ok(content) = std::fs::read_to_string(&p) {
                if content.contains("#[test]") {
                    *count += 1;
                }
            }
        }
    }
}

fn count_files_recursive(path: &Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                count += 1;
            } else if p.is_dir() {
                count += count_files_recursive(&p);
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
    fn detects_tests_directory() {
        let dir = TempDir::new().unwrap();
        let test_dir = dir.path().join("tests");
        fs::create_dir_all(&test_dir).unwrap();
        fs::write(test_dir.join("test_main.py"), "def test_foo(): pass").unwrap();
        fs::write(test_dir.join("test_utils.py"), "def test_bar(): pass").unwrap();

        let info = detect_tests(dir.path());
        assert!(info.has_tests);
        assert_eq!(info.test_files_count, 2);
    }

    #[test]
    fn detects_jest_config() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("jest.config.js"), "module.exports = {}").unwrap();

        let info = detect_tests(dir.path());
        assert!(info.has_tests);
        assert!(info.frameworks.contains(&"Jest".to_string()));
    }

    #[test]
    fn detects_vitest_config() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("vitest.config.ts"), "export default {}").unwrap();

        let info = detect_tests(dir.path());
        assert!(info.has_tests);
        assert!(info.frameworks.contains(&"Vitest".to_string()));
    }

    #[test]
    fn detects_rust_cargo_tests() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"",
        )
        .unwrap();
        let test_dir = dir.path().join("tests");
        fs::create_dir_all(&test_dir).unwrap();
        fs::write(test_dir.join("integration.rs"), "#[test] fn it_works() {}").unwrap();

        let info = detect_tests(dir.path());
        assert!(info.has_tests);
        assert!(info.frameworks.contains(&"cargo test".to_string()));
    }

    #[test]
    fn no_tests_in_empty_dir() {
        let dir = TempDir::new().unwrap();
        let info = detect_tests(dir.path());
        assert!(!info.has_tests);
        assert_eq!(info.test_files_count, 0);
        assert!(info.frameworks.is_empty());
    }
}
