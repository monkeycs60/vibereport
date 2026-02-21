# Vibereport UX Redesign + VPS Scan Worker — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Redesign scoring (AI% + Vibe Score), add tug-of-war homepage, new vibe detectors, VPS scan worker, and contextual roasts.

**Architecture:** CLI gets new detectors feeding into a redesigned composite Vibe Score. Frontend replaces BattleChart with tug-of-war and shows chaos badges. A new Axum-based VPS worker handles web scans via `git clone` instead of GitHub API. Cloudflare Worker proxies to VPS with GitHub API fallback.

**Tech Stack:** Rust (clap, gix, axum, tokio), Astro/Tailwind frontend, Cloudflare Workers (Hono), D1 SQLite, nginx on OVH VPS.

---

## TRACK A: CLI Scoring + Detectors (Rust)

These tasks modify `src/project/`, `src/score/`, `src/main.rs`, `src/share/upload.rs`.

### Task A1: Add vibe detectors module

**Files:**
- Create: `src/project/vibe_detect.rs`
- Modify: `src/project/mod.rs:1-23`

**Step 1: Create `src/project/vibe_detect.rs`**

```rust
use std::path::Path;

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
    ".eslintrc", ".eslintrc.js", ".eslintrc.cjs", ".eslintrc.json", ".eslintrc.yml",
    "eslint.config.js", "eslint.config.mjs", "eslint.config.ts",
    ".prettierrc", ".prettierrc.js", ".prettierrc.json", ".prettierrc.yml",
    "prettier.config.js", "prettier.config.mjs",
    "biome.json", "biome.jsonc",
    "deno.json", "deno.jsonc",
    ".oxlintrc.json",
    "rustfmt.toml", ".rustfmt.toml",
    ".rubocop.yml",
    "pylintrc", ".pylintrc", ".flake8", "pyproject.toml",  // pyproject checked for [tool.ruff] later
    ".golangci.yml", ".golangci.yaml",
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
    let mut info = VibeInfo::default();

    // No linting
    info.no_linting = !LINT_CONFIGS.iter().any(|f| path.join(f).exists());

    // No CI/CD
    info.no_ci_cd = !CI_CONFIGS.iter().any(|f| path.join(f).exists());

    // Boomer AI: uses AI but has no config
    if ai_ratio > 0.0 {
        info.boomer_ai = !AI_CONFIGS.iter().any(|f| path.join(f).exists());
    }

    // node_modules in git
    info.node_modules_in_git = path.join("node_modules").is_dir()
        && path.join("node_modules").join("package.json").exists(); // heuristic: if node_modules has content, it's tracked

    // No .gitignore
    let gitignore_path = path.join(".gitignore");
    if !gitignore_path.exists() {
        info.no_gitignore = true;
    } else if let Ok(content) = std::fs::read_to_string(&gitignore_path) {
        let non_empty_lines = content.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).count();
        info.no_gitignore = non_empty_lines < 3;
    }

    // No README
    info.no_readme = !path.join("README.md").exists()
        && !path.join("readme.md").exists()
        && !path.join("README").exists()
        && !path.join("README.rst").exists();

    // TODO flood
    info.todo_count = count_todos(path);
    info.todo_flood = info.todo_count > 20;

    // Single branch — check via gix
    info.single_branch = check_single_branch(path);

    info
}

fn count_todos(path: &Path) -> usize {
    let mut count = 0;
    let skip_dirs = ["node_modules", "target", ".git", "dist", "build", ".next", "vendor", "__pycache__", ".venv", "venv"];
    count_todos_recursive(path, &skip_dirs, &mut count, 0);
    count
}

fn count_todos_recursive(path: &Path, skip_dirs: &[&str], count: &mut usize, depth: usize) {
    if depth > 10 || *count > 100 { return; } // early exit
    let entries = match std::fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if p.is_dir() {
            if !skip_dirs.contains(&name.as_str()) {
                count_todos_recursive(&p, skip_dirs, count, depth + 1);
            }
        } else if p.is_file() {
            if let Some(ext) = p.extension() {
                let ext = ext.to_string_lossy();
                if matches!(ext.as_ref(), "rs" | "ts" | "js" | "py" | "go" | "rb" | "java" | "tsx" | "jsx" | "vue" | "svelte" | "php" | "swift" | "kt" | "c" | "cpp" | "cs" | "h") {
                    if let Ok(content) = std::fs::read_to_string(&p) {
                        for line in content.lines() {
                            let upper = line.to_uppercase();
                            if upper.contains("TODO") || upper.contains("FIXME") || upper.contains("HACK") {
                                *count += 1;
                            }
                        }
                    }
                }
            }
        }
    }
}

fn check_single_branch(path: &Path) -> bool {
    let repo = match gix::open(path) {
        Ok(r) => r,
        Err(_) => return false,
    };
    let refs = repo.references();
    match refs {
        Ok(r) => {
            let branch_count = r.local_branches()
                .map(|iter| iter.count())
                .unwrap_or(0);
            branch_count <= 1
        }
        Err(_) => false,
    }
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
        fs::write(dir.path().join(".gitignore"), "node_modules\ntarget\n.env\ndist\n").unwrap();
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
}
```

