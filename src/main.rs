mod git;
mod project;
mod render;
mod scanner;
mod score;
mod share;

use clap::Parser;
use std::path::Path;

#[derive(Parser, Debug)]
#[command(
    name = "vibereport",
    version,
    about = "The Spotify Wrapped for your code"
)]
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

    /// Don't share report to vibereport.dev
    #[arg(long)]
    no_share: bool,

    /// Only analyze commits since this date (YYYY-MM-DD, "6m", "1y", "2y", or "all")
    #[arg(long, default_value = "all")]
    since: String,
}

fn main() {
    let cli = Cli::parse();

    // Check if path is a GitHub reference
    if let Some((user, repo)) = scanner::remote::parse_github_ref(&cli.path) {
        run_remote(&cli, &user, &repo);
        return;
    }

    let path = Path::new(&cli.path);

    if cli.scan_all {
        run_scan_all(path);
        return;
    }

    run_single(&cli, path);
}

/// Analyze a single local repo.
fn run_single(cli: &Cli, path: &Path) {
    eprintln!("Scanning {}...", path.display());

    // ── Step 1: Analyze git history ──
    let since = git::parser::parse_since(&cli.since);
    let git_stats = match git::parser::analyze_repo(path, since) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: not a git repository ({})", path.display());
            eprintln!("  {}", e);
            eprintln!();
            eprintln!("Usage:");
            eprintln!("  vibereport /path/to/repo       # scan a specific repo");
            eprintln!("  vibereport --scan-all ~/Desktop # scan all repos in a directory");
            eprintln!("  vibereport github:user/repo     # scan a GitHub repo");
            std::process::exit(1);
        }
    };

    // ── Step 2: Analyze project structure ──
    let project_stats = project::analyze_project_with_ai_ratio(path, git_stats.ai_ratio);

    // ── Step 3: Calculate vibe score ──
    let vibe_score = score::calculator::calculate(&git_stats, &project_stats);

    // ── Repo name ──
    let repo_name = path
        .canonicalize()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| cli.path.clone());

    // ── Output + export ──
    output_report(cli, &git_stats, &project_stats, &vibe_score, &repo_name);
}

/// Clone a remote GitHub repo and analyze it.
fn run_remote(cli: &Cli, user: &str, repo: &str) {
    eprintln!("Cloning {}/{}...", user, repo);
    let tmp_path = match scanner::remote::clone_for_analysis(user, repo) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error cloning repo: {}", e);
            std::process::exit(1);
        }
    };

    let repo_name = format!("{}/{}", user, repo);

    // Run the same analysis pipeline as single-repo
    let since = git::parser::parse_since(&cli.since);
    let git_stats = match git::parser::analyze_repo(&tmp_path, since) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error analyzing repo: {}", e);
            scanner::remote::cleanup(&tmp_path);
            std::process::exit(1);
        }
    };
    let project_stats = project::analyze_project_with_ai_ratio(&tmp_path, git_stats.ai_ratio);
    let vibe_score = score::calculator::calculate(&git_stats, &project_stats);

    // Output + export
    output_report(cli, &git_stats, &project_stats, &vibe_score, &repo_name);

    // Cleanup temp dir
    scanner::remote::cleanup(&tmp_path);
}

/// Common output logic: terminal/JSON rendering + SVG export.
fn output_report(
    cli: &Cli,
    git_stats: &git::parser::GitStats,
    project_stats: &project::ProjectStats,
    vibe_score: &score::calculator::VibeScore,
    repo_name: &str,
) {
    if cli.json {
        let languages: std::collections::HashMap<&String, &usize> =
            project_stats.languages.languages.iter().collect();

        let ai_tools: Vec<serde_json::Value> = git_stats
            .ai_tools
            .iter()
            .map(|(tool, count)| {
                serde_json::json!({
                    "tool": tool.to_string(),
                    "commits": count,
                })
            })
            .collect();

        let output = serde_json::json!({
            "repo": repo_name,
            "ai_ratio": vibe_score.ai_ratio,
            "human_ratio": 1.0 - vibe_score.ai_ratio,
            "score": vibe_score.points,
            "vibe_score": vibe_score.points,
            "grade": vibe_score.grade,
            "roast": vibe_score.roast,
            "total_commits": git_stats.total_commits,
            "ai_commits": git_stats.ai_commits,
            "human_commits": git_stats.human_commits,
            "ai_tools": ai_tools,
            "deps": {
                "total": project_stats.deps.total,
                "manager": project_stats.deps.manager,
            },
            "tests": {
                "has_tests": project_stats.tests.has_tests,
                "test_files": project_stats.tests.test_files_count,
                "frameworks": project_stats.tests.frameworks,
            },
            "languages": languages,
            "total_lines": project_stats.languages.total_lines,
            "security": {
                "env_in_git": project_stats.security.env_in_git,
            },
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
        });

        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        render::terminal::render_with_name(git_stats, project_stats, vibe_score, repo_name);
    }

    // ── SVG export ──
    if let Some(svg_path) = &cli.svg {
        let svg_content = render::svg::render_svg(git_stats, project_stats, vibe_score, repo_name);
        std::fs::write(svg_path, &svg_content).unwrap_or_else(|e| {
            eprintln!("Error writing SVG: {}", e);
            std::process::exit(1);
        });
        eprintln!("SVG saved to {}", svg_path);
    }

    // ── Share to vibereport.dev (default unless --no-share) ──
    if !cli.no_share {
        eprintln!("  Sharing stats to vibereport.dev (use --no-share to disable)");
        share_report(git_stats, project_stats, vibe_score, repo_name);
    }
}

