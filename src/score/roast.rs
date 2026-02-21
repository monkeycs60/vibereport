use crate::project::ProjectStats;

/// Pick a fun roast tagline based on the score and project characteristics.
pub fn pick_roast(points: u32, ai_ratio: f64, project: &ProjectStats) -> String {
    // ── Contextual roasts (checked first, most specific wins) ──

    if project.vibe.node_modules_in_git {
        return "Committing node_modules. Bold strategy.".to_string();
    }
    if project.vibe.boomer_ai {
        return "Uses AI like a boomer uses email.".to_string();
    }
    if ai_ratio > 0.95 {
        return "You're the project manager now.".to_string();
    }
    if ai_ratio > 0.9 && !project.tests.has_tests {
        return "Vibe coded to production. No safety net.".to_string();
    }
    if ai_ratio == 0.0 {
        return "Write code like it's 2019.".to_string();
    }
    if project.security.env_files_count >= 3 {
        return "Your secrets have secrets.".to_string();
    }
    if project.security.env_in_git {
        return "Secrets? What secrets?".to_string();
    }
    if project.deps.total > 500 {
        return "node_modules is the real project.".to_string();
    }
    if !project.tests.has_tests && project.languages.total_lines > 10000 {
        return "10K lines of YOLO.".to_string();
    }
    if project.vibe.no_gitignore && project.vibe.no_readme {
        return "No .gitignore, no README, no mercy.".to_string();
    }
    if project.vibe.todo_flood {
        return "TODO: finish this project.".to_string();
    }
    if project.vibe.single_branch && ai_ratio > 0.5 {
        return "One branch, one dream, one AI.".to_string();
    }
    if project.vibe.no_ci_cd && project.vibe.no_linting {
        return "Deploys from localhost. Formats with vibes.".to_string();
    }

    // ── Score-based fallback ──
    match points {
        101.. => "Beyond vibe. You are the vibe.",
        90..=100 => "The AI is the senior dev here.",
        80..=89 => "You prompt, Claude delivers.",
        70..=79 => "More vibes than version control.",
        60..=69 => "Solid vibe-to-code ratio.",
        50..=59 => "Half human, half machine.",
        40..=49 => "Training wheels still on.",
        30..=39 => "Mostly artisanal, free-range code.",
        20..=29 => "You actually read the docs?",
        _ => "Handcrafted with mass-produced tears.",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::deps::DepsInfo;
    use crate::project::languages::LanguageStats;
    use crate::project::security::SecurityInfo;
    use crate::project::tests_detect::TestsInfo;
    use crate::project::vibe_detect::VibeInfo;
    use std::collections::HashMap;

    fn base_project() -> ProjectStats {
        ProjectStats {
            deps: DepsInfo {
                total: 10,
                manager: "npm".into(),
            },
            tests: TestsInfo {
                has_tests: true,
                test_files_count: 5,
                frameworks: vec![],
            },
            languages: LanguageStats {
                languages: HashMap::new(),
                total_lines: 5000,
            },
            security: SecurityInfo::default(),
            vibe: VibeInfo::default(),
        }
    }

    #[test]
    fn node_modules_in_git_roast() {
        let mut p = base_project();
        p.vibe.node_modules_in_git = true;
        let roast = pick_roast(50, 0.5, &p);
        assert_eq!(roast, "Committing node_modules. Bold strategy.");
    }

    #[test]
    fn boomer_ai_roast() {
        let mut p = base_project();
        p.vibe.boomer_ai = true;
        let roast = pick_roast(50, 0.5, &p);
        assert_eq!(roast, "Uses AI like a boomer uses email.");
    }

    #[test]
    fn project_manager_roast() {
        let p = base_project();
        let roast = pick_roast(60, 0.96, &p);
        assert_eq!(roast, "You're the project manager now.");
    }

    #[test]
    fn vibe_coded_no_safety_net() {
        let mut p = base_project();
        p.tests.has_tests = false;
        p.tests.test_files_count = 0;
        let roast = pick_roast(80, 0.92, &p);
        assert_eq!(roast, "Vibe coded to production. No safety net.");
    }

    #[test]
    fn zero_ai_roast() {
        let p = base_project();
        let roast = pick_roast(10, 0.0, &p);
        assert_eq!(roast, "Write code like it's 2019.");
    }

    #[test]
    fn many_env_files_roast() {
        let mut p = base_project();
        p.security.env_files_count = 3;
        p.security.env_in_git = true;
        let roast = pick_roast(50, 0.5, &p);
        assert_eq!(roast, "Your secrets have secrets.");
    }

    #[test]
    fn env_in_git_roast() {
        let mut p = base_project();
        p.security.env_in_git = true;
        p.security.env_files_count = 1;
        let roast = pick_roast(50, 0.5, &p);
        assert_eq!(roast, "Secrets? What secrets?");
    }

    #[test]
    fn huge_deps_roast() {
        let mut p = base_project();
        p.deps.total = 600;
        let roast = pick_roast(50, 0.5, &p);
        assert_eq!(roast, "node_modules is the real project.");
    }

    #[test]
    fn yolo_10k_lines_no_tests() {
        let mut p = base_project();
        p.tests.has_tests = false;
        p.tests.test_files_count = 0;
        p.languages.total_lines = 15000;
        let roast = pick_roast(50, 0.5, &p);
        assert_eq!(roast, "10K lines of YOLO.");
    }

    #[test]
    fn no_gitignore_no_readme_roast() {
        let mut p = base_project();
        p.vibe.no_gitignore = true;
        p.vibe.no_readme = true;
        let roast = pick_roast(50, 0.5, &p);
        assert_eq!(roast, "No .gitignore, no README, no mercy.");
    }

    #[test]
    fn todo_flood_roast() {
        let mut p = base_project();
        p.vibe.todo_flood = true;
        p.vibe.todo_count = 25;
        let roast = pick_roast(50, 0.5, &p);
        assert_eq!(roast, "TODO: finish this project.");
    }

    #[test]
    fn single_branch_high_ai_roast() {
        let mut p = base_project();
        p.vibe.single_branch = true;
        let roast = pick_roast(50, 0.6, &p);
        assert_eq!(roast, "One branch, one dream, one AI.");
    }

    #[test]
    fn no_ci_no_linting_roast() {
        let mut p = base_project();
        p.vibe.no_ci_cd = true;
        p.vibe.no_linting = true;
        let roast = pick_roast(50, 0.5, &p);
        assert_eq!(roast, "Deploys from localhost. Formats with vibes.");
    }

    #[test]
    fn score_based_fallback_high() {
        let p = base_project();
        let roast = pick_roast(105, 0.5, &p);
        assert_eq!(roast, "Beyond vibe. You are the vibe.");
    }

    #[test]
    fn score_based_fallback_mid() {
        let p = base_project();
        let roast = pick_roast(55, 0.5, &p);
        assert_eq!(roast, "Half human, half machine.");
    }

    #[test]
    fn score_based_fallback_low() {
        let p = base_project();
        let roast = pick_roast(5, 0.3, &p);
        assert_eq!(roast, "Handcrafted with mass-produced tears.");
    }

    #[test]
    fn contextual_takes_priority_over_score() {
        // node_modules_in_git should trigger even with score of 105
        let mut p = base_project();
        p.vibe.node_modules_in_git = true;
        let roast = pick_roast(105, 0.5, &p);
        assert_eq!(roast, "Committing node_modules. Bold strategy.");
    }

    #[test]
    fn priority_order_node_modules_over_boomer() {
        let mut p = base_project();
        p.vibe.node_modules_in_git = true;
        p.vibe.boomer_ai = true;
        let roast = pick_roast(50, 0.5, &p);
        // node_modules_in_git is checked first
        assert_eq!(roast, "Committing node_modules. Bold strategy.");
    }

    #[test]
    fn single_branch_low_ai_falls_through() {
        // single_branch with ai_ratio <= 0.5 should NOT trigger the single_branch roast
        let mut p = base_project();
        p.vibe.single_branch = true;
        let roast = pick_roast(55, 0.4, &p);
        // Falls through to score-based
        assert_eq!(roast, "Half human, half machine.");
    }
}
