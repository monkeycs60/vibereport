use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct ReportPayload {
    pub github_username: Option<String>,
    pub repo_name: Option<String>,
    pub ai_ratio: f64,
    pub ai_tool: String,
    pub score_points: u32,
    pub score_grade: String,
    pub roast: String,
    pub deps_count: usize,
    pub has_tests: bool,
    pub total_lines: usize,
    pub languages: String, // JSON string
    pub repo_fingerprint: Option<String>,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
pub struct ShareResponse {
    pub id: String,
    pub url: String,
    pub rank: Option<u64>,
    pub percentile: Option<f64>,
}

const API_URL: &str = "https://api.vibereport.dev";

/// Upload a report to the vibereport.dev API.
/// Returns the share URL and leaderboard rank.
pub fn upload_report(payload: &ReportPayload) -> Result<ShareResponse, Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let resp = client
        .post(format!("{}/api/reports", API_URL))
        .json(payload)
        .send()?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(format!("API error ({}): {}", status, body).into());
    }

    let share_resp = resp.json::<ShareResponse>()?;
    Ok(share_resp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_serializes_correctly() {
        let payload = ReportPayload {
            github_username: Some("testuser".into()),
            repo_name: Some("my-repo".into()),
            ai_ratio: 0.75,
            ai_tool: "Claude Code".into(),
            score_points: 67,
            score_grade: "B+".into(),
            roast: "Ships fast, tests never".into(),
            deps_count: 42,
            has_tests: false,
            total_lines: 5000,
            languages: r#"{"TypeScript":3000,"Rust":2000}"#.into(),
            repo_fingerprint: Some("abc123:https://github.com/user/repo.git".into()),
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["ai_ratio"], 0.75);
        assert_eq!(json["score_grade"], "B+");
        assert_eq!(json["has_tests"], false);
        assert!(json["github_username"].is_string());
    }

    #[test]
    fn payload_with_none_fields() {
        let payload = ReportPayload {
            github_username: None,
            repo_name: None,
            ai_ratio: 0.0,
            ai_tool: "Human".into(),
            score_points: 10,
            score_grade: "F".into(),
            roast: "Write code like it's 2019.".into(),
            deps_count: 0,
            has_tests: true,
            total_lines: 100,
            languages: "{}".into(),
            repo_fingerprint: None,
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert!(json["github_username"].is_null());
        assert!(json["repo_name"].is_null());
        assert!(json["repo_fingerprint"].is_null());
    }

    #[test]
    fn payload_includes_fingerprint_in_json() {
        let fingerprint =
            "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2:https://github.com/user/repo.git";
        let payload = ReportPayload {
            github_username: Some("user".into()),
            repo_name: Some("repo".into()),
            ai_ratio: 0.5,
            ai_tool: "Claude Code".into(),
            score_points: 50,
            score_grade: "C".into(),
            roast: "Mid.".into(),
            deps_count: 10,
            has_tests: true,
            total_lines: 1000,
            languages: "{}".into(),
            repo_fingerprint: Some(fingerprint.into()),
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["repo_fingerprint"], fingerprint);
    }
}
