use crate::git::parser::GitStats;
use crate::git::timeline::{build_timeline, MonthlyStats};
use crate::project::ProjectStats;
use crate::score::calculator::VibeScore;
use owo_colors::OwoColorize;

/// Inner width (content area between the two border chars).
const W: usize = 52;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  Public API
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Render a full vibe report with repo name shown under the title.
pub fn render_with_name(
    git: &GitStats,
    project: &ProjectStats,
    score: &VibeScore,
    repo_name: &str,
) {
    println!();
    border_top();
    blank();

    // ── Title ──
    center_bold("VIBE REPORT");
    let subtitle = format!("{}  {}", repo_name, emoji_for_grade(&score.grade));
    center_dimmed(&subtitle);
    blank();
    separator();
    blank();

    // ── AI vs Human ──
    kv("AI-authored", &format!("{:.0}%", score.ai_ratio * 100.0));
    kv(
        "Human-authored",
        &format!("{:.0}%", (1.0 - score.ai_ratio) * 100.0),
    );
    kv("Total commits", &git.total_commits.to_string());
    blank();

    // ── AI Tool Breakdown ──
    if !git.ai_tools.is_empty() {
        section("AI TOOLS");
        let mut tools: Vec<_> = git.ai_tools.iter().collect();
        tools.sort_by(|a, b| b.1.cmp(&a.1));
        for (tool, count) in &tools {
            let pct = (*count as f64 / git.total_commits.max(1) as f64) * 100.0;
            kv_indent(&tool.to_string(), &format!("{} ({:.0}%)", count, pct));
        }
        blank();
    }

    // ── Project Stats ──
    section("PROJECT");
    if project.deps.total > 0 {
        kv(
            "Dependencies",
            &format!("{} ({})", project.deps.total, project.deps.manager),
        );
    } else {
        kv("Dependencies", "0");
    }

    let test_str = if project.tests.has_tests {
        let fw = if project.tests.frameworks.is_empty() {
            String::new()
        } else {
            format!(" [{}]", project.tests.frameworks.join(", "))
        };
        format!("{} files{}", project.tests.test_files_count, fw)
    } else {
        "none".to_string()
    };
    kv("Tests", &test_str);
    kv("Lines of code", &fmt_num(project.languages.total_lines));

    // ── Top Languages ──
    let mut langs: Vec<_> = project.languages.languages.iter().collect();
    langs.sort_by(|a, b| b.1.cmp(a.1));
    if !langs.is_empty() {
        blank();
        section("LANGUAGES");
        for (lang, lines) in langs.iter().take(5) {
            let pct = (**lines as f64 / project.languages.total_lines.max(1) as f64) * 100.0;
            lang_row(lang, pct);
        }
    }

    // ── Timeline ──
    let timeline = build_timeline(&git.commits);
    if timeline.len() >= 2 {
        blank();
        render_timeline_chart(&timeline);
    }

    // ── Security ──
    if project.security.env_in_git || project.security.hardcoded_secrets_hints > 0 {
        blank();
        section("SECURITY");
        if project.security.env_files_count > 0 {
            let env_msg = if project.security.env_files_count == 1 {
                ".env committed to git!".to_string()
            } else {
                format!(
                    "{} .env files committed to git!",
                    project.security.env_files_count
                )
            };
            warning_line(&env_msg);
        }
        if project.security.hardcoded_secrets_hints > 0 {
            warning_line(&format!(
                "{} hardcoded secret(s) detected",
                project.security.hardcoded_secrets_hints
            ));
        }
    }

    blank();
    separator();
    blank();

    // ── Score ──
    score_line(&score.grade, score.points);
    blank();
    roast_line(&score.roast);

    blank();
    border_bot();
    println!();
}

/// Render a full vibe report (without explicit repo name).
#[allow(dead_code)]
pub fn render(git: &GitStats, project: &ProjectStats, score: &VibeScore) {
    render_with_name(git, project, score, "");
}

