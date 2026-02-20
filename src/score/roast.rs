use crate::project::ProjectStats;

/// Pick a fun roast tagline based on the score and project characteristics.
pub fn pick_roast(points: u32, ai_ratio: f64, project: &ProjectStats) -> String {
    // Special case roasts (checked first)
    if ai_ratio > 0.95 {
        return "You're the project manager now.".to_string();
    }
    if ai_ratio > 0.9 && !project.tests.has_tests {
        return "Ships fast, tests never.".to_string();
    }
    if ai_ratio == 0.0 {
        return "Write code like it's 2019.".to_string();
    }
    if project.deps.total > 500 {
        return "node_modules is the real project.".to_string();
    }
    if !project.tests.has_tests && project.languages.total_lines > 10000 {
        return "10K lines of YOLO.".to_string();
    }
    if project.security.env_in_git {
        return "Secrets? What secrets?".to_string();
    }

    // Score-based roasts
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
