use crate::git::parser::GitStats;
use crate::project::ProjectStats;
use crate::score::calculator::VibeScore;
use std::path::PathBuf;

#[derive(Debug)]
#[allow(dead_code)]
pub struct RepoReport {
    pub path: PathBuf,
    pub name: String,
    pub git_stats: GitStats,
    pub project_stats: ProjectStats,
    pub score: VibeScore,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct MultiReport {
    pub repos: Vec<RepoReport>,
    pub total_commits: usize,
    pub total_ai_commits: usize,
    pub global_ai_ratio: f64,
    pub total_lines: usize,
    pub average_score: u32,
}

/// Aggregate individual repo reports into a combined multi-report.
pub fn aggregate(repos: Vec<RepoReport>) -> MultiReport {
    let total_commits: usize = repos.iter().map(|r| r.git_stats.total_commits).sum();
    let total_ai_commits: usize = repos.iter().map(|r| r.git_stats.ai_commits).sum();
    let global_ai_ratio = if total_commits > 0 {
        total_ai_commits as f64 / total_commits as f64
    } else {
        0.0
    };
    let total_lines: usize = repos
        .iter()
        .map(|r| r.project_stats.languages.total_lines)
        .sum();
    let average_score = if repos.is_empty() {
        0
    } else {
        repos.iter().map(|r| r.score.points as usize).sum::<usize>() as u32 / repos.len() as u32
    };

    MultiReport {
        repos,
        total_commits,
        total_ai_commits,
        global_ai_ratio,
        total_lines,
        average_score,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::ai_detect::AiTool;

    fn mock_repo_report(
        name: &str,
        total_commits: usize,
        ai_commits: usize,
        total_lines: usize,
        score_points: u32,
    ) -> RepoReport {
        let ai_ratio = if total_commits > 0 {
            ai_commits as f64 / total_commits as f64
        } else {
            0.0
        };
        RepoReport {
            path: PathBuf::from(format!("/fake/{}", name)),
            name: name.to_string(),
            git_stats: GitStats {
                total_commits,
                ai_commits,
                human_commits: total_commits - ai_commits,
                ai_ratio,
                ai_tools: vec![(AiTool::ClaudeCode, ai_commits)],
                commits: vec![],
                first_commit_date: None,
                last_commit_date: None,
                repo_fingerprint: None,
            },
            project_stats: ProjectStats {
                deps: crate::project::deps::DepsInfo {
                    total: 10,
                    manager: "npm".into(),
                },
                tests: crate::project::tests_detect::TestsInfo {
                    has_tests: true,
                    test_files_count: 5,
                    frameworks: vec![],
                },
                languages: crate::project::languages::LanguageStats {
                    languages: std::collections::HashMap::new(),
                    total_lines,
                },
                security: crate::project::security::SecurityInfo::default(),
                vibe: crate::project::vibe_detect::VibeInfo::default(),
            },
            score: VibeScore {
                grade: "B".to_string(),
                points: score_points,
                roast: "Test roast".to_string(),
                ai_ratio,
            },
        }
    }

    #[test]
    fn aggregate_empty() {
        let report = aggregate(vec![]);
        assert_eq!(report.total_commits, 0);
        assert_eq!(report.total_ai_commits, 0);
        assert_eq!(report.global_ai_ratio, 0.0);
        assert_eq!(report.total_lines, 0);
        assert_eq!(report.average_score, 0);
        assert!(report.repos.is_empty());
    }

    #[test]
    fn aggregate_single_repo() {
        let repo = mock_repo_report("my-project", 100, 60, 5000, 70);
        let report = aggregate(vec![repo]);

        assert_eq!(report.total_commits, 100);
        assert_eq!(report.total_ai_commits, 60);
        assert!((report.global_ai_ratio - 0.6).abs() < f64::EPSILON);
        assert_eq!(report.total_lines, 5000);
        assert_eq!(report.average_score, 70);
        assert_eq!(report.repos.len(), 1);
    }

    #[test]
    fn aggregate_multiple_repos() {
        let repos = vec![
            mock_repo_report("project-a", 100, 80, 10000, 80),
            mock_repo_report("project-b", 50, 10, 3000, 40),
            mock_repo_report("project-c", 200, 100, 20000, 60),
        ];
        let report = aggregate(repos);

        assert_eq!(report.total_commits, 350);
        assert_eq!(report.total_ai_commits, 190);
        // 190/350 ~= 0.5428
        assert!((report.global_ai_ratio - 190.0 / 350.0).abs() < 0.001);
        assert_eq!(report.total_lines, 33000);
        // (80 + 40 + 60) / 3 = 60
        assert_eq!(report.average_score, 60);
        assert_eq!(report.repos.len(), 3);
    }
}
