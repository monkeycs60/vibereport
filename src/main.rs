mod git;
mod project;
mod score;

use clap::Parser;
use std::path::Path;

#[derive(Parser, Debug)]
#[command(name = "vibereport", version, about = "The Spotify Wrapped for your code")]
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

    println!("Scanning {}...", path.display());

    match git::parser::analyze_repo(path) {
        Ok(stats) => {
            println!("Total commits: {}", stats.total_commits);
            println!(
                "AI commits: {} ({:.0}%)",
                stats.ai_commits,
                stats.ai_ratio * 100.0
            );
            println!("Human commits: {}", stats.human_commits);
            for (tool, count) in &stats.ai_tools {
                println!("  {}: {}", tool, count);
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}
