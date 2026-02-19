mod git;
mod project;
mod render;
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
    let path = Path::new(&cli.path);

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

    // ── Repo name (used by both terminal and SVG output) ──
    let repo_name = path
        .canonicalize()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| cli.path.clone());

    // ── Step 4: Output ──
    if cli.json {
        // JSON output
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
            "repo": &repo_name,
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
        // Beautiful terminal output
        render::terminal::render_with_name(&git_stats, &project_stats, &vibe_score, &repo_name);
    }

    // ── SVG export ──
    if let Some(svg_path) = &cli.svg {
        let svg_content =
            render::svg::render_svg(&git_stats, &project_stats, &vibe_score, &repo_name);
        std::fs::write(svg_path, &svg_content).unwrap_or_else(|e| {
            eprintln!("Error writing SVG: {}", e);
            std::process::exit(1);
        });
        eprintln!("SVG saved to {}", svg_path);
    }
}