/// Render a multi-repo summary table.
pub fn render_multi(report: &crate::scanner::multi_report::MultiReport) {
    println!();
    println!(
        "  {} {}",
        "YOUR DEV LIFE — Vibe Report".bold().white(),
        format!("({} repos)", report.repos.len()).dimmed()
    );
    println!();

    // Column headers
    println!(
        "  {:<25} {:>5}  {:>5}  {}",
        "REPO".dimmed(),
        "AI%".dimmed(),
        "SCORE".dimmed(),
        "ROAST".dimmed()
    );
    println!("  {}", "\u{2500}".repeat(70).bright_black());

    // Sort repos by score descending
    let mut sorted: Vec<_> = report.repos.iter().collect();
    sorted.sort_by(|a, b| b.score.points.cmp(&a.score.points));

    for repo in &sorted {
        let ai_pct = format!("{:.0}%", repo.score.ai_ratio * 100.0);
        let grade = &repo.score.grade;
        let roast_short = if repo.score.roast.chars().count() > 35 {
            let truncated: String = repo.score.roast.chars().take(32).collect();
            format!("\"{}...\"", truncated)
        } else {
            format!("\"{}\"", repo.score.roast)
        };
        println!(
            "  {:<25} {:>5}  {:>5}  {}",
            repo.name.white().bold(),
            ai_pct.cyan(),
            grade.yellow().bold(),
            roast_short.dimmed()
        );
    }

    // Global summary
    println!();
    println!("  {}", "\u{2500}".repeat(70).bright_black());
    let global_summary = format!(
        "GLOBAL: {:.0}% AI | {} lines | Avg Score: {} ({})",
        report.global_ai_ratio * 100.0,
        fmt_num(report.total_lines),
        grade_from_points(report.average_score),
        report.average_score
    );
    println!("  {}", global_summary.bold().white());
    println!();
}

/// Convert numeric points to a letter grade.
fn grade_from_points(points: u32) -> &'static str {
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
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  Display width calculation
//
//  We need to know how many terminal columns a string occupies,
//  ignoring ANSI codes. Unicode box-drawing chars = 1 col each.
//  Emoji = 2 cols. Variation selectors = 0 cols. ASCII = 1 col.
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn display_width(s: &str) -> usize {
    let mut w = 0;
    for ch in s.chars() {
        match ch {
            // Variation selectors / zero-width joiners / combining marks
            '\u{FE00}'..='\u{FE0F}' | '\u{200D}' | '\u{20E3}' => {}
            // Common emoji ranges (simplified — covers most we use)
            '\u{1F300}'..='\u{1F9FF}' | '\u{2600}'..='\u{27BF}' | '\u{2B50}'..='\u{2B55}' => {
                w += 2;
            }
            // Regional indicators, tags, etc
            '\u{1F1E0}'..='\u{1F1FF}' => {
                w += 2;
            }
            // Box-drawing, regular ASCII, Latin
            _ if ch.is_ascii() => {
                w += 1;
            }
            // CJK characters
            '\u{2E80}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '\u{FE30}'..='\u{FE4F}' => {
                w += 2;
            }
            // Full-width forms
            '\u{FF01}'..='\u{FF60}' | '\u{FFE0}'..='\u{FFE6}' => {
                w += 2;
            }
            // Box-drawing characters (U+2500..U+257F) = 1 col
            '\u{2500}'..='\u{257F}' => {
                w += 1;
            }
            // Most other Unicode = 1 col (Latin extended, etc.)
            _ => {
                w += 1;
            }
        }
    }
    w
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  Low-level line builders
//
//  Every line is exactly:
//     "  " + border_left + <W display-columns of content> + border_right
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn border_top() {
    println!(
        "  {}",
        format!("\u{256D}{}\u{256E}", "\u{2500}".repeat(W)).cyan()
    );
}

fn border_bot() {
    println!(
        "  {}",
        format!("\u{2570}{}\u{256F}", "\u{2500}".repeat(W)).cyan()
    );
}

