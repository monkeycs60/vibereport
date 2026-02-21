pub mod deps;
pub mod languages;
pub mod security;
pub mod tests_detect;
pub mod vibe_detect;

use std::path::Path;

#[derive(Debug)]
pub struct ProjectStats {
    pub deps: deps::DepsInfo,
    pub tests: tests_detect::TestsInfo,
    pub languages: languages::LanguageStats,
    pub security: security::SecurityInfo,
    pub vibe: vibe_detect::VibeInfo,
}

pub fn analyze_project(path: &Path) -> ProjectStats {
    analyze_project_with_ai_ratio(path, 0.0)
}

pub fn analyze_project_with_ai_ratio(path: &Path, ai_ratio: f64) -> ProjectStats {
    ProjectStats {
        deps: deps::count_deps(path),
        tests: tests_detect::detect_tests(path),
        languages: languages::count_languages(path),
        security: security::check_security(path),
        vibe: vibe_detect::detect_vibe(path, ai_ratio),
    }
}