/// Build a ReportPayload from computed stats and upload to vibereport.dev.
fn share_report(
    git_stats: &git::parser::GitStats,
    project_stats: &project::ProjectStats,
    vibe_score: &score::calculator::VibeScore,
    repo_name: &str,
) {
    // Determine the most common AI tool, or "Human" if no AI commits
    let ai_tool = git_stats
        .ai_tools
        .iter()
        .max_by_key(|(_, count)| *count)
        .map(|(tool, _)| tool.to_string())
        .unwrap_or_else(|| "Human".to_string());

    // Extract github username from repo_name if it looks like "user/repo"
    let (github_username, short_repo_name) = if repo_name.contains('/') {
        let parts: Vec<&str> = repo_name.splitn(2, '/').collect();
        (Some(parts[0].to_string()), Some(parts[1].to_string()))
    } else {
        (None, Some(repo_name.to_string()))
    };

    // Serialize languages HashMap to JSON string
    let languages_map: std::collections::HashMap<&String, &usize> =
        project_stats.languages.languages.iter().collect();
    let languages_json = serde_json::to_string(&languages_map).unwrap_or_else(|_| "{}".into());

    // Build chaos badges from detected patterns
    let mut badges: Vec<&str> = Vec::new();
    if !project_stats.tests.has_tests {
        badges.push("no-tests");
    }
    if project_stats.security.env_in_git {
        badges.push("env-in-git");
    }
    if project_stats.security.hardcoded_secrets_hints > 0 {
        badges.push("hardcoded-secrets");
    }
    if project_stats.vibe.no_linting {
        badges.push("no-linting");
    }
    if project_stats.vibe.no_ci_cd {
        badges.push("no-ci-cd");
    }
    if project_stats.vibe.boomer_ai {
        badges.push("boomer-ai");
    }
    if project_stats.vibe.node_modules_in_git {
        badges.push("node-modules");
    }
    if project_stats.vibe.no_gitignore {
        badges.push("no-gitignore");
    }
    if project_stats.vibe.no_readme {
        badges.push("no-readme");
    }
    if project_stats.vibe.todo_flood {
        badges.push("todo-flood");
    }
    if project_stats.vibe.single_branch {
        badges.push("single-branch");
    }
    if project_stats.vibe.mega_commit {
        badges.push("mega-commit");
    }
    let chaos_badges_json =
        serde_json::to_string(&badges).unwrap_or_else(|_| "[]".into());

    let payload = share::upload::ReportPayload {
        github_username,
        repo_name: short_repo_name,
        ai_ratio: vibe_score.ai_ratio,
        ai_tool,
        score_points: vibe_score.points,
        score_grade: vibe_score.grade.clone(),
        roast: vibe_score.roast.clone(),
        deps_count: project_stats.deps.total,
        has_tests: project_stats.tests.has_tests,
        total_lines: project_stats.languages.total_lines,
        total_commits: git_stats.total_commits,
        ai_commits: git_stats.ai_commits,
        languages: languages_json,
        repo_fingerprint: git_stats.repo_fingerprint.clone(),
        chaos_badges: chaos_badges_json,
    };

    eprintln!("\n  Uploading report...");

    match share::upload::upload_report(&payload) {
        Ok(resp) => {
            eprintln!("  \u{1f517} Shared! {}", resp.url);
            if let (Some(rank), Some(percentile)) = (resp.rank, resp.percentile) {
                eprintln!(
                    "  \u{1f3c6} Rank #{} \u{2014} More AI-dependent than {:.0}% of devs",
                    rank, percentile
                );
            }
        }
        Err(e) => {
            eprintln!("  Failed to share report: {}", e);
            eprintln!("  (The report was still rendered locally above.)");
        }
    }
}

/// Scan all git repos under the given directory and produce a multi-repo report.
fn run_scan_all(path: &Path) {
    eprintln!("Discovering git repos in {}...", path.display());

    let repo_paths = scanner::discover::find_git_repos(path, 5);

    if repo_paths.is_empty() {
        eprintln!("No git repos found under {}", path.display());
        std::process::exit(1);
    }

    eprintln!("Found {} repos. Analyzing...", repo_paths.len());

    let mut reports = Vec::new();

    for repo_path in &repo_paths {
        let name = repo_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| repo_path.display().to_string());

        eprint!("  {} ... ", name);

        // Analyze git history
        let git_stats = match git::parser::analyze_repo(repo_path, None) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("skipped ({})", e);
                continue;
            }
        };

        // Analyze project structure
        let project_stats = project::analyze_project(repo_path);

        // Calculate vibe score
        let vibe_score = score::calculator::calculate(&git_stats, &project_stats);

        eprintln!("OK ({} commits)", git_stats.total_commits);

        reports.push(scanner::multi_report::RepoReport {
            path: repo_path.clone(),
            name,
            git_stats,
            project_stats,
            score: vibe_score,
        });
    }

    if reports.is_empty() {
        eprintln!("All repos failed to parse.");
        std::process::exit(1);
    }

    let multi = scanner::multi_report::aggregate(reports);
    render::terminal::render_multi(&multi);
}