fn separator() {
    println!(
        "  {}",
        format!("\u{251C}{}\u{2524}", "\u{2500}".repeat(W)).cyan()
    );
}

fn blank() {
    println!(
        "  {}{}{}",
        "\u{2502}".cyan(),
        " ".repeat(W),
        "\u{2502}".cyan()
    );
}

// ── Content line builders ─────────────────────────────────────────

fn center_bold(text: &str) {
    let dw = display_width(text);
    let lp = (W.saturating_sub(dw)) / 2;
    let rp = W.saturating_sub(dw).saturating_sub(lp);
    println!(
        "  {}{}{}{}{}",
        "\u{2502}".cyan(),
        " ".repeat(lp),
        text.bold().white(),
        " ".repeat(rp),
        "\u{2502}".cyan(),
    );
}

fn center_dimmed(text: &str) {
    let dw = display_width(text);
    let lp = (W.saturating_sub(dw)) / 2;
    let rp = W.saturating_sub(dw).saturating_sub(lp);
    println!(
        "  {}{}{}{}{}",
        "\u{2502}".cyan(),
        " ".repeat(lp),
        text.dimmed(),
        " ".repeat(rp),
        "\u{2502}".cyan(),
    );
}

fn section(label: &str) {
    // Display: "   -- LABEL"
    let prefix_display = "   \u{2500}\u{2500} ";
    let dw = display_width(prefix_display) + display_width(label);
    let rp = W.saturating_sub(dw);
    println!(
        "  {}{}{}{}",
        "\u{2502}".cyan(),
        format!("   \u{2500}\u{2500} {}", label).cyan().bold(),
        " ".repeat(rp),
        "\u{2502}".cyan(),
    );
}

fn kv(label: &str, value: &str) {
    // Layout: "   {label}  {dots}  {value}  "
    //          ^3          ^2      ^2       ^2 = margins
    let ml = 3_usize;
    let mr = 2_usize;
    let gap = 2_usize;
    let label_w = display_width(label);
    let value_w = display_width(value);
    let fixed = ml + label_w + gap + gap + value_w + mr;
    let ndots = W.saturating_sub(fixed).max(1);

    println!(
        "  {}{}{}  {}  {}{}{}",
        "\u{2502}".cyan(),
        " ".repeat(ml),
        label.dimmed(),
        ".".repeat(ndots).bright_black(),
        value.white().bold(),
        " ".repeat(mr),
        "\u{2502}".cyan(),
    );
}

fn kv_indent(label: &str, value: &str) {
    let ml = 5_usize;
    let mr = 2_usize;
    let gap = 2_usize;
    let label_w = display_width(label);
    let value_w = display_width(value);
    let fixed = ml + label_w + gap + gap + value_w + mr;
    let ndots = W.saturating_sub(fixed).max(1);

    println!(
        "  {}{}{}  {}  {}{}{}",
        "\u{2502}".cyan(),
        " ".repeat(ml),
        label.dimmed(),
        ".".repeat(ndots).bright_black(),
        value.white().bold(),
        " ".repeat(mr),
        "\u{2502}".cyan(),
    );
}

fn lang_row(lang: &str, pct: f64) {
    // Layout: "     {lang:<14} {bar:12} {pct:>6}  "
    let ml = 5_usize;
    let mr = 2_usize;
    let lang_col = 14_usize;
    let bar_w = 12_usize;
    let pct_str = format!("{:>5.1}%", pct);
    let pct_w = pct_str.len(); // ASCII, so len == display width

    // Total used display columns
    let used = ml + lang_col + 1 + bar_w + 1 + pct_w + mr;
    let extra = W.saturating_sub(used);

    let filled = ((pct / 100.0) * bar_w as f64).round() as usize;
    let empty_count = bar_w.saturating_sub(filled);
    let bar_filled = "\u{2588}".repeat(filled);
    let bar_empty = "\u{2591}".repeat(empty_count);

    // Pad lang name to `lang_col` display columns
    let lang_dw = display_width(lang);
    let lang_pad = lang_col.saturating_sub(lang_dw);

    println!(
        "  {}{}{}{} {}{} {}{}{}",
        "\u{2502}".cyan(),
        " ".repeat(ml),
        lang.white(),
        " ".repeat(lang_pad),
        bar_filled.green(),
        bar_empty.bright_black(),
        pct_str.dimmed(),
        " ".repeat(mr + extra),
        "\u{2502}".cyan(),
    );
}

