use clap::Parser;

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
    println!("Scanning {}...", cli.path);
}
