use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Default)]
pub struct LanguageStats {
    /// Map of language name -> lines of code
    pub languages: HashMap<String, usize>,
    pub total_lines: usize,
}

/// Count lines of code by language by walking the source tree.
pub fn count_languages(path: &Path) -> LanguageStats {
    let mut stats = LanguageStats::default();
    walk_dir(path, &mut stats);
    stats
}

fn walk_dir(dir: &Path, stats: &mut LanguageStats) {
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
        "coverage",
    ];

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if path.is_dir() {
            if !skip_dirs.contains(&name.as_str()) && !name.starts_with('.') {
                walk_dir(&path, stats);
            }
        } else if path.is_file() {
            if let Some(lang) = detect_language(&name) {
                let lines = count_lines(&path);
                *stats.languages.entry(lang).or_insert(0) += lines;
                stats.total_lines += lines;
            }
        }
    }
}

fn detect_language(filename: &str) -> Option<String> {
    let ext = filename.rsplit('.').next()?;
    match ext {
        "ts" | "tsx" => Some("TypeScript".to_string()),
        "js" | "jsx" | "mjs" | "cjs" => Some("JavaScript".to_string()),
        "rs" => Some("Rust".to_string()),
        "py" => Some("Python".to_string()),
        "go" => Some("Go".to_string()),
        "rb" => Some("Ruby".to_string()),
        "java" => Some("Java".to_string()),
        "css" | "scss" | "sass" => Some("CSS".to_string()),
        "html" | "htm" => Some("HTML".to_string()),
        "svelte" => Some("Svelte".to_string()),
        "vue" => Some("Vue".to_string()),
        "php" => Some("PHP".to_string()),
        "swift" => Some("Swift".to_string()),
        "kt" => Some("Kotlin".to_string()),
        "c" | "h" => Some("C".to_string()),
        "cpp" | "cc" | "hpp" => Some("C++".to_string()),
        "cs" => Some("C#".to_string()),
        _ => None,
    }
}

fn count_lines(path: &Path) -> usize {
    std::fs::read_to_string(path)
        .map(|content| content.lines().count())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn counts_rust_lines() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("main.rs"), "fn main() {\n    println!(\"hello\");\n}\n").unwrap();
        fs::write(src.join("lib.rs"), "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n").unwrap();

        let stats = count_languages(dir.path());
        assert_eq!(stats.languages.get("Rust"), Some(&6));
        assert_eq!(stats.total_lines, 6);
    }

    #[test]
    fn counts_multiple_languages() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("app.ts"), "const x = 1;\nconst y = 2;\n").unwrap();
        fs::write(dir.path().join("style.css"), "body { margin: 0; }\n").unwrap();

        let stats = count_languages(dir.path());
        assert_eq!(stats.languages.get("TypeScript"), Some(&2));
        assert_eq!(stats.languages.get("CSS"), Some(&1));
        assert_eq!(stats.total_lines, 3);
    }

    #[test]
    fn skips_node_modules() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("index.js"), "console.log('hello');\n").unwrap();
        let nm = dir.path().join("node_modules").join("pkg");
        fs::create_dir_all(&nm).unwrap();
        fs::write(nm.join("index.js"), "// lots\n// of\n// code\n// here\n").unwrap();

        let stats = count_languages(dir.path());
        assert_eq!(stats.languages.get("JavaScript"), Some(&1));
        assert_eq!(stats.total_lines, 1);
    }

    #[test]
    fn skips_target_dir() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}\n").unwrap();
        let target = dir.path().join("target").join("debug");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("build.rs"), "// generated\n// code\n").unwrap();

        let stats = count_languages(dir.path());
        assert_eq!(stats.languages.get("Rust"), Some(&1));
        assert_eq!(stats.total_lines, 1);
    }

    #[test]
    fn empty_dir_returns_empty_stats() {
        let dir = TempDir::new().unwrap();
        let stats = count_languages(dir.path());
        assert!(stats.languages.is_empty());
        assert_eq!(stats.total_lines, 0);
    }

    #[test]
    fn detects_language_from_extension() {
        assert_eq!(detect_language("app.tsx"), Some("TypeScript".to_string()));
        assert_eq!(detect_language("main.py"), Some("Python".to_string()));
        assert_eq!(detect_language("server.go"), Some("Go".to_string()));
        assert_eq!(detect_language("readme.md"), None);
        assert_eq!(detect_language("Makefile"), None);
    }
}