**Step 2: Register module in `src/project/mod.rs`**

Replace the full file:

```rust
pub mod deps;
pub mod languages;
pub mod security;
pub mod tests_detect;
pub mod vibe_detect;

use std::path::Path;

#[derive(Debug)]
pub struct ProjectStats {
    pub deps: deps::DepsInfo,
    pub tests: tests_detect::TestsInfo,
    pub languages: languages::LanguageStats,
    pub security: security::SecurityInfo,
    pub vibe: vibe_detect::VibeInfo,
}

pub fn analyze_project(path: &Path) -> ProjectStats {
    analyze_project_with_ai_ratio(path, 0.0)
}

pub fn analyze_project_with_ai_ratio(path: &Path, ai_ratio: f64) -> ProjectStats {
    ProjectStats {
        deps: deps::count_deps(path),
        tests: tests_detect::detect_tests(path),
        languages: languages::count_languages(path),
        security: security::check_security(path),
        vibe: vibe_detect::detect_vibe(path, ai_ratio),
    }
}
```

**Step 3: Run tests**

```bash
cargo test -p vibereport -- project::vibe_detect
```

Expected: all vibe_detect tests pass.

**Step 4: Fix compilation errors**

`ProjectStats` now requires `vibe` field. Update all call sites:

- `src/main.rs:78` — change `project::analyze_project(path)` to `project::analyze_project(path)` (unchanged, uses default ai_ratio=0.0)
- After git analysis, call with ai_ratio: change `run_single` and `run_remote` to call `analyze_project_with_ai_ratio(path, git_stats.ai_ratio)`
- `src/score/calculator.rs:101-118` — mock_project_stats must include `vibe: VibeInfo::default()`

**Step 5: Run full test suite**

```bash
cargo test
cargo clippy -- -D warnings
```

**Step 6: Commit**

```bash
git add src/project/vibe_detect.rs src/project/mod.rs
git commit -m "feat: add vibe detectors (lint, ci, boomer ai, gitignore, readme, todo, branch)"
```

---

### Task A2: Update scoring calculator

**Files:**
- Modify: `src/score/calculator.rs`

**Step 1: Rewrite `calculate` function**

Replace the body of `calculate` in `src/score/calculator.rs:20-63`:

