pub mod deps;
pub mod languages;
pub mod security;
pub mod tests_detect;

use std::path::Path;

#[derive(Debug)]
pub struct ProjectStats {
    pub deps: deps::DepsInfo,
    pub tests: tests_detect::TestsInfo,
    pub languages: languages::LanguageStats,
    pub security: security::SecurityInfo,
}

pub fn analyze_project(path: &Path) -> ProjectStats {
    ProjectStats {
        deps: deps::count_deps(path),
        tests: tests_detect::detect_tests(path),
        languages: languages::count_languages(path),
        security: security::check_security(path),
    }
}