fn warning_line(msg: &str) {
    let prefix_dw = 3 + 3; // "   " + "!! "
    let msg_dw = display_width(msg);
    let rp = W.saturating_sub(prefix_dw + msg_dw);
    println!(
        "  {}   {}{}{}{}",
        "\u{2502}".cyan(),
        "!! ".bright_red().bold(),
        msg.bright_red(),
        " ".repeat(rp),
        "\u{2502}".cyan(),
    );
}

fn score_line(grade: &str, points: u32) {
    let text = format!("VIBE SCORE: {} ({}pts)", grade, points);
    let dw = display_width(&text);
    let lp = (W.saturating_sub(dw)) / 2;
    let rp = W.saturating_sub(dw).saturating_sub(lp);
    println!(
        "  {}{}{}{}{}",
        "\u{2502}".cyan(),
        " ".repeat(lp),
        text.bold().yellow(),
        " ".repeat(rp),
        "\u{2502}".cyan(),
    );
}

fn roast_line(roast: &str) {
    let text = format!("\"{}\"", roast);
    let dw = display_width(&text);
    let lp = (W.saturating_sub(dw)) / 2;
    let rp = W.saturating_sub(dw).saturating_sub(lp);
    println!(
        "  {}{}{}{}{}",
        "\u{2502}".cyan(),
        " ".repeat(lp),
        text.italic().dimmed(),
        " ".repeat(rp),
        "\u{2502}".cyan(),
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  Timeline bar chart
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

const MONTH_NAMES: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Number of rows in the bar chart (0%, 20%, 40%, 60%, 80%, 100%).
const CHART_ROWS: usize = 6;

/// Maximum number of months to display (latest N if more data).
const MAX_MONTHS: usize = 12;

/// Render a vertical bar chart of AI% per month inside the box.
///
/// Layout within W=52 content columns:
///
///   "  100% │ ██ ██ ██ ██ ██ ██                    "
///    ^^     ^ ^                                     ^
///    2      6 8  each bar=2 + 1 space = 3 chars
///
/// Y-axis label: 6 chars right-aligned ("  100%")
/// Separator: " │ " = 3 chars
/// Prefix total: 9 display columns
/// Bars area: up to MAX_MONTHS * 3 chars
/// Right padding fills the rest to W.
fn render_timeline_chart(timeline: &[MonthlyStats]) {
    // Take at most MAX_MONTHS (latest months).
    let months: &[MonthlyStats] = if timeline.len() > MAX_MONTHS {
        &timeline[timeline.len() - MAX_MONTHS..]
    } else {
        timeline
    };

    let n = months.len();
    // prefix = "  100% │ " = 9 display columns
    let prefix_w: usize = 9;
    let bars_w: usize = n * 3; // each bar = "██ " (3 cols), last one has trailing space too
    let total_content = prefix_w + bars_w;
    let right_pad = W.saturating_sub(total_content);

    section("TIMELINE");

    // Y-axis thresholds: 100, 80, 60, 40, 20, 0
    for row in 0..CHART_ROWS {
        let threshold = (CHART_ROWS - 1 - row) as f64 * 20.0; // 100, 80, 60, 40, 20, 0
        let label = format!("{:>4.0}%", threshold);

        // Build the bars string (uncolored) and colored parts separately
        let mut bar_segments: Vec<String> = Vec::with_capacity(n);
        for (i, m) in months.iter().enumerate() {
            let pct = m.ai_ratio * 100.0;
            let filled = pct >= threshold + 0.5; // round: show block if ai% >= threshold
            let block = if filled { "\u{2588}\u{2588}" } else { "  " };
            // Color gradient: alternate green and cyan by month index
            let colored = if filled {
                if i % 2 == 0 {
                    format!("{}", block.green())
                } else {
                    format!("{}", block.cyan())
                }
            } else {
                block.to_string()
            };
            bar_segments.push(format!("{} ", colored));
        }

        let bars_str = bar_segments.concat();

        println!(
            "  {}{} {} {}{}{}",
            "\u{2502}".cyan(),
            format!("  {}", label).dimmed(),
            "\u{2502}".bright_black(),
            bars_str,
            " ".repeat(right_pad),
            "\u{2502}".cyan(),
        );
    }

    // X-axis line: "        └─────..."
    // prefix area: 8 chars for "        " then "└" then "─" repeated
    let axis_line_w = bars_w;
    let axis_prefix = "        \u{2514}";
    let axis_dashes = "\u{2500}".repeat(axis_line_w);
    let axis_dw = display_width(axis_prefix) + axis_line_w;
    let axis_rp = W.saturating_sub(axis_dw);
    println!(
        "  {}{}{}{}",
        "\u{2502}".cyan(),
        format!("{}{}", axis_prefix, axis_dashes).bright_black(),
        " ".repeat(axis_rp),
        "\u{2502}".cyan(),
    );

    // Month labels row
    let mut labels = String::new();
    for m in months {
        let name = MONTH_NAMES[(m.month as usize).saturating_sub(1).min(11)];
        labels.push_str(&format!("{:<3}", name));
    }
    let labels_prefix = "         "; // 9 spaces to align under bars
    let labels_dw = display_width(labels_prefix) + display_width(&labels);
    let labels_rp = W.saturating_sub(labels_dw);
    println!(
        "  {}{}{}{}{}",
        "\u{2502}".cyan(),
        labels_prefix,
        labels.dimmed(),
        " ".repeat(labels_rp),
        "\u{2502}".cyan(),
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  Utilities
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn fmt_num(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

fn emoji_for_grade(grade: &str) -> &'static str {
    match grade {
        "S+" => "\u{1F451}\u{1F525}\u{1F525}",
        "S" => "\u{1F525}\u{1F525}\u{1F525}",
        "A+" => "\u{1F525}\u{1F525}",
        "A" => "\u{1F525}",
        "B+" => "\u{26A1}",
        "B" => "\u{1F916}",
        "C+" => "\u{1F6E0}\u{FE0F}",
        "C" => "\u{1F331}",
        "D" => "\u{270D}\u{FE0F}",
        _ => "\u{1F9D1}\u{200D}\u{1F4BB}",
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  Tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_num_works() {
        assert_eq!(fmt_num(0), "0");
        assert_eq!(fmt_num(500), "500");
        assert_eq!(fmt_num(1000), "1.0K");
        assert_eq!(fmt_num(1500), "1.5K");
        assert_eq!(fmt_num(10000), "10.0K");
        assert_eq!(fmt_num(1_500_000), "1.5M");
    }

    #[test]
    fn emoji_for_every_grade() {
        for g in &["S+", "S", "A+", "A", "B+", "B", "C+", "C", "D", "F"] {
            assert!(!emoji_for_grade(g).is_empty());
        }
    }

    #[test]
    fn display_width_ascii() {
        assert_eq!(display_width("hello"), 5);
        assert_eq!(display_width(""), 0);
        assert_eq!(display_width("  "), 2);
    }

    #[test]
    fn display_width_box_drawing() {
        assert_eq!(display_width("\u{2500}"), 1); // ─
        assert_eq!(display_width("\u{256D}"), 1); // ╭
        assert_eq!(display_width("\u{2502}"), 1); // │
    }

    #[test]
    fn display_width_emoji() {
        assert_eq!(display_width("\u{1F525}"), 2); // fire
        assert_eq!(display_width("\u{26A1}"), 2); // lightning
        assert_eq!(display_width("\u{270D}\u{FE0F}"), 2); // writing hand + VS16
    }
}