```rust
pub fn calculate(git: &GitStats, project: &ProjectStats) -> VibeScore {
    let mut points: u32 = 0;

    // AI ratio (0-60 points)
    points += (git.ai_ratio * 60.0) as u32;

    // No tests (+20) or few tests (+10)
    if !project.tests.has_tests {
        points += 20;
    } else if project.tests.test_files_count < 3 {
        points += 10;
    }

    // .env in git (+20/file, max 60)
    let env_points = (project.security.env_files_count as u32 * 20).min(60);
    points += env_points;

    // Hardcoded secrets (+20/each, max 60)
    let secrets_points = (project.security.hardcoded_secrets_hints as u32 * 20).min(60);
    points += secrets_points;

    // Deps bloat (0-10)
    let deps_score = (project.deps.total as f64 / 100.0).min(1.0) * 10.0;
    points += deps_score as u32;

    // No linting (+10)
    if project.vibe.no_linting {
        points += 10;
    }

    // No CI/CD (+10)
    if project.vibe.no_ci_cd {
        points += 10;
    }

    // Boomer AI (+10)
    if project.vibe.boomer_ai {
        points += 10;
    }

    // node_modules in git (+15)
    if project.vibe.node_modules_in_git {
        points += 15;
    }

    // Mega commit (+10)
    if project.vibe.mega_commit {
        points += 10;
    }

    // No .gitignore (+10)
    if project.vibe.no_gitignore {
        points += 10;
    }

    // No README (+10)
    if project.vibe.no_readme {
        points += 10;
    }

    // TODO flood (+5)
    if project.vibe.todo_flood {
        points += 5;
    }

    // Single branch (+5)
    if project.vibe.single_branch {
        points += 5;
    }

    let grade = grade_from_points(points);
    let roast = super::roast::pick_roast(points, git.ai_ratio, project);

    VibeScore {
        grade,
        points,
        roast,
        ai_ratio: git.ai_ratio,
    }
}
```

**Step 2: Update mock_project_stats in tests**

Add `vibe: crate::project::vibe_detect::VibeInfo::default()` to `mock_project_stats`.

**Step 3: Run tests and fix**

```bash
cargo test -p vibereport -- score::calculator
```

**Step 4: Commit**

```bash
git add src/score/calculator.rs
git commit -m "feat: new scoring weights — AI 60pts, env/secrets +20, new vibe indicators"
```

---

### Task A3: Contextual roasts

**Files:**
- Modify: `src/score/roast.rs`

**Step 1: Rewrite `pick_roast` with contextual patterns**

```rust
use crate::project::ProjectStats;

pub fn pick_roast(points: u32, ai_ratio: f64, project: &ProjectStats) -> String {
    // ── Contextual roasts (checked first, most specific wins) ──

    if project.vibe.node_modules_in_git {
        return "Committing node_modules. Bold strategy.".to_string();
    }
    if project.vibe.boomer_ai {
        return "Uses AI like a boomer uses email.".to_string();
    }
    if ai_ratio > 0.95 {
        return "You're the project manager now.".to_string();
    }
    if ai_ratio > 0.9 && !project.tests.has_tests {
        return "Vibe coded to production. No safety net.".to_string();
    }
    if ai_ratio == 0.0 {
        return "Write code like it's 2019.".to_string();
    }
    if project.security.env_files_count >= 3 {
        return "Your secrets have secrets.".to_string();
    }
    if project.security.env_in_git {
        return "Secrets? What secrets?".to_string();
    }
    if project.deps.total > 500 {
        return "node_modules is the real project.".to_string();
    }
    if !project.tests.has_tests && project.languages.total_lines > 10000 {
        return "10K lines of YOLO.".to_string();
    }
    if project.vibe.no_gitignore && project.vibe.no_readme {
        return "No .gitignore, no README, no mercy.".to_string();
    }
    if project.vibe.todo_flood {
        return "TODO: finish this project.".to_string();
    }
    if project.vibe.single_branch && ai_ratio > 0.5 {
        return "One branch, one dream, one AI.".to_string();
    }
    if project.vibe.no_ci_cd && project.vibe.no_linting {
        return "Deploys from localhost. Formats with vibes.".to_string();
    }

    // ── Score-based fallback ──
    match points {
        101.. => "Beyond vibe. You are the vibe.",
        90..=100 => "The AI is the senior dev here.",
        80..=89 => "You prompt, Claude delivers.",
        70..=79 => "More vibes than version control.",
        60..=69 => "Solid vibe-to-code ratio.",
        50..=59 => "Half human, half machine.",
        40..=49 => "Training wheels still on.",
        30..=39 => "Mostly artisanal, free-range code.",
        20..=29 => "You actually read the docs?",
        _ => "Handcrafted with mass-produced tears.",
    }
    .to_string()
}
```

