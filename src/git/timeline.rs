use std::collections::BTreeMap;

use chrono::Datelike;

use super::ai_detect::AiTool;
use super::parser::CommitInfo;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MonthlyStats {
    pub year: i32,
    pub month: u32,
    pub total_commits: usize,
    pub ai_commits: usize,
    pub human_commits: usize,
    pub ai_ratio: f64,
}

/// Group commits by month and compute AI ratio per month.
/// Returns sorted by date (oldest first).
pub fn build_timeline(commits: &[CommitInfo]) -> Vec<MonthlyStats> {
    // Use BTreeMap so keys are naturally sorted (oldest first).
    let mut buckets: BTreeMap<(i32, u32), (usize, usize)> = BTreeMap::new();

    for commit in commits {
        let key = (commit.timestamp.year(), commit.timestamp.month());
        let entry = buckets.entry(key).or_insert((0, 0));
        entry.0 += 1; // total
        if commit.ai_tool != AiTool::Human {
            entry.1 += 1; // ai
        }
    }

    buckets
        .into_iter()
        .map(|((year, month), (total, ai))| {
            let human = total - ai;
            let ai_ratio = if total == 0 {
                0.0
            } else {
                ai as f64 / total as f64
            };
            MonthlyStats {
                year,
                month,
                total_commits: total,
                ai_commits: ai,
                human_commits: human,
                ai_ratio,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn make_commit(year: i32, month: u32, day: u32, ai_tool: AiTool) -> CommitInfo {
        CommitInfo {
            hash: "abcd1234".to_string(),
            message: "test commit".to_string(),
            author: "dev".to_string(),
            timestamp: Utc.with_ymd_and_hms(year, month, day, 12, 0, 0).unwrap(),
            ai_tool,
            lines_added: 0,
            lines_removed: 0,
        }
    }

    #[test]
    fn empty_commits_yields_empty_timeline() {
        let timeline = build_timeline(&[]);
        assert!(timeline.is_empty());
    }

    #[test]
    fn all_commits_same_month_single_entry() {
        let commits = vec![
            make_commit(2025, 6, 1, AiTool::ClaudeCode),
            make_commit(2025, 6, 15, AiTool::Human),
            make_commit(2025, 6, 28, AiTool::Aider),
        ];
        let timeline = build_timeline(&commits);
        assert_eq!(timeline.len(), 1);
        let entry = &timeline[0];
        assert_eq!(entry.year, 2025);
        assert_eq!(entry.month, 6);
        assert_eq!(entry.total_commits, 3);
        assert_eq!(entry.ai_commits, 2);
        assert_eq!(entry.human_commits, 1);
        assert!((entry.ai_ratio - 2.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn multiple_months_sorted_oldest_first() {
        let commits = vec![
            make_commit(2025, 9, 5, AiTool::Human),
            make_commit(2025, 3, 10, AiTool::ClaudeCode),
            make_commit(2025, 6, 20, AiTool::Cursor),
        ];
        let timeline = build_timeline(&commits);
        assert_eq!(timeline.len(), 3);
        assert_eq!((timeline[0].year, timeline[0].month), (2025, 3));
        assert_eq!((timeline[1].year, timeline[1].month), (2025, 6));
        assert_eq!((timeline[2].year, timeline[2].month), (2025, 9));
    }

    #[test]
    fn mix_ai_and_human_correct_per_month_breakdown() {
        let commits = vec![
            // January: 2 AI, 1 human
            make_commit(2025, 1, 3, AiTool::ClaudeCode),
            make_commit(2025, 1, 12, AiTool::Aider),
            make_commit(2025, 1, 20, AiTool::Human),
            // February: 0 AI, 2 human
            make_commit(2025, 2, 5, AiTool::Human),
            make_commit(2025, 2, 18, AiTool::Human),
            // March: 3 AI, 0 human
            make_commit(2025, 3, 1, AiTool::GeminiCli),
            make_commit(2025, 3, 15, AiTool::CodexCli),
            make_commit(2025, 3, 28, AiTool::GithubCopilot),
        ];
        let timeline = build_timeline(&commits);
        assert_eq!(timeline.len(), 3);

        // January
        assert_eq!(timeline[0].total_commits, 3);
        assert_eq!(timeline[0].ai_commits, 2);
        assert_eq!(timeline[0].human_commits, 1);
        assert!((timeline[0].ai_ratio - 2.0 / 3.0).abs() < 1e-9);

        // February
        assert_eq!(timeline[1].total_commits, 2);
        assert_eq!(timeline[1].ai_commits, 0);
        assert_eq!(timeline[1].human_commits, 2);
        assert!((timeline[1].ai_ratio - 0.0).abs() < 1e-9);

        // March
        assert_eq!(timeline[2].total_commits, 3);
        assert_eq!(timeline[2].ai_commits, 3);
        assert_eq!(timeline[2].human_commits, 0);
        assert!((timeline[2].ai_ratio - 1.0).abs() < 1e-9);
    }

    #[test]
    fn spans_across_years() {
        let commits = vec![
            make_commit(2024, 12, 25, AiTool::ClaudeCode),
            make_commit(2025, 1, 5, AiTool::Human),
        ];
        let timeline = build_timeline(&commits);
        assert_eq!(timeline.len(), 2);
        assert_eq!((timeline[0].year, timeline[0].month), (2024, 12));
        assert_eq!((timeline[1].year, timeline[1].month), (2025, 1));
    }
}
