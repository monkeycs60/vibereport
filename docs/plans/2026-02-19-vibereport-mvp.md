# Vibereport MVP Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a Rust CLI that scans one or multiple git repos (local or remote GitHub), detects AI-generated commits (focused on Claude Code), computes project health stats, generates a fun "Vibe Score" with a roast tagline, and renders a beautiful terminal report with optional shareable link + global leaderboard.

**Architecture:** Pure Rust CLI using `gix` for git parsing, `ratatui` for terminal UI. No AI dependency. Three scan modes: single repo (default), multi-repo (`--scan-all`), remote GitHub repo (`github:user/repo`). The CLI is 100% local for the free tier. The `--share` flag uploads anonymized JSON to a Cloudflare Workers API backed by Turso (SQLite edge), which powers a public leaderboard on vibereport.dev (Astro static site on Cloudflare Pages). GitHub OAuth for user identity. The web frontend also allows scanning public GitHub repos directly via URL input.

**Tech Stack:**
- CLI: Rust (gix, ratatui, clap, serde, reqwest)
- Backend API: Cloudflare Workers (TypeScript)
- Database: Turso (libSQL/SQLite edge)
- Frontend: Astro + Tailwind on Cloudflare Pages
- Auth: GitHub OAuth

---

## Phase 1 — Core CLI (Day 1-2)

### Task 1: Project Setup

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `.gitignore`
- Create: `CLAUDE.md`

**Step 1: Initialize Rust project**

```bash
cd /home/clement/Desktop/vibereport
cargo init
```

**Step 2: Add dependencies to Cargo.toml**

```toml
[package]
name = "vibereport"
version = "0.1.0"
edition = "2021"
description = "The Spotify Wrapped for your code. How much of your repo is AI-generated?"
license = "MIT"

[dependencies]
clap = { version = "4", features = ["derive"] }
gix = { version = "0.72", default-features = false, features = ["max-performance-safe"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
ratatui = "0.29"
crossterm = "0.28"
owo-colors = "4"
reqwest = { version = "0.12", features = ["json", "blocking"], optional = true }
toml = "0.8"
chrono = { version = "0.4", features = ["serde"] }

[features]
default = ["share"]
share = ["reqwest"]

[dev-dependencies]
tempfile = "3"
assert_cmd = "2"
predicates = "3"
```

**Step 3: Create .gitignore**

```
/target
.env
```

**Step 4: Create CLAUDE.md with project conventions**

```markdown
# Vibereport

Rust CLI tool. "The Spotify Wrapped for your code."

## Conventions
- Use `thiserror` pattern for errors (enum VibereportError)
- All git operations go through `gix` crate, never shell out to `git`
- Module structure: git/, project/, score/, render/
- Tests: unit tests in same file (#[cfg(test)] mod tests), integration tests in tests/
- Run tests: `cargo test`
- Run lints: `cargo clippy -- -D warnings`
- Format: `cargo fmt`

## Architecture
- src/git/ — git log parsing, AI commit detection
- src/project/ — dependency counting, test detection, language stats
- src/score/ — composite score calculation, roast tagline selection
- src/render/ — terminal output (ratatui), SVG export, JSON export
- src/share/ — upload to vibereport.dev API (behind "share" feature flag)
- src/scanner/ — multi-repo discovery (--scan-all) + remote GitHub clone

## Scan Modes
1. Single repo (default): `vibereport` or `vibereport /path/to/repo`
2. Multi-repo: `vibereport --scan-all ~/projects` — finds all git repos recursively
3. Remote GitHub: `vibereport github:user/repo` — shallow clone + analyze
```

**Step 5: Create minimal main.rs to verify build**

```rust
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "vibereport", version, about = "The Spotify Wrapped for your code 🎯")]
struct Cli {
    /// Path to git repo, directory of repos, or github:user/repo
    #[arg(default_value = ".")]
    path: String,

    /// Scan all git repos found in the given directory
    #[arg(long)]
    scan_all: bool,

    /// Export report as SVG
    #[arg(long)]
    svg: Option<String>,

    /// Export report as JSON
    #[arg(long)]
    json: bool,

    /// Share report to vibereport.dev and get a public link
    #[arg(long)]
    share: bool,
}

fn main() {
    let cli = Cli::parse();
    println!("Scanning {}...", cli.path);
}
```

**Step 6: Build and verify**

Run: `cargo build`
Expected: Compiles successfully

**Step 7: Init git and commit**

```bash
cd /home/clement/Desktop/vibereport
git init
git add Cargo.toml src/main.rs .gitignore CLAUDE.md
git commit -m "feat: initial project setup with clap CLI skeleton"
```

---

### Task 2: Git Commit Parser + AI Detection

**Files:**
- Create: `src/git/mod.rs`
- Create: `src/git/parser.rs`
- Create: `src/git/ai_detect.rs`
- Modify: `src/main.rs` (add mod git)

**Step 1: Write failing test for AI detection**

Create `src/git/ai_detect.rs`:

```rust
/// Detects if a commit was AI-authored based on commit message patterns.
/// Focused on Claude Code, with experimental support for other tools.

#[derive(Debug, Clone, PartialEq)]
pub enum AiTool {
    ClaudeCode,
    GithubCopilot,
    CodexCli,
    Other(String),
    Human,
}

impl std::fmt::Display for AiTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AiTool::ClaudeCode => write!(f, "Claude Code"),
            AiTool::GithubCopilot => write!(f, "GitHub Copilot"),
            AiTool::CodexCli => write!(f, "Codex CLI"),
            AiTool::Other(name) => write!(f, "{}", name),
            AiTool::Human => write!(f, "Human"),
        }
    }
}

/// Analyze a commit message and return which AI tool authored it (if any).
pub fn detect_ai_tool(commit_message: &str) -> AiTool {
    let msg = commit_message.to_lowercase();

    // Claude Code patterns
    if msg.contains("co-authored-by: claude")
        || msg.contains("noreply@anthropic.com")
        || msg.contains("generated with claude code")
    {
        return AiTool::ClaudeCode;
    }

    // GitHub Copilot patterns
    if msg.contains("co-authored-by: copilot")
        || msg.contains("github-copilot")
        || msg.contains("noreply@github.com") && msg.contains("copilot")
    {
        return AiTool::GithubCopilot;
    }

    // Codex CLI
    if msg.contains("generated by codex") || msg.contains("codex-cli") {
        return AiTool::CodexCli;
    }

    AiTool::Human
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_claude_code_co_authored() {
        let msg = "feat: add login page\n\nCo-Authored-By: Claude <noreply@anthropic.com>";
        assert_eq!(detect_ai_tool(msg), AiTool::ClaudeCode);
    }

    #[test]
    fn detects_claude_code_opus() {
        let msg = "fix: resolve auth bug\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>";
        assert_eq!(detect_ai_tool(msg), AiTool::ClaudeCode);
    }

    #[test]
    fn detects_claude_code_generated_footer() {
        let msg = "refactor: clean up utils\n\nGenerated with Claude Code";
        assert_eq!(detect_ai_tool(msg), AiTool::ClaudeCode);
    }

    #[test]
    fn detects_human_commit() {
        let msg = "fix: typo in readme";
        assert_eq!(detect_ai_tool(msg), AiTool::Human);
    }

    #[test]
    fn detects_copilot() {
        let msg = "feat: add search\n\nCo-authored-by: copilot <noreply@github.com>";
        assert_eq!(detect_ai_tool(msg), AiTool::GithubCopilot);
    }
}
```

**Step 2: Run tests to verify they pass**

Run: `cargo test -- ai_detect`
Expected: All 5 tests PASS

**Step 3: Write git commit parser using gix**

Create `src/git/parser.rs`:

```rust
use gix::Repository;
use std::path::Path;
use super::ai_detect::{AiTool, detect_ai_tool};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub hash: String,
    pub message: String,
    pub author: String,
    pub timestamp: DateTime<Utc>,
    pub ai_tool: AiTool,
    pub lines_added: u64,
    pub lines_removed: u64,
}

#[derive(Debug)]
pub struct GitStats {
    pub total_commits: usize,
    pub ai_commits: usize,
    pub human_commits: usize,
    pub ai_ratio: f64,
    pub ai_tools: Vec<(AiTool, usize)>,
    pub commits: Vec<CommitInfo>,
    pub first_commit_date: Option<DateTime<Utc>>,
    pub last_commit_date: Option<DateTime<Utc>>,
}

/// Walk all commits in HEAD and classify each as AI or Human.
pub fn analyze_repo(path: &Path) -> Result<GitStats, Box<dyn std::error::Error>> {
    let repo = Repository::open(path)?;

    let head = repo.head_commit()?;
    let mut commits = Vec::new();

    // Walk all ancestors of HEAD
    for ancestor in head.ancestors().all()? {
        let ancestor = ancestor?;
        let commit = ancestor.object()?;
        let commit = commit.into_commit();
        let message = commit.message_raw_sloppy().to_string();
        let author = commit.author()?.name.to_string();

        let timestamp = {
            let time = commit.author()?.time;
            DateTime::from_timestamp(time.seconds, 0).unwrap_or_default()
        };

        let ai_tool = detect_ai_tool(&message);

        commits.push(CommitInfo {
            hash: commit.id().to_string()[..8].to_string(),
            message: message.lines().next().unwrap_or("").to_string(),
            author,
            timestamp,
            ai_tool,
            lines_added: 0, // TODO: compute from diff in v0.2
            lines_removed: 0,
        });
    }

    // Count AI tools
    let ai_commits = commits.iter().filter(|c| c.ai_tool != AiTool::Human).count();
    let human_commits = commits.len() - ai_commits;
    let ai_ratio = if commits.is_empty() { 0.0 } else { ai_commits as f64 / commits.len() as f64 };

    // Count by tool
    let mut tool_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for c in &commits {
        if c.ai_tool != AiTool::Human {
            *tool_counts.entry(c.ai_tool.to_string()).or_insert(0) += 1;
        }
    }
    let ai_tools: Vec<(AiTool, usize)> = commits
        .iter()
        .filter(|c| c.ai_tool != AiTool::Human)
        .fold(std::collections::HashMap::new(), |mut acc, c| {
            *acc.entry(c.ai_tool.clone()).or_insert(0) += 1;
            acc
        })
        .into_iter()
        .collect();

    let first_commit_date = commits.last().map(|c| c.timestamp);
    let last_commit_date = commits.first().map(|c| c.timestamp);

    Ok(GitStats {
        total_commits: commits.len(),
        ai_commits,
        human_commits,
        ai_ratio,
        ai_tools,
        commits,
        first_commit_date,
        last_commit_date,
    })
}
```

**Step 4: Create module file**

Create `src/git/mod.rs`:

```rust
pub mod ai_detect;
pub mod parser;
```

**Step 5: Wire into main.rs**

Update `src/main.rs` to add `mod git;` and call the parser:

```rust
mod git;

use clap::Parser;
use std::path::Path;

#[derive(Parser, Debug)]
#[command(name = "vibereport", version, about = "The Spotify Wrapped for your code 🎯")]
struct Cli {
    #[arg(default_value = ".")]
    path: String,

    #[arg(long)]
    svg: Option<String>,

    #[arg(long)]
    json: bool,

    #[arg(long)]
    share: bool,
}

fn main() {
    let cli = Cli::parse();
    let path = Path::new(&cli.path);

    println!("Scanning {}...", path.display());

    match git::parser::analyze_repo(path) {
        Ok(stats) => {
            println!("Total commits: {}", stats.total_commits);
            println!("AI commits: {} ({:.0}%)", stats.ai_commits, stats.ai_ratio * 100.0);
            println!("Human commits: {}", stats.human_commits);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}
```

**Step 6: Test on a real repo**

Run: `cd /home/clement/Desktop/vibereport && cargo run -- /home/clement/Desktop/affilae-mono`
Expected: Shows commit counts with AI detection working

**Step 7: Commit**

```bash
git add src/git/ src/main.rs
git commit -m "feat: git commit parser with Claude Code AI detection"
```

---

### Task 3: Project Analyzer (deps, tests, languages, LOC)

**Files:**
- Create: `src/project/mod.rs`
- Create: `src/project/deps.rs`
- Create: `src/project/tests_detect.rs`
- Create: `src/project/languages.rs`
- Create: `src/project/security.rs`

**Step 1: Write dependency counter**

Create `src/project/deps.rs`:

```rust
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
                let deps = parsed.get("dependencies")
                    .and_then(|d| d.as_object())
                    .map(|d| d.len())
                    .unwrap_or(0);
                let dev_deps = parsed.get("devDependencies")
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
                let deps = parsed.get("dependencies")
                    .and_then(|d| d.as_table())
                    .map(|d| d.len())
                    .unwrap_or(0);
                let dev_deps = parsed.get("dev-dependencies")
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
            let count = content.lines()
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
    use tempfile::TempDir;
    use std::fs;

    #[test]
    fn counts_npm_deps() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{
            "dependencies": { "react": "^18", "next": "^15" },
            "devDependencies": { "typescript": "^5" }
        }"#).unwrap();
        let info = count_deps(dir.path());
        assert_eq!(info.total, 3);
        assert_eq!(info.manager, "npm");
    }

    #[test]
    fn returns_default_for_empty_dir() {
        let dir = TempDir::new().unwrap();
        let info = count_deps(dir.path());
        assert_eq!(info.total, 0);
    }
}
```

**Step 2: Write test detector**

Create `src/project/tests_detect.rs`:

```rust
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
            info.frameworks.push(framework.to_string());
        }
    }

    // For Rust: check if tests/ dir exists or if src/ files contain #[cfg(test)]
    if path.join("Cargo.toml").exists() && path.join("tests").is_dir() {
        info.has_tests = true;
        info.frameworks.push("cargo test".to_string());
    }

    info
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
```

**Step 3: Write language/LOC counter**

Create `src/project/languages.rs`:

```rust
use std::path::Path;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct LanguageStats {
    /// Map of language name -> lines of code
    pub languages: HashMap<String, usize>,
    pub total_lines: usize,
}

/// Count lines of code by language by walking the source tree.
pub fn count_languages(path: &Path) -> LanguageStats {
    let mut stats = LanguageStats::default();
    walk_dir(path, &mut stats, path);
    stats
}

fn walk_dir(dir: &Path, stats: &mut LanguageStats, root: &Path) {
    let skip_dirs = ["node_modules", "target", ".git", "dist", "build", ".next",
                     "vendor", "__pycache__", ".venv", "venv", "coverage"];

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if path.is_dir() {
            if !skip_dirs.contains(&name.as_str()) && !name.starts_with('.') {
                walk_dir(&path, stats, root);
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
```

**Step 4: Write security checker**

Create `src/project/security.rs`:

```rust
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
    info.has_env_example = path.join(".env.example").exists() || path.join(".env.local.example").exists();

    info
}

fn is_gitignored(repo_path: &Path, file: &str) -> bool {
    let gitignore = repo_path.join(".gitignore");
    if let Ok(content) = std::fs::read_to_string(gitignore) {
        content.lines().any(|line| {
            let line = line.trim();
            line == file || line == &format!("/{}", file)
        })
    } else {
        false
    }
}
```

**Step 5: Create module file**

Create `src/project/mod.rs`:

```rust
pub mod deps;
pub mod tests_detect;
pub mod languages;
pub mod security;

use std::path::Path;

#[derive(Debug)]
pub struct ProjectStats {
    pub deps: deps::DepsInfo,
    pub tests: tests_detect::TestsInfo,
    pub languages: languages::LanguageStats,
    pub security: security::SecurityInfo,
}

pub fn analyze_project(path: &Path) -> ProjectStats {
    ProjectStats {
        deps: deps::count_deps(path),
        tests: tests_detect::detect_tests(path),
        languages: languages::count_languages(path),
        security: security::check_security(path),
    }
}
```

**Step 6: Run tests**

Run: `cargo test`
Expected: All tests pass

**Step 7: Commit**

```bash
git add src/project/
git commit -m "feat: project analyzer (deps, tests, languages, security)"
```

---

### Task 4: Score Calculator + Roast Taglines

**Files:**
- Create: `src/score/mod.rs`
- Create: `src/score/calculator.rs`
- Create: `src/score/roast.rs`

**Step 1: Write score calculator**

Create `src/score/calculator.rs`:

```rust
use crate::git::parser::GitStats;
use crate::project::ProjectStats;

#[derive(Debug, Clone)]
pub struct VibeScore {
    /// Letter grade A+ to F
    pub grade: String,
    /// Numeric score 0-100
    pub points: u32,
    /// Fun tagline
    pub roast: String,
    /// AI percentage (0.0 to 1.0)
    pub ai_ratio: f64,
}

/// Compute the Vibe Score based on git stats and project stats.
/// Higher score = more "vibe coded" (this is not a quality judgment,
/// it's a fun metric for how AI-assisted your project is).
pub fn calculate(git: &GitStats, project: &ProjectStats) -> VibeScore {
    let mut points: u32 = 0;

    // AI ratio is the primary factor (0-50 points)
    points += (git.ai_ratio * 50.0) as u32;

    // Dependencies boost (0-15 points) — more deps = more vibe
    let deps_score = (project.deps.total as f64 / 100.0).min(1.0) * 15.0;
    points += deps_score as u32;

    // No tests = more vibe (0-15 points)
    if !project.tests.has_tests {
        points += 15;
    } else if project.tests.test_files_count < 5 {
        points += 8;
    }

    // Large codebase with high AI ratio = impressive vibe (0-10 points)
    let size_factor = (project.languages.total_lines as f64 / 10000.0).min(1.0);
    points += (size_factor * 10.0) as u32;

    // Security issues = extra vibe points (0-10 points)
    if project.security.env_in_git {
        points += 10;
    }

    let points = points.min(100);
    let grade = match points {
        90..=100 => "S",
        80..=89 => "A+",
        70..=79 => "A",
        60..=69 => "B+",
        50..=59 => "B",
        40..=49 => "C+",
        30..=39 => "C",
        20..=29 => "D",
        _ => "F",
    }.to_string();

    let roast = super::roast::pick_roast(points, git.ai_ratio, project);

    VibeScore {
        grade,
        points,
        roast,
        ai_ratio: git.ai_ratio,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::ai_detect::AiTool;

    fn mock_git_stats(ai_ratio: f64) -> GitStats {
        GitStats {
            total_commits: 100,
            ai_commits: (100.0 * ai_ratio) as usize,
            human_commits: (100.0 * (1.0 - ai_ratio)) as usize,
            ai_ratio,
            ai_tools: vec![(AiTool::ClaudeCode, (100.0 * ai_ratio) as usize)],
            commits: vec![],
            first_commit_date: None,
            last_commit_date: None,
        }
    }

    fn mock_project_stats(deps: usize, has_tests: bool) -> ProjectStats {
        ProjectStats {
            deps: crate::project::deps::DepsInfo { total: deps, manager: "npm".into() },
            tests: crate::project::tests_detect::TestsInfo {
                has_tests,
                test_files_count: if has_tests { 10 } else { 0 },
                frameworks: vec![],
            },
            languages: crate::project::languages::LanguageStats {
                languages: std::collections::HashMap::new(),
                total_lines: 5000,
            },
            security: crate::project::security::SecurityInfo::default(),
        }
    }

    #[test]
    fn high_ai_no_tests_high_score() {
        let git = mock_git_stats(0.9);
        let proj = mock_project_stats(200, false);
        let score = calculate(&git, &proj);
        assert!(score.points >= 70, "Expected high score, got {}", score.points);
    }

    #[test]
    fn zero_ai_with_tests_low_score() {
        let git = mock_git_stats(0.0);
        let proj = mock_project_stats(5, true);
        let score = calculate(&git, &proj);
        assert!(score.points <= 30, "Expected low score, got {}", score.points);
    }
}
```

**Step 2: Write roast tagline picker**

Create `src/score/roast.rs`:

```rust
use crate::project::ProjectStats;

/// Pick a fun roast tagline based on the score and project characteristics.
pub fn pick_roast(points: u32, ai_ratio: f64, project: &ProjectStats) -> String {
    // Special case roasts (checked first)
    if ai_ratio > 0.95 {
        return "You're the project manager now.".to_string();
    }
    if ai_ratio > 0.9 && !project.tests.has_tests {
        return "Ships fast, tests never.".to_string();
    }
    if ai_ratio == 0.0 {
        return "Write code like it's 2019.".to_string();
    }
    if project.deps.total > 500 {
        return "node_modules is the real project.".to_string();
    }
    if !project.tests.has_tests && project.languages.total_lines > 10000 {
        return "10K lines of YOLO.".to_string();
    }
    if project.security.env_in_git {
        return "Secrets? What secrets?".to_string();
    }

    // Score-based roasts
    match points {
        90..=100 => "The AI is the senior dev here.",
        80..=89  => "You prompt, Claude delivers.",
        70..=79  => "More vibes than version control.",
        60..=69  => "Solid vibe-to-code ratio.",
        50..=59  => "Half human, half machine.",
        40..=49  => "Training wheels still on.",
        30..=39  => "Mostly artisanal, free-range code.",
        20..=29  => "You actually read the docs?",
        _        => "Handcrafted with mass-produced tears.",
    }.to_string()
}
```

**Step 3: Create module file**

Create `src/score/mod.rs`:

```rust
pub mod calculator;
pub mod roast;
```

**Step 4: Run tests**

Run: `cargo test`
Expected: All tests pass

**Step 5: Commit**

```bash
git add src/score/
git commit -m "feat: vibe score calculator with roast taglines"
```

---

### Task 5: Beautiful Terminal Renderer

**Files:**
- Create: `src/render/mod.rs`
- Create: `src/render/terminal.rs`
- Modify: `src/main.rs`