**Step 2: Run tests**

```bash
cargo test
cargo clippy -- -D warnings
```

**Step 3: Commit**

```bash
git add src/score/roast.rs
git commit -m "feat: contextual roasts — boomer AI, node_modules, TODO flood, etc."
```

---

### Task A4: Add `--since` flag to CLI

**Files:**
- Modify: `src/main.rs:17-37` (Cli struct)
- Modify: `src/git/parser.rs:36-129` (analyze_repo)

**Step 1: Add `since` to Cli struct in `src/main.rs`**

Add after `no_share` field:

```rust
    /// Only analyze commits since this date (YYYY-MM-DD, "6m", "1y", or "all")
    #[arg(long, default_value = "all")]
    since: String,
```

**Step 2: Add date parsing helper**

Add to `src/git/parser.rs` before `analyze_repo`:

```rust
/// Parse a --since value into an optional cutoff DateTime.
/// Accepts: "YYYY-MM-DD", "6m", "1y", "all"
pub fn parse_since(since: &str) -> Option<DateTime<Utc>> {
    match since.trim().to_lowercase().as_str() {
        "all" | "" => None,
        "6m" => Some(Utc::now() - chrono::Duration::days(180)),
        "1y" => Some(Utc::now() - chrono::Duration::days(365)),
        "2y" => Some(Utc::now() - chrono::Duration::days(730)),
        date => {
            chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .map(|dt| dt.and_utc())
        }
    }
}
```

**Step 3: Add `since` parameter to `analyze_repo`**

Change signature to:
```rust
pub fn analyze_repo(path: &Path, since: Option<DateTime<Utc>>) -> Result<GitStats, Box<dyn std::error::Error>>
```

In the commit walk loop, after building `timestamp`, add:
```rust
        // Filter by --since if specified
        if let Some(cutoff) = since {
            if timestamp < cutoff {
                // Still track root commit hash for fingerprint
                root_commit_full_hash = id_str.clone();
                continue;
            }
        }
```

**Step 4: Update all call sites**

- `src/main.rs` run_single: `git::parser::analyze_repo(path, parse_since(&cli.since))`
- `src/main.rs` run_remote: same
- `src/main.rs` run_scan_all: `git::parser::analyze_repo(repo_path, None)`

Add `use git::parser::parse_since;` or inline it.

**Step 5: Update `analyze_project` call in `run_single`/`run_remote`**

After git analysis, pass ai_ratio:
```rust
let project_stats = project::analyze_project_with_ai_ratio(path, git_stats.ai_ratio);
```

**Step 6: Run tests**

```bash
cargo test
cargo clippy -- -D warnings
```

**Step 7: Commit**

```bash
git add src/main.rs src/git/parser.rs
git commit -m "feat: --since flag for temporal filtering (YYYY-MM-DD, 6m, 1y)"
```

---

### Task A5: Update JSON output and upload payload

**Files:**
- Modify: `src/main.rs:134-176` (output_report JSON block)
- Modify: `src/share/upload.rs:1-17` (ReportPayload)

**Step 1: Add vibe fields to JSON output**

In the `serde_json::json!` block in `output_report`, add:

