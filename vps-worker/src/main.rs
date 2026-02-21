use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use chrono::Datelike;
use futures::stream::{self, StreamExt};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::Semaphore;
use uuid::Uuid;

struct AppState {
    user_semaphore: Semaphore,  // 2 slots for user web scans
    index_semaphore: Semaphore, // 3 slots for index cron
    auth_token: String,
    vibereport_bin: String,
}

#[derive(Deserialize)]
struct ScanRequest {
    repo: String,
    since: Option<String>,
}

async fn scan_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ScanRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Auth check
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if auth != format!("Bearer {}", state.auth_token) {
        return Err((StatusCode::UNAUTHORIZED, "Invalid token".into()));
    }

    // Acquire user semaphore (max 2 concurrent user scans)
    let _permit = state
        .user_semaphore
        .acquire()
        .await
        .map_err(|_| (StatusCode::TOO_MANY_REQUESTS, "Too many concurrent scans".into()))?;

    let uuid = Uuid::new_v4().to_string();
    let tmp_dir = format!("/tmp/vibereport-{}", uuid);
    let since = req.since.unwrap_or_else(|| "2025-01-01".into());

    // Parse repo: "user/repo" or "github:user/repo" or URL
    let repo_url = if req.repo.starts_with("http") {
        req.repo.clone()
    } else {
        let cleaned = req.repo.replace("github:", "");
        format!("https://github.com/{}.git", cleaned)
    };

    // Clone
    let clone_result = tokio::process::Command::new("git")
        .args([
            "clone",
            "--bare",
            &format!("--shallow-since={}", since),
            &repo_url,
            &tmp_dir,
        ])
        .output()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Clone failed: {}", e),
            )
        })?;

    if !clone_result.status.success() {
        let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
        let stderr = String::from_utf8_lossy(&clone_result.stderr);
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Clone failed: {}", stderr),
        ));
    }

    // Run vibereport
    let analyze_result = tokio::process::Command::new(&state.vibereport_bin)
        .args([&tmp_dir, "--json", "--since", &since, "--no-share"])
        .output()
        .await
        .map_err(|e| {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Analysis failed: {}", e),
            )
        })?;

    // Cleanup
    let _ = tokio::fs::remove_dir_all(&tmp_dir).await;

    if !analyze_result.status.success() {
        let stderr = String::from_utf8_lossy(&analyze_result.stderr);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Analysis failed: {}", stderr),
        ));
    }

    let stdout = String::from_utf8_lossy(&analyze_result.stdout);
    let data: serde_json::Value = serde_json::from_str(&stdout)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Parse error: {}", e)))?;

    Ok(Json(data))
}

// ── Index scan types ──

#[derive(Deserialize)]
struct IndexScanRequest {
    api_url: String,
}

#[derive(serde::Serialize)]
struct RepoScanResult {
    repo_slug: String,
    total_commits: u64,
    ai_commits: u64,
}

// ── Index scan handler ──

async fn index_scan_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<IndexScanRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Auth check
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if auth != format!("Bearer {}", state.auth_token) {
        return Err((StatusCode::UNAUTHORIZED, "Invalid token".into()));
    }

    let api_url = req.api_url;
    let quarter = get_current_quarter();

    // 1. Fetch panel from CF API
    let client = reqwest::Client::new();
    let panel_res = client
        .get(format!("{}/api/index-panel?quarter={}", api_url, quarter))
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to fetch panel: {}", e),
            )
        })?;

    let panel: serde_json::Value = panel_res.json().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Panel parse error: {}", e),
        )
    })?;

    let repos: Vec<String> = panel["repos"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|r| r["repo_slug"].as_str().map(String::from))
        .collect();

    if repos.is_empty() {
        return Ok(Json(
            serde_json::json!({ "error": "No repos in panel", "quarter": quarter }),
        ));
    }

    tracing::info!(
        "Index scan starting: {} repos for {}",
        repos.len(),
        quarter
    );

    // 2. Scan repos with index semaphore (3 concurrent via buffer_unordered + semaphore)
    let results: Vec<RepoScanResult> = stream::iter(repos)
        .map(|slug| {
            let sem = &state.index_semaphore;
            let bin = state.vibereport_bin.clone();
            async move {
                let _permit = sem.acquire().await.ok()?;
                scan_single_repo(&slug, &bin).await
            }
        })
        .buffer_unordered(3)
        .filter_map(|r| async { r })
        .collect()
        .await;

    let scan_date = chrono::Utc::now().format("%Y-%m-%d").to_string();

    // 3. Post results back to CF API
    let post_body = serde_json::json!({
        "scan_date": scan_date,
        "results": results,
    });

    let post_res = client
        .post(format!("{}/api/index-results", api_url))
        .header("Authorization", format!("Bearer {}", state.auth_token))
        .json(&post_body)
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to post results: {}", e),
            )
        })?;

    let response: serde_json::Value = post_res.json().await.unwrap_or_default();

    tracing::info!("Index scan complete: {} repos scanned", results.len());

    Ok(Json(serde_json::json!({
        "scanned": results.len(),
        "scan_date": scan_date,
        "api_response": response,
    })))
}

// ── Single repo scanner for index ──

async fn scan_single_repo(slug: &str, vibereport_bin: &str) -> Option<RepoScanResult> {
    let uuid = Uuid::new_v4().to_string();
    let tmp_dir = format!("/tmp/vibereport-idx-{}", uuid);
    let repo_url = format!("https://github.com/{}.git", slug);

    // Clone
    let clone = tokio::process::Command::new("git")
        .args([
            "clone",
            "--bare",
            "--shallow-since=2026-01-01",
            &repo_url,
            &tmp_dir,
        ])
        .output()
        .await
        .ok()?;

    if !clone.status.success() {
        let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
        tracing::warn!("Clone failed for {}", slug);
        return None;
    }

    // Analyze
    let analyze = tokio::process::Command::new(vibereport_bin)
        .args([&tmp_dir, "--json", "--since", "2026-01-01", "--no-share"])
        .output()
        .await
        .ok()?;

    let _ = tokio::fs::remove_dir_all(&tmp_dir).await;

    if !analyze.status.success() {
        tracing::warn!("Analysis failed for {}", slug);
        return None;
    }

    let stdout = String::from_utf8_lossy(&analyze.stdout);
    let data: serde_json::Value = serde_json::from_str(&stdout).ok()?;

    Some(RepoScanResult {
        repo_slug: slug.to_string(),
        total_commits: data["total_commits"].as_u64().unwrap_or(0),
        ai_commits: data["ai_commits"].as_u64().unwrap_or(0),
    })
}

// ── Quarter helper ──

fn get_current_quarter() -> String {
    let now = chrono::Utc::now();
    let q = (now.month() - 1) / 3 + 1;
    format!("{}-Q{}", now.year(), q)
}

// ── Main ──

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let port = std::env::var("PORT").unwrap_or_else(|_| "3001".into());
    let auth_token = std::env::var("AUTH_TOKEN").expect("AUTH_TOKEN required");
    let vibereport_bin =
        std::env::var("VIBEREPORT_BIN").unwrap_or_else(|_| "vibereport".into());

    let state = Arc::new(AppState {
        user_semaphore: Semaphore::new(2),
        index_semaphore: Semaphore::new(3),
        auth_token,
        vibereport_bin,
    });

    let app = Router::new()
        .route("/scan", post(scan_handler))
        .route("/index-scan", post(index_scan_handler))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("VPS worker listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
