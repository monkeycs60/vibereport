use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};

use super::ai_detect::{detect_ai_tool, AiTool};

#[derive(Debug, Clone)]
#[allow(dead_code)]
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
    #[allow(dead_code)]
    pub first_commit_date: Option<DateTime<Utc>>,
    #[allow(dead_code)]
    pub last_commit_date: Option<DateTime<Utc>>,
    pub repo_fingerprint: Option<String>,
}

/// Walk all commits in HEAD and classify each as AI or Human.
pub fn analyze_repo(path: &Path) -> Result<GitStats, Box<dyn std::error::Error>> {
    let repo = gix::open(path)?;

    let head = repo.head_commit()?;
    let mut commits = Vec::new();
    let mut root_commit_full_hash = String::new();

    // Walk all ancestors of HEAD
    for info in head.ancestors().all()? {
        let info = info?;
        let commit = info.object()?;
        let message = commit.message_raw_sloppy().to_string();
        let author_sig = commit.author()?;
        let author_name = author_sig.name.to_string();
        let seconds = author_sig.seconds();

        let timestamp = DateTime::from_timestamp(seconds, 0).unwrap_or_default();

        let ai_tool = detect_ai_tool(&message);

        let id_str = info.id.to_string();
        // Track the full hash; last iteration = oldest (root) commit
        root_commit_full_hash = id_str.clone();

        let short_hash = if id_str.len() >= 8 {
            id_str[..8].to_string()
        } else {
            id_str
        };

        commits.push(CommitInfo {
            hash: short_hash,
            message: message.lines().next().unwrap_or("").to_string(),
            author: author_name,
            timestamp,
            ai_tool,
            lines_added: 0, // TODO: compute from diff in v0.2
            lines_removed: 0,
        });
    }

    // Count AI tools
    let ai_commits = commits
        .iter()
        .filter(|c| c.ai_tool != AiTool::Human)
        .count();
    let human_commits = commits.len() - ai_commits;
    let ai_ratio = if commits.is_empty() {
        0.0
    } else {
        ai_commits as f64 / commits.len() as f64
    };

    // Count by tool
    let ai_tools: Vec<(AiTool, usize)> = commits
        .iter()
        .filter(|c| c.ai_tool != AiTool::Human)
        .fold(HashMap::new(), |mut acc, c| {
            *acc.entry(c.ai_tool.clone()).or_insert(0usize) += 1;
            acc
        })
        .into_iter()
        .collect();

    let first_commit_date = commits.last().map(|c| c.timestamp);
    let last_commit_date = commits.first().map(|c| c.timestamp);

    // Compute repo fingerprint: root commit hash + remote origin URL
    let remote_url = repo.find_remote("origin").ok().and_then(|r| {
        r.url(gix::remote::Direction::Fetch)
            .map(|u| u.to_bstring().to_string())
    });
    let repo_fingerprint = if root_commit_full_hash.is_empty() {
        None
    } else {
        Some(format!(
            "{}:{}",
            root_commit_full_hash,
            remote_url.unwrap_or_default()
        ))
    };

    Ok(GitStats {
        total_commits: commits.len(),
        ai_commits,
        human_commits,
        ai_ratio,
        ai_tools,
        commits,
        first_commit_date,
        last_commit_date,
        repo_fingerprint,
    })
}