```rust
            "vibe_score": vibe_score.points,
            "vibe": {
                "no_linting": project_stats.vibe.no_linting,
                "no_ci_cd": project_stats.vibe.no_ci_cd,
                "boomer_ai": project_stats.vibe.boomer_ai,
                "node_modules_in_git": project_stats.vibe.node_modules_in_git,
                "no_gitignore": project_stats.vibe.no_gitignore,
                "no_readme": project_stats.vibe.no_readme,
                "todo_flood": project_stats.vibe.todo_flood,
                "todo_count": project_stats.vibe.todo_count,
                "single_branch": project_stats.vibe.single_branch,
                "mega_commit": project_stats.vibe.mega_commit,
            },
```

**Step 2: Add `total_commits`, `ai_commits`, `vibe_score`, `chaos_badges` to ReportPayload**

```rust
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
    pub total_commits: usize,
    pub ai_commits: usize,
    pub languages: String,
    pub repo_fingerprint: Option<String>,
    pub chaos_badges: String, // JSON array of badge names
}
```

**Step 3: Build chaos_badges in `share_report` function**

```rust
    let mut badges: Vec<&str> = Vec::new();
    if !project_stats.tests.has_tests { badges.push("no-tests"); }
    if project_stats.security.env_in_git { badges.push("env-in-git"); }
    if project_stats.security.hardcoded_secrets_hints > 0 { badges.push("hardcoded-secrets"); }
    if project_stats.vibe.no_linting { badges.push("no-linting"); }
    if project_stats.vibe.no_ci_cd { badges.push("no-ci-cd"); }
    if project_stats.vibe.boomer_ai { badges.push("boomer-ai"); }
    if project_stats.vibe.node_modules_in_git { badges.push("node-modules"); }
    if project_stats.vibe.no_gitignore { badges.push("no-gitignore"); }
    if project_stats.vibe.no_readme { badges.push("no-readme"); }
    if project_stats.vibe.todo_flood { badges.push("todo-flood"); }
    if project_stats.vibe.single_branch { badges.push("single-branch"); }
    if project_stats.vibe.mega_commit { badges.push("mega-commit"); }
    let chaos_badges_json = serde_json::to_string(&badges).unwrap_or_else(|_| "[]".into());
```

**Step 4: Run tests**

```bash
cargo test
cargo clippy -- -D warnings
```

**Step 5: Commit**

```bash
git add src/main.rs src/share/upload.rs
git commit -m "feat: vibe indicators in JSON output + upload payload"
```

---

## TRACK B: Frontend Redesign (Astro/Tailwind)

### Task B1: Tug of War component (replace BattleChart)

**Files:**
- Modify: `web/frontend/src/components/BattleChart.astro` (full rewrite)

Rewrite `BattleChart.astro` as a horizontal tug-of-war bar:
- Left side: robot emoji + "AI" + commit count (red/pink)
- Right side: person emoji + "Humans" + commit count (blue)
- Single horizontal bar split proportionally
- Dynamic phrase below based on ratio thresholds
- Animated bar width on scroll (IntersectionObserver)
- Use **frontend-design skill** for polished aesthetics

The component receives `totalAiCommits` and `totalHumanCommits` as props (computed from trends data).

**Step 1: Rewrite component**

Replace full file. Key structure:
```html
<div class="tug-of-war">
  <div class="flex justify-between">
    <div>🤖 AI · {aiCount}</div>
    <div>{humanCount} · 🧑 Humans</div>
  </div>
  <div class="bar-container">
    <div class="ai-bar" style="width: {aiPercent}%"></div>
  </div>
  <p class="battle-phrase">{phrase}</p>
</div>
```

Phrase logic:
```typescript
const phrase = aiPercent < 5 ? "Humans are winning... for now."
  : aiPercent < 20 ? "The machines are gaining ground."
  : aiPercent < 50 ? "It's anyone's game."
  : "AI has taken over.";
```

**Step 2: Update `index.astro` props**

Change from `<BattleChart trends={trends.trends} />` to pass computed totals:
```typescript
const totalAiCommits = trends.trends.reduce((s, t) => s + (t.ai_commits || 0), 0);
const totalHumanCommits = trends.trends.reduce((s, t) => s + ((t.total_commits || 0) - (t.ai_commits || 0)), 0);
```