**Step 1: Write terminal renderer**

Create `src/render/terminal.rs`:

```rust
use crate::git::parser::GitStats;
use crate::project::ProjectStats;
use crate::score::calculator::VibeScore;
use owo_colors::OwoColorize;

/// Render the full vibe report to the terminal using box-drawing characters.
pub fn render(git: &GitStats, project: &ProjectStats, score: &VibeScore) {
    let width = 48;
    let border_top = format!("╭{}╮", "─".repeat(width));
    let border_bot = format!("╰{}╯", "─".repeat(width));
    let empty_line = format!("│{}│", " ".repeat(width));

    println!();
    println!("  {}", border_top.cyan());
    println!("  {}", empty_line.cyan());

    // Title
    let title = " VIBE REPORT";
    println!("  {} {}{}{}", "│".cyan(), "🎯".bold(), title.bold(), pad(width - title.len() - 3, "│").cyan());
    println!("  {}", empty_line.cyan());

    // AI Ratio
    let ai_pct = format!("{:.0}%", score.ai_ratio * 100.0);
    print_line("🤖 AI-authored", &ai_pct, width);
    print_line("👤 Human-authored", &format!("{:.0}%", (1.0 - score.ai_ratio) * 100.0), width);
    print_line("📝 Total commits", &git.total_commits.to_string(), width);

    println!("  {}", empty_line.cyan());

    // AI tools breakdown
    for (tool, count) in &git.ai_tools {
        let pct = (*count as f64 / git.total_commits.max(1) as f64) * 100.0;
        print_line(&format!("   {}", tool), &format!("{} ({:.0}%)", count, pct), width);
    }

    println!("  {}", empty_line.cyan());

    // Project stats
    if project.deps.total > 0 {
        print_line("📦 Dependencies", &format!("{} ({})", project.deps.total, project.deps.manager), width);
    }

    let test_str = if project.tests.has_tests {
        format!("{} files", project.tests.test_files_count)
    } else {
        "0  💀".to_string()
    };
    print_line("🧪 Tests", &test_str, width);

    print_line("📏 Lines of code", &format_number(project.languages.total_lines), width);

    // Top languages
    let mut langs: Vec<_> = project.languages.languages.iter().collect();
    langs.sort_by(|a, b| b.1.cmp(a.1));
    for (lang, lines) in langs.iter().take(3) {
        let pct = (**lines as f64 / project.languages.total_lines.max(1) as f64) * 100.0;
        print_line(&format!("   {}", lang), &format!("{:.0}%", pct), width);
    }

    // Security
    if project.security.env_in_git {
        println!("  {}", empty_line.cyan());
        print_line("🔒 .env in git", "yes (yikes)", width);
    }

    println!("  {}", empty_line.cyan());

    // Score
    let score_display = format!("VIBE SCORE: {} ({})", score.grade, score.points);
    println!("  {} {}{}", "│".cyan(), score_display.bold().yellow(), pad(width - score_display.len() - 1, "│").cyan());

    let roast_display = format!("\"{}\"", score.roast);
    println!("  {} {}{}", "│".cyan(), roast_display.italic().dimmed(), pad(width - roast_display.len() - 1, "│").cyan());

    println!("  {}", empty_line.cyan());
    println!("  {}", border_bot.cyan());
    println!();
}

fn print_line(label: &str, value: &str, width: usize) {
    let content = format!(" {}  {}", label, value);
    let padding = width.saturating_sub(content.len());
    println!("  {} {}{}{}", "│".cyan(), label.dimmed(), format!("  {}", value).white().bold(), pad(padding.saturating_sub(label.len() + value.len()), "│").cyan());
}

fn pad(n: usize, end: &str) -> String {
    format!("{}{}", " ".repeat(n.min(200)), end)
}

fn format_number(n: usize) -> String {
    if n >= 1000 {
        format!("{:.1}K", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}
```

Note: The terminal rendering will need iteration to get pixel-perfect alignment. The code above is a starting point — when vibe coding with Claude Code, iterate on the layout by running `cargo run` repeatedly and tweaking until the output looks beautiful.

**Step 2: Create module file**

Create `src/render/mod.rs`:

```rust
pub mod terminal;
```

**Step 3: Wire everything in main.rs**

Update `src/main.rs`:

```rust
mod git;
mod project;
mod score;
mod render;

use clap::Parser;
use std::path::Path;

#[derive(Parser, Debug)]
#[command(name = "vibereport", version, about = "The Spotify Wrapped for your code 🎯")]
struct Cli {
    /// Path to git repo, directory of repos, or github:user/repo
    #[arg(default_value = ".")]
    path: String,

    /// Scan all git repos found in the given directory
    #[arg(long)]
    scan_all: bool,

    #[arg(long)]
    svg: Option<String>,

    #[arg(long)]
    json: bool,

    #[arg(long)]
    share: bool,
}

fn main() {
    let cli = Cli::parse();
    let path = Path::new(&cli.path);

    eprintln!("🔍 Scanning {}...", path.display());

    let git_stats = match git::parser::analyze_repo(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading git repo: {}", e);
            std::process::exit(1);
        }
    };

    let project_stats = project::analyze_project(path);
    let vibe_score = score::calculator::calculate(&git_stats, &project_stats);

    if cli.json {
        // TODO: JSON output
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "ai_ratio": vibe_score.ai_ratio,
            "score": vibe_score.points,
            "grade": vibe_score.grade,
            "roast": vibe_score.roast,
            "total_commits": git_stats.total_commits,
            "ai_commits": git_stats.ai_commits,
            "deps": project_stats.deps.total,
            "has_tests": project_stats.tests.has_tests,
            "total_lines": project_stats.languages.total_lines,
        })).unwrap());
    } else {
        render::terminal::render(&git_stats, &project_stats, &vibe_score);
    }
}
```

