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

    // AI ratio is the dominant factor (0-70 points)
    points += (git.ai_ratio * 70.0) as u32;

    // Dependencies boost (0-10 points) — more deps = more vibe
    let deps_score = (project.deps.total as f64 / 100.0).min(1.0) * 10.0;
    points += deps_score as u32;

    // No tests = more vibe (0-10 points)
    if !project.tests.has_tests {
        points += 10;
    } else if project.tests.test_files_count < 5 {
        points += 5;
    }

    // Large codebase with high AI ratio = impressive vibe (0-5 points)
    let size_factor = (project.languages.total_lines as f64 / 10000.0).min(1.0);
    points += (size_factor * 5.0) as u32;

    // Security issues = extra vibe points (0-5 points)
    if project.security.env_in_git {
        points += 5;
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
    }
    .to_string();

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
            deps: crate::project::deps::DepsInfo {
                total: deps,
                manager: "npm".into(),
            },
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
        assert!(
            score.points >= 70,
            "Expected high score, got {}",
            score.points
        );
    }

    #[test]
    fn zero_ai_with_tests_low_score() {
        let git = mock_git_stats(0.0);
        let proj = mock_project_stats(5, true);
        let score = calculate(&git, &proj);
        assert!(
            score.points <= 30,
            "Expected low score, got {}",
            score.points
        );
    }

    #[test]
    fn score_capped_at_100() {
        let git = mock_git_stats(1.0);
        let mut proj = mock_project_stats(500, false);
        proj.security.env_in_git = true;
        proj.languages.total_lines = 50000;
        let score = calculate(&git, &proj);
        assert!(
            score.points <= 100,
            "Score should be capped at 100, got {}",
            score.points
        );
    }

    #[test]
    fn grade_matches_points() {
        let git = mock_git_stats(0.5);
        let proj = mock_project_stats(50, true);
        let score = calculate(&git, &proj);
        // Points should be moderate, grade should match
        let expected_grade = match score.points {
            90..=100 => "S",
            80..=89 => "A+",
            70..=79 => "A",
            60..=69 => "B+",
            50..=59 => "B",
            40..=49 => "C+",
            30..=39 => "C",
            20..=29 => "D",
            _ => "F",
        };
        assert_eq!(score.grade, expected_grade);
    }

    #[test]
    fn roast_is_not_empty() {
        let git = mock_git_stats(0.5);
        let proj = mock_project_stats(10, true);
        let score = calculate(&git, &proj);
        assert!(!score.roast.is_empty(), "Roast should not be empty");
    }
}
