use crate::git::parser::GitStats;
use crate::project::ProjectStats;

#[derive(Debug, Clone)]
pub struct ScoreFactor {
    pub label: String,
    pub points: u32,
}

#[derive(Debug, Clone)]
pub struct VibeScore {
    /// Letter grade S+ to F
    pub grade: String,
    /// Numeric score 0-100+ (can exceed 100 for peak vibe chaos)
    pub points: u32,
    /// Fun tagline
    pub roast: String,
    /// AI percentage (0.0 to 1.0)
    pub ai_ratio: f64,
    /// Score breakdown by factor
    pub breakdown: Vec<ScoreFactor>,
}

/// Compute the Vibe Score based on git stats and project stats.
/// Higher score = more "vibe coded" (this is not a quality judgment,
/// it's a fun metric for how AI-assisted your project is).
/// Score CAN exceed 100 for true vibe chaos (S+ tier).
pub fn calculate(git: &GitStats, project: &ProjectStats) -> VibeScore {
    let mut points: u32 = 0;
    let mut breakdown: Vec<ScoreFactor> = Vec::new();

    // AI ratio (0-60 points)
    let ai_pts = (git.ai_ratio * 60.0) as u32;
    points += ai_pts;
    if ai_pts > 0 {
        breakdown.push(ScoreFactor {
            label: "AI Ratio".into(),
            points: ai_pts,
        });
    }

    // No tests (+20) or few tests (+10)
    if !project.tests.has_tests {
        points += 20;
        breakdown.push(ScoreFactor {
            label: "No Tests".into(),
            points: 20,
        });
    } else if project.tests.test_files_count < 3 {
        points += 10;
        breakdown.push(ScoreFactor {
            label: "Few Tests".into(),
            points: 10,
        });
    }

    // .env in git (+20/file, max 60)
    let env_points = (project.security.env_files_count as u32 * 20).min(60);
    points += env_points;
    if env_points > 0 {
        breakdown.push(ScoreFactor {
            label: ".env in Git".into(),
            points: env_points,
        });
    }

    // Hardcoded secrets (+20/each, max 60)
    let secrets_points = (project.security.hardcoded_secrets_hints as u32 * 20).min(60);
    points += secrets_points;
    if secrets_points > 0 {
        breakdown.push(ScoreFactor {
            label: "Hardcoded Secrets".into(),
            points: secrets_points,
        });
    }

    // Deps bloat (0-10)
    let deps_score = (project.deps.total as f64 / 100.0).min(1.0) * 10.0;
    let deps_pts = deps_score as u32;
    points += deps_pts;
    if deps_pts > 0 {
        breakdown.push(ScoreFactor {
            label: "Deps Bloat".into(),
            points: deps_pts,
        });
    }

    // No linting (+10)
    if project.vibe.no_linting {
        points += 10;
        breakdown.push(ScoreFactor {
            label: "No Linting".into(),
            points: 10,
        });
    }

    // No CI/CD (+10)
    if project.vibe.no_ci_cd {
        points += 10;
        breakdown.push(ScoreFactor {
            label: "No CI/CD".into(),
            points: 10,
        });
    }

    // Boomer AI (+10)
    if project.vibe.boomer_ai {
        points += 10;
        breakdown.push(ScoreFactor {
            label: "Boomer AI".into(),
            points: 10,
        });
    }

    // node_modules in git (+15)
    if project.vibe.node_modules_in_git {
        points += 15;
        breakdown.push(ScoreFactor {
            label: "node_modules in Git".into(),
            points: 15,
        });
    }

    // Mega commit (+10)
    if project.vibe.mega_commit {
        points += 10;
        breakdown.push(ScoreFactor {
            label: "Mega Commit".into(),
            points: 10,
        });
    }

    // No .gitignore (+10)
    if project.vibe.no_gitignore {
        points += 10;
        breakdown.push(ScoreFactor {
            label: "No .gitignore".into(),
            points: 10,
        });
    }

    // No README (+10)
    if project.vibe.no_readme {
        points += 10;
        breakdown.push(ScoreFactor {
            label: "No README".into(),
            points: 10,
        });
    }

    // TODO flood (+5)
    if project.vibe.todo_flood {
        points += 5;
        breakdown.push(ScoreFactor {
            label: "TODO Flood".into(),
            points: 5,
        });
    }

    // Single branch (+5)
    if project.vibe.single_branch {
        points += 5;
        breakdown.push(ScoreFactor {
            label: "Single Branch".into(),
            points: 5,
        });
    }

    // Score is NOT capped — true chaos can exceed 100
    let grade = grade_from_points(points);
    let roast = super::roast::pick_roast(points, git.ai_ratio, project);

    VibeScore {
        grade,
        points,
        roast,
        ai_ratio: git.ai_ratio,
        breakdown,
    }
}

/// Map points to letter grade. S+ for scores above 100.
pub fn grade_from_points(points: u32) -> String {
    match points {
        101.. => "S+",
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
    .to_string()
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
            repo_fingerprint: None,
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
            vibe: crate::project::vibe_detect::VibeInfo::default(),
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
    fn score_can_exceed_100() {
        let git = mock_git_stats(1.0);
        let mut proj = mock_project_stats(500, false);
        proj.security.env_in_git = true;
        proj.security.env_files_count = 4;
        proj.security.hardcoded_secrets_hints = 5;
        proj.languages.total_lines = 50000;
        let score = calculate(&git, &proj);
        assert!(
            score.points > 100,
            "Expected score > 100 for peak chaos, got {}",
            score.points
        );
        assert_eq!(score.grade, "S+");
    }

    #[test]
    fn grade_matches_points() {
        let git = mock_git_stats(0.5);
        let proj = mock_project_stats(50, true);
        let score = calculate(&git, &proj);
        let expected_grade = grade_from_points(score.points);
        assert_eq!(score.grade, expected_grade);
    }

    #[test]
    fn roast_is_not_empty() {
        let git = mock_git_stats(0.5);
        let proj = mock_project_stats(10, true);
        let score = calculate(&git, &proj);
        assert!(!score.roast.is_empty(), "Roast should not be empty");
    }

    #[test]
    fn env_files_add_security_points() {
        let git = mock_git_stats(0.5);
        let mut proj = mock_project_stats(10, true);
        let score_clean = calculate(&git, &proj);

        proj.security.env_in_git = true;
        proj.security.env_files_count = 3;
        let score_dirty = calculate(&git, &proj);

        assert!(
            score_dirty.points > score_clean.points,
            "Env files should add points: {} vs {}",
            score_dirty.points,
            score_clean.points
        );
    }

    #[test]
    fn s_plus_grade_above_100() {
        assert_eq!(grade_from_points(101), "S+");
        assert_eq!(grade_from_points(120), "S+");
        assert_eq!(grade_from_points(100), "S");
        assert_eq!(grade_from_points(90), "S");
    }
}