**Step 4: Build, run on a real repo, iterate on visuals**

Run: `cargo run -- /home/clement/Desktop/affilae-mono`
Expected: Beautiful boxed output with vibe score

**Step 5: Commit**

```bash
git add src/render/ src/main.rs
git commit -m "feat: beautiful terminal renderer with vibe score display"
```

---

### Task 6: SVG Export

**Files:**
- Create: `src/render/svg.rs`
- Modify: `src/render/mod.rs`
- Modify: `src/main.rs`

**Step 1: Write SVG renderer**

Create `src/render/svg.rs`:

Write a function `pub fn render_svg(git: &GitStats, project: &ProjectStats, score: &VibeScore) -> String` that generates an SVG string mimicking the terminal output. Use a dark background (#1a1b26), monospace font, and colored text. The SVG should be approximately 600x400px and look good when shared on social media.

Key approach: build the SVG as a raw string with `format!()` — no need for a crate. Use `<rect>` for background, `<text>` for each line, position with y-offsets.

**Step 2: Wire into main.rs**

If `cli.svg` is Some(path), write the SVG string to that file path.

**Step 3: Test**

Run: `cargo run -- /path/to/repo --svg report.svg`
Then open report.svg in a browser to verify it looks good.

**Step 4: Commit**

```bash
git add src/render/svg.rs src/render/mod.rs src/main.rs
git commit -m "feat: SVG export for shareable vibe reports"
```

---

### Task 7: Multi-Repo Scanner (--scan-all)

**Files:**
- Create: `src/scanner/mod.rs`
- Create: `src/scanner/discover.rs`
- Create: `src/scanner/multi_report.rs`
- Modify: `src/main.rs`

**Step 1: Write repo discovery function**

Create `src/scanner/discover.rs`:

```rust
use std::path::{Path, PathBuf};

/// Recursively find all directories containing a .git folder.
/// Stops descending into a directory once a .git is found (doesn't look for nested repos).
/// Skips: node_modules, target, .git, vendor, dist, build
pub fn find_git_repos(root: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut repos = Vec::new();
    walk_for_repos(root, &mut repos, 0, max_depth);
    repos
}

fn walk_for_repos(dir: &Path, repos: &mut Vec<PathBuf>, depth: usize, max_depth: usize) {
    if depth > max_depth { return; }

    let skip = ["node_modules", "target", ".git", "vendor", "dist", "build", ".next"];

    if dir.join(".git").is_dir() {
        repos.push(dir.to_path_buf());
        return; // Don't descend into nested repos
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !skip.contains(&name.as_str()) && !name.starts_with('.') {
                walk_for_repos(&path, repos, depth + 1, max_depth);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[test]
    fn finds_git_repos() {
        let root = TempDir::new().unwrap();
        let repo_a = root.path().join("project-a");
        let repo_b = root.path().join("project-b");
        fs::create_dir_all(repo_a.join(".git")).unwrap();
        fs::create_dir_all(repo_b.join(".git")).unwrap();
        fs::create_dir_all(root.path().join("not-a-repo")).unwrap();

        let repos = find_git_repos(root.path(), 3);
        assert_eq!(repos.len(), 2);
    }
}
```

**Step 2: Write multi-repo report aggregator**

Create `src/scanner/multi_report.rs`:

```rust
use crate::git::parser::GitStats;
use crate::project::ProjectStats;
use crate::score::calculator::VibeScore;
use std::path::PathBuf;

#[derive(Debug)]
pub struct RepoReport {
    pub path: PathBuf,
    pub name: String,
    pub git_stats: GitStats,
    pub project_stats: ProjectStats,
    pub score: VibeScore,
}

#[derive(Debug)]
pub struct MultiReport {
    pub repos: Vec<RepoReport>,
    pub total_commits: usize,
    pub total_ai_commits: usize,
    pub global_ai_ratio: f64,
    pub total_lines: usize,
    pub average_score: u32,
}

pub fn aggregate(repos: Vec<RepoReport>) -> MultiReport {
    let total_commits: usize = repos.iter().map(|r| r.git_stats.total_commits).sum();
    let total_ai_commits: usize = repos.iter().map(|r| r.git_stats.ai_commits).sum();
    let global_ai_ratio = if total_commits > 0 {
        total_ai_commits as f64 / total_commits as f64
    } else { 0.0 };
    let total_lines: usize = repos.iter().map(|r| r.project_stats.languages.total_lines).sum();
    let average_score = if repos.is_empty() { 0 } else {
        repos.iter().map(|r| r.score.points as usize).sum::<usize>() as u32 / repos.len() as u32
    };

    MultiReport { repos, total_commits, total_ai_commits, global_ai_ratio, total_lines, average_score }
}
```

**Step 3: Create module file**

Create `src/scanner/mod.rs`:

```rust
pub mod discover;
pub mod multi_report;
```

**Step 4: Wire --scan-all into main.rs**

In main.rs, when `cli.scan_all` is true:
1. Call `scanner::discover::find_git_repos(&path, 3)`
2. For each repo found, run `git::parser::analyze_repo` + `project::analyze_project` + `score::calculator::calculate`
3. Aggregate with `scanner::multi_report::aggregate`
4. Render a multi-repo table view in terminal (new function in `render/terminal.rs`)

The multi-repo terminal output should look like:
```
  🎯 YOUR DEV LIFE — Vibe Report (23 repos)

  REPO                 AI%    SCORE   ROAST
  saas-app/            91%    A+      "The AI is the senior dev"
  portfolio/           73%    B+      "Ships fast, tests never"
  cli-tool/            12%    D       "Handcrafted artisan code"
  ...

  GLOBAL: 47% AI | 34,521 lines | Avg Score: B (58)
  "Half your brain is Claude at this point."
```

**Step 5: Test on ~/Desktop**

Run: `cargo run -- --scan-all /home/clement/Desktop`
Expected: Finds multiple repos, shows table with per-repo scores

**Step 6: Commit**

```bash
git add src/scanner/ src/main.rs src/render/terminal.rs
git commit -m "feat: --scan-all mode for multi-repo vibe reports"
```

---

### Task 8: Remote GitHub Repo Analysis

**Files:**
- Create: `src/scanner/remote.rs`
- Modify: `src/scanner/mod.rs`
- Modify: `src/main.rs`

**Step 1: Write remote clone + analyze function**

Create `src/scanner/remote.rs`:

```rust
use std::path::PathBuf;
use std::process::Command;

/// Parse "github:user/repo" format and return (user, repo).
pub fn parse_github_ref(input: &str) -> Option<(String, String)> {
    let stripped = input
        .strip_prefix("github:")
        .or_else(|| input.strip_prefix("https://github.com/"))
        .or_else(|| input.strip_prefix("github.com/"))?;
    let parts: Vec<&str> = stripped.trim_end_matches('/').splitn(2, '/').collect();
    if parts.len() == 2 {
        Some((parts[0].to_string(), parts[1].to_string()))
    } else {
        None
    }
}

/// Shallow-clone a GitHub repo into a temp directory for analysis.
/// Uses --bare --filter=blob:none to download only commit history, not file content.
/// For project analysis (deps, tests, LOC), does a sparse checkout of root config files.
pub fn clone_for_analysis(user: &str, repo: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let tmp_dir = std::env::temp_dir().join(format!("vibereport-{}-{}", user, repo));

    // Clean up previous clone if exists
    if tmp_dir.exists() {
        std::fs::remove_dir_all(&tmp_dir)?;
    }

    // Shallow clone with full tree (needed for project analysis)
    let status = Command::new("git")
        .args([
            "clone",
            "--depth", "500",
            &format!("https://github.com/{}/{}.git", user, repo),
            &tmp_dir.to_string_lossy(),
        ])
        .status()?;

    if !status.success() {
        return Err(format!("Failed to clone {}/{}", user, repo).into());
    }

    Ok(tmp_dir)
}

/// Clean up the temp directory after analysis.
pub fn cleanup(path: &PathBuf) {
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
    fn returns_none_for_invalid() {
        assert_eq!(parse_github_ref("/some/local/path"), None);
    }
}
```

**Step 2: Wire into main.rs**

In main.rs, before the existing logic, check if `cli.path` matches a GitHub ref:
```rust
// Check if path is a GitHub reference
if let Some((user, repo)) = scanner::remote::parse_github_ref(&cli.path) {
    eprintln!("🔍 Cloning {}/{}...", user, repo);
    let tmp_path = scanner::remote::clone_for_analysis(&user, &repo)?;
    // ... run analysis on tmp_path ...
    // ... render report ...
    scanner::remote::cleanup(&tmp_path);
    return;
}
```

**Step 3: Test on a public repo**

Run: `cargo run -- github:anthropics/claude-code`
Expected: Clones repo, analyzes, shows vibe report

**Step 4: Commit**

```bash
git add src/scanner/remote.rs src/scanner/mod.rs src/main.rs
git commit -m "feat: remote GitHub repo analysis via github:user/repo"
```

---

## Phase 2 — Share + Leaderboard (Day 3-4)

### Task 9: Backend API + Leaderboard (Cloudflare Workers)

**Files:**
- Create: `web/api/` directory (separate project)
- Create: `web/api/src/index.ts`
- Create: `web/api/wrangler.toml`

**Step 1: Init Cloudflare Workers project**

```bash
cd /home/clement/Desktop/vibereport
mkdir -p web/api
cd web/api
npm init -y
npm install wrangler @libsql/client
```

**Step 2: Create the API with these endpoints:**

```
POST /api/reports       — Submit a new report (returns report ID + URL + rank)
GET  /api/reports/:id   — Get a single report
GET  /api/leaderboard   — Top scores, paginated, filterable (?lang=typescript&period=week)
GET  /api/stats         — Aggregate stats (avg AI ratio, total reports, trends)
GET  /api/badge/:id.svg — Dynamic SVG badge for README embed
POST /api/scan          — Web scan: accepts {github_url}, clones + analyzes server-side, returns report
```

Database schema (Turso/SQLite):

```sql
CREATE TABLE reports (
  id TEXT PRIMARY KEY,
  github_username TEXT,
  repo_name TEXT,           -- NULL if anonymous
  ai_ratio REAL NOT NULL,
  ai_tool TEXT,
  score_points INTEGER NOT NULL,
  score_grade TEXT NOT NULL,
  roast TEXT NOT NULL,
  deps_count INTEGER,
  has_tests BOOLEAN,
  total_lines INTEGER,
  languages TEXT,           -- JSON string
  created_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX idx_score ON reports(score_points DESC);
CREATE INDEX idx_ai_ratio ON reports(ai_ratio DESC);
CREATE INDEX idx_created ON reports(created_at DESC);
```

**Step 3: Deploy**

```bash
npx wrangler deploy
```

**Step 4: Commit**

```bash
git add web/api/
git commit -m "feat: Cloudflare Workers API for report sharing + leaderboard"
```

---

### Task 10: CLI --share Integration

**Files:**
- Create: `src/share/mod.rs`
- Create: `src/share/upload.rs`
- Modify: `src/main.rs`

**Step 1: Write upload function**

Create `src/share/upload.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct ReportPayload {
    pub github_username: Option<String>,
    pub repo_name: Option<String>,
    pub ai_ratio: f64,
    pub ai_tool: String,
    pub score_points: u32,
    pub score_grade: String,
    pub roast: String,
    pub deps_count: usize,
    pub has_tests: bool,
    pub total_lines: usize,
    pub languages: String,
}

#[derive(Deserialize)]
pub struct ShareResponse {
    pub id: String,
    pub url: String,
    pub rank: Option<u64>,
    pub percentile: Option<f64>,
}

const API_URL: &str = "https://api.vibereport.dev";

pub fn upload_report(payload: &ReportPayload) -> Result<ShareResponse, Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!("{}/api/reports", API_URL))
        .json(payload)
        .send()?
        .json::<ShareResponse>()?;
    Ok(resp)
}
```

**Step 2: Wire into main.rs**

When `cli.share` is true, after rendering terminal output, build the payload from stats and call `upload_report()`. Print the share URL and leaderboard rank.

```
  🔗 Shared! vibereport.dev/r/abc123
  🏆 Rank #847 — More AI-dependent than 73% of devs
```

**Step 3: Commit**

```bash
git add src/share/ src/main.rs
git commit -m "feat: --share uploads report and returns leaderboard rank"
```

---

### Task 11: Frontend Web + Remote Scan (Astro)

**Files:**
- Create: `web/frontend/` directory (separate Astro project)

**Step 1: Init Astro project**

```bash
cd /home/clement/Desktop/vibereport/web
npm create astro@latest frontend -- --template minimal
cd frontend
npm install tailwindcss @astrojs/tailwind
```

**Step 2: Create pages:**

```
/              — Landing page with:
                 - Global stats (average AI ratio across all reports)
                 - "Scan a GitHub repo" input field (enter URL → calls API → shows result)
                 - Top 10 leaderboard preview
/r/[id]        — Individual report view (beautiful card, OG meta tags for social sharing)
/leaderboard   — Full leaderboard with filters (by language, framework, time period)
/trends        — Global trends dashboard (AI ratio over time, by language, by tool)
/scan          — Web-based repo scanner: enter any public GitHub URL → get instant vibe report
```

Key points:
- The `/r/[id]` page MUST have proper Open Graph meta tags so when shared on Twitter/Reddit, a beautiful preview card is generated
- The `/scan` page calls a backend endpoint that does `git clone --depth 500` + analysis server-side (Cloudflare Worker or a small VPS for heavier work)
- The landing page "Scan a repo" input is the #1 conversion tool: zero friction, no install needed

**Step 3: Deploy to Cloudflare Pages**

```bash
npx wrangler pages deploy dist/
```

**Step 4: Commit**

```bash
git add web/frontend/
git commit -m "feat: web frontend with report pages, leaderboard, trends"
```

---

### Task 12: README + GitHub Release

**Files:**
- Create: `README.md`
- Create: `.github/workflows/release.yml`

**Step 1: Write README with beautiful screenshot**

Include: GIF of running vibereport in terminal, installation instructions, usage examples, link to vibereport.dev.

**Step 2: Create GitHub Actions release workflow**

Use `cross` for cross-compilation (Linux x86_64, macOS arm64, macOS x86_64, Windows). Auto-publish binaries on git tag.

**Step 3: Commit and tag v0.1.0**

```bash
git add README.md .github/
git commit -m "feat: README and automated release workflow"
git tag v0.1.0
git push origin main --tags
```

---

## Summary

| Task | What | Phase | Time (vibe coding) |
|------|------|-------|---------------------|
| 1 | Project setup + skeleton | Core CLI | 15 min |
| 2 | Git parser + AI detection | Core CLI | 1-2h |
| 3 | Project analyzer (deps/tests/LOC) | Core CLI | 1-2h |
| 4 | Score calculator + roasts | Core CLI | 30 min |
| 5 | Terminal renderer (beautiful output) | Core CLI | 1-2h |
| 6 | SVG export | Core CLI | 30 min-1h |
| 7 | **Multi-repo scanner (--scan-all)** | Core CLI | 1h |
| 8 | **Remote GitHub repo analysis** | Core CLI | 1h |
| 9 | Backend API + leaderboard | Share | 1-2h |
| 10 | CLI --share integration | Share | 30 min |
| 11 | Frontend web + web scan + OG cards | Share | 2-3h |
| 12 | README + GitHub Release | Ship | 30 min |
| **Total** | | | **~10-16h (1 weekend)** |

## Virality Checklist

- [ ] Beautiful terminal output → screenshot-worthy
- [ ] `--scan-all` → "my entire dev life" report → shareable
- [ ] `github:user/repo` → scan ANY public repo → content creation ("Top 100 repos ranked")
- [ ] `--share` → vibereport.dev/r/abc123 → social sharing with OG preview cards
- [ ] Leaderboard → competition → repeat visits
- [ ] Web scan (vibereport.dev/scan) → zero friction, no install → maximum reach
- [ ] SVG badge for README → permanent visibility
- [ ] Roast taglines → funny → people share for the humor
- [ ] Aggregate trends → data journalism → content marketing flywheel
