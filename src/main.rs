mod git;
mod project;
mod render;
mod scanner;
mod score;

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

    /// Share report to vibereport.dev and get a public link
    #[arg(long)]
    share: bool,
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
    let git_stats = match git::parser::analyze_repo(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading git repo: {}", e);
            std::process::exit(1);
        }
    };

    // ── Step 2: Analyze project structure ──
    let project_stats = project::analyze_project(path);

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
    let git_stats = match git::parser::analyze_repo(&tmp_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error analyzing repo: {}", e);
            scanner::remote::cleanup(&tmp_path);
            std::process::exit(1);
        }
    };
    let project_stats = project::analyze_project(&tmp_path);
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
        let git_stats = match git::parser::analyze_repo(repo_path) {
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