**Step 3: Build and verify**

```bash
cd web/frontend && npx astro build
```

**Step 4: Commit**

```bash
git add web/frontend/src/components/BattleChart.astro web/frontend/src/pages/index.astro
git commit -m "feat: tug of war visualization — AI vs Humans horizontal bar"
```

---

### Task B2: 2-stat header

**Files:**
- Modify: `web/frontend/src/pages/index.astro:89-110`
- Modify: `web/frontend/src/lib/api.ts:17-21,40-54`

**Step 1: Change stats grid from 3 to 2 columns**

In `index.astro`, change:
```html
<div class="grid grid-cols-1 sm:grid-cols-2 gap-8 sm:gap-12 max-w-2xl mx-auto">
  <StatsCounter label="Repos scanned" value={stats.total_reports} emoji="📊" />
  <StatsCounter label="Commits analyzed" value={stats.total_commits} emoji="⚡" />
</div>
```

Remove Average AI % counter (it's now shown in the tug of war).

**Step 2: Clean up api.ts StatsData**

Remove `average_ai_percent` from `StatsData` if no longer needed elsewhere (check usages first).

**Step 3: Build and verify**

```bash
cd web/frontend && npx astro build
```

**Step 4: Commit**

```bash
git add web/frontend/src/pages/index.astro web/frontend/src/lib/api.ts
git commit -m "feat: 2-stat header — repos scanned + commits analyzed"
```

---

### Task B3: Scan result page with chaos badges

**Files:**
- Modify: `web/frontend/src/pages/scan.astro:67-113` (result section)
- Modify: `web/api/src/index.ts:276-394` (POST /api/scan response)

**Step 1: Update scan result to show AI% + Vibe Score + badges**

In `scan.astro`, after the AI ratio bar, add:
- Vibe Score display: `Vibe Score: {score} · Grade: {grade}`
- Chaos badges row: colored pills based on detected patterns

The API `/api/scan` response already returns `score` and `grade`. For chaos badges, the VPS worker will return them. In the meantime, we can derive basic badges from the available data (ai_ratio > 0 with simplified detection).

**Step 2: Add badge rendering in the result area**

```html
<div id="result-badges" class="flex flex-wrap gap-1.5 mb-4"></div>
```

JS to populate:
```javascript
// Basic badge derivation from scan data
const badges = [];
if (data.ai_ratio > 0) badges.push({ label: 'AI-assisted', color: 'pink' });
if (data.chaos_badges) {
  // From VPS worker — full badge list
  for (const badge of data.chaos_badges) {
    badges.push({ label: badge.replace(/-/g, ' '), color: 'red' });
  }
}
```

**Step 3: Build and verify**

**Step 4: Commit**

---

## TRACK C: VPS Scan Worker (Rust/Axum)

### Task C1: Create VPS worker crate

**Files:**
- Create: `vps-worker/Cargo.toml`
- Create: `vps-worker/src/main.rs`
- Modify: `Cargo.toml` (workspace)

**Step 1: Set up workspace**

Add to root `Cargo.toml`:
```toml
[workspace]
members = [".", "vps-worker"]
```

Create `vps-worker/Cargo.toml`:
```toml
[package]
name = "vps-worker"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4"] }
reqwest = { version = "0.12", features = ["json"] }
tower-http = { version = "0.6", features = ["cors"] }
tracing = "0.1"
tracing-subscriber = "0.3"
```

**Step 2: Create `vps-worker/src/main.rs`**

```rust
use axum::{Router, routing::post, Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Semaphore;
use uuid::Uuid;

struct AppState {
    semaphore: Semaphore,
    auth_token: String,
    vibereport_bin: String,
    api_url: String,
    api_token: Option<String>,
}

#[derive(Deserialize)]
struct ScanRequest {
    repo: String,
    since: Option<String>,
}

#[derive(Serialize)]
struct ScanResponse {
    // passthrough from vibereport --json output
    #[serde(flatten)]
    data: serde_json::Value,
}

async fn scan_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ScanRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Auth check
    let auth = headers.get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if auth != format!("Bearer {}", state.auth_token) {
        return Err((StatusCode::UNAUTHORIZED, "Invalid token".into()));
    }

    // Acquire semaphore (max 5 concurrent clones)
    let _permit = state.semaphore.acquire().await
        .map_err(|_| (StatusCode::TOO_MANY_REQUESTS, "Too many concurrent scans".into()))?;

    let uuid = Uuid::new_v4().to_string();
    let tmp_dir = format!("/tmp/vibereport-{}", uuid);
    let since = req.since.unwrap_or_else(|| "2025-01-01".into());

    // Parse repo: "user/repo" or "github:user/repo" or URL
    let repo_url = if req.repo.starts_with("http") {
        req.repo.clone()
    } else {
        let cleaned = req.repo.replace("github:", "");
        format!("https://github.com/{}.git", cleaned)
    };

    // Clone
    let clone_result = tokio::process::Command::new("git")
        .args(["clone", "--bare", &format!("--shallow-since={}", since), &repo_url, &tmp_dir])
        .output()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Clone failed: {}", e)))?;

    if !clone_result.status.success() {
        let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
        let stderr = String::from_utf8_lossy(&clone_result.stderr);
        return Err((StatusCode::BAD_REQUEST, format!("Clone failed: {}", stderr)));
    }

    // Run vibereport
    let analyze_result = tokio::process::Command::new(&state.vibereport_bin)
        .args([&tmp_dir, "--json", "--since", &since, "--no-share"])
        .output()
        .await
        .map_err(|e| {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Analysis failed: {}", e))
        })?;

    // Cleanup
    let _ = tokio::fs::remove_dir_all(&tmp_dir).await;

    if !analyze_result.status.success() {
        let stderr = String::from_utf8_lossy(&analyze_result.stderr);
        return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Analysis failed: {}", stderr)));
    }

    let stdout = String::from_utf8_lossy(&analyze_result.stdout);
    let data: serde_json::Value = serde_json::from_str(&stdout)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Parse error: {}", e)))?;

    Ok(Json(data))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::init();

    let port = std::env::var("PORT").unwrap_or_else(|_| "3001".into());
    let auth_token = std::env::var("AUTH_TOKEN").expect("AUTH_TOKEN required");
    let vibereport_bin = std::env::var("VIBEREPORT_BIN").unwrap_or_else(|_| "vibereport".into());
    let api_url = std::env::var("API_URL").unwrap_or_else(|_| "https://vibereport-api.clement-serizay.workers.dev".into());
    let api_token = std::env::var("API_TOKEN").ok();

    let state = Arc::new(AppState {
        semaphore: Semaphore::new(5),
        auth_token,
        vibereport_bin,
        api_url,
        api_token,
    });

    let app = Router::new()
        .route("/scan", post(scan_handler))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("VPS worker listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

**Step 3: Build locally**

```bash
cd vps-worker && cargo build
```

**Step 4: Commit**

```bash
git add vps-worker/ Cargo.toml
git commit -m "feat: VPS scan worker — Axum HTTP server with git clone pipeline"
```

---

### Task C2: Deploy to VPS

**Step 1: Cross-compile or build on VPS**

```bash
# Option A: Build on VPS
ssh ubuntu@vps-139a77b3.vps.ovh.net
# Install Rust if needed, then:
cd ~/vibereport && git pull && cargo build --release
cargo build --release -p vps-worker
```

**Step 2: Set up systemd service**

Create `/etc/systemd/system/vibereport-worker.service` on VPS.

**Step 3: Set up nginx reverse proxy**

Configure nginx for scan.vibereport.dev with HTTPS (certbot).

**Step 4: Test endpoint**

```bash
curl -X POST https://scan.vibereport.dev/scan \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"repo":"denoland/deno","since":"2025-01-01"}'
```

---

## TRACK D: API Integration (Cloudflare Worker)

### Task D1: DB schema migration

**Files:**
- Modify: `web/api/schema.sql`

**Step 1: Run migration on D1**

```bash
cd web/api
npx wrangler d1 execute vibereport-db --command "ALTER TABLE reports ADD COLUMN vibe_score INTEGER DEFAULT 0;"
npx wrangler d1 execute vibereport-db --command "ALTER TABLE reports ADD COLUMN chaos_badges TEXT DEFAULT '[]';"
npx wrangler d1 execute vibereport-db --command "ALTER TABLE reports ADD COLUMN scan_source TEXT DEFAULT 'cli';"
npx wrangler d1 execute vibereport-db --command "ALTER TABLE reports ADD COLUMN period_start TEXT;"
npx wrangler d1 execute vibereport-db --command "ALTER TABLE reports ADD COLUMN period_end TEXT;"
```

**Step 2: Update schema.sql for reference**

Add the new columns to the CREATE TABLE statement.

**Step 3: Commit**

```bash
git add web/api/schema.sql
git commit -m "feat: DB schema — vibe_score, chaos_badges, scan_source, period columns"
```

---

### Task D2: Cloudflare Worker VPS proxy

**Files:**
- Modify: `web/api/src/index.ts:276-394` (POST /api/scan)

**Step 1: Add VPS proxy with fallback**

At the top of POST /api/scan handler, try VPS first:

```typescript
  // Try VPS worker first
  const vpsUrl = c.env.VPS_SCAN_URL;
  const vpsToken = c.env.VPS_AUTH_TOKEN;
  if (vpsUrl && vpsToken) {
    try {
      const vpsRes = await fetch(`${vpsUrl}/scan`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${vpsToken}`,
        },
        body: JSON.stringify({ repo: repoInput, since: '2025-01-01' }),
        signal: AbortSignal.timeout(45000),
      });
      if (vpsRes.ok) {
        const vpsData = await vpsRes.json();
        // Store in DB and return
        // ... (save to reports table with scan_source = 'web_vps')
        return c.json(vpsData);
      }
    } catch {
      // VPS down — fall through to GitHub API fallback
    }
  }

  // Fallback: existing GitHub API logic below
```

**Step 2: Add env vars to wrangler.toml**

```toml
[vars]
VPS_SCAN_URL = "https://scan.vibereport.dev"

# Secret (set via wrangler secret put):
# VPS_AUTH_TOKEN
```

**Step 3: Update Bindings type**

```typescript
type Bindings = {
  DB: D1Database
  GITHUB_TOKEN?: string
  VPS_SCAN_URL?: string
  VPS_AUTH_TOKEN?: string
}
```

**Step 4: Deploy**

```bash
cd web/api && npx wrangler deploy
```

**Step 5: Commit**

```bash
git add web/api/
git commit -m "feat: Cloudflare Worker proxies to VPS with GitHub API fallback"
```

---

## Dependency Graph

```
A1 (vibe detectors) ──► A2 (scoring) ──► A3 (roasts) ──► A5 (JSON + upload)
                                                              │
A4 (--since flag) ─────────────────────────────────────────────┤
                                                              │
B1 (tug of war) ──► B2 (2 stats) ──► B3 (scan badges)       │
                                                              │
C1 (VPS worker crate) ──► C2 (deploy to VPS)                 │
                                                              │
D1 (DB migration) ──► D2 (CF Worker proxy) ◄─────────────────┘
```

**Parallel tracks:**
- Track A (CLI) and Track B (Frontend) are independent
- Track C (VPS) depends on Track A being complete (needs the binary)
- Track D depends on Track C (VPS must be deployed) and Track A (new fields)
