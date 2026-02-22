use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use chrono::Datelike;
use futures::stream::{self, StreamExt};
use regex::Regex;
use serde::Deserialize;
use std::sync::{Arc, LazyLock};
use subtle::ConstantTimeEq;
use tokio::sync::Semaphore;
use uuid::Uuid;

// FIX 1: Regex patterns for repo URL validation
static GITHUB_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^https://github\.com/[a-zA-Z0-9_.-]+/[a-zA-Z0-9_.-]+(\.git)?$").unwrap()
});
static REPO_SLUG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9_.-]+/[a-zA-Z0-9_.-]+$").unwrap());

// FIX 3: Regex for since date validation
static SINCE_DATE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap());

struct AppState {
    user_semaphore: Semaphore,  // 2 slots for user web scans
    index_semaphore: Semaphore, // 3 slots for index cron
    auth_token: String,
    vibereport_bin: String,
    api_url: String, // FIX 2: api_url from env, not from request
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
    // Auth check (FIX 4: constant-time comparison)
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let expected = format!("Bearer {}", state.auth_token);
    if auth.as_bytes().ct_eq(expected.as_bytes()).unwrap_u8() != 1 {
        return Err((StatusCode::UNAUTHORIZED, "Invalid token".into()));
    }

    // Acquire user semaphore (max 2 concurrent user scans)
    let _permit = state.user_semaphore.acquire().await.map_err(|_| {
        (
            StatusCode::TOO_MANY_REQUESTS,
            "Too many concurrent scans".into(),
        )
    })?;

    let uuid = Uuid::new_v4().to_string();
    let tmp_dir = format!("/tmp/vibereport-{}", uuid);

    // FIX 3: Validate since parameter
    let since = req.since.unwrap_or_else(|| "2025-01-01".into());
    if !SINCE_DATE_RE.is_match(&since) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid since format, expected YYYY-MM-DD".into(),
        ));
    }

    // FIX 1: Parse repo with strict validation
    let repo_url = if req.repo.starts_with("http") {
        if !GITHUB_URL_RE.is_match(&req.repo) {
            return Err((
                StatusCode::BAD_REQUEST,
                "Invalid repo URL: must be https://github.com/{user}/{repo}".into(),
            ));
        }
        req.repo.clone()
    } else {
        let cleaned = req.repo.replace("github:", "");
        if !REPO_SLUG_RE.is_match(&cleaned) {
            return Err((
                StatusCode::BAD_REQUEST,
                "Invalid repo slug: must be {user}/{repo}".into(),
            ));
        }
        format!("https://github.com/{}.git", cleaned)
    };

    // Clone
    let clone_result = tokio::process::Command::new("git")
        .args([
            "clone",
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
        // FIX 5: Log stderr, return generic message
        let stderr = String::from_utf8_lossy(&clone_result.stderr);
        eprintln!("Clone failed for {}: {}", repo_url, stderr);
        return Err((
            StatusCode::BAD_REQUEST,
            "Clone failed: repository not accessible".into(),
        ));
    }

    // Run vibereport
    let analyze_result = tokio::process::Command::new(&state.vibereport_bin)
        .args([&tmp_dir, "--json", "--since", &since, "--no-share"])
        .output()
        .await
        .map_err(|e| {
            // FIX 7: Use tokio::fs in async context (spawn blocking cleanup)
            let tmp = tmp_dir.clone();
            tokio::spawn(async move {
                let _ = tokio::fs::remove_dir_all(&tmp).await;
            });
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Analysis failed: {}", e),
            )
        })?;

    // Cleanup
    let _ = tokio::fs::remove_dir_all(&tmp_dir).await;

    if !analyze_result.status.success() {
        // FIX 5: Log stderr, return generic message
        let stderr = String::from_utf8_lossy(&analyze_result.stderr);
        eprintln!("Analysis failed for {}: {}", repo_url, stderr);
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "Analysis failed".into()));
    }

    let stdout = String::from_utf8_lossy(&analyze_result.stdout);
    let data: serde_json::Value = serde_json::from_str(&stdout).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Parse error: {}", e),
        )
    })?;

    Ok(Json(data))
}

// ── Index scan types ──

// FIX 2: Removed api_url from IndexScanRequest
#[derive(Deserialize)]
struct IndexScanRequest {}

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
    Json(_req): Json<IndexScanRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Auth check (FIX 4: constant-time comparison)
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let expected = format!("Bearer {}", state.auth_token);
    if auth.as_bytes().ct_eq(expected.as_bytes()).unwrap_u8() != 1 {
        return Err((StatusCode::UNAUTHORIZED, "Invalid token".into()));
    }

    // FIX 2: Use api_url from state instead of request
    let api_url = state.api_url.clone();
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

    let repo_count = repos.len();
    tracing::info!("Index scan starting: {} repos for {}", repo_count, quarter);

    // Fire-and-forget: spawn background task, return immediately
    // (Cloudflare Tunnel has ~100s timeout, scan takes ~30min)
    let auth_token = state.auth_token.clone();
    let vibereport_bin = state.vibereport_bin.clone();
    let state_clone = Arc::clone(&state);

    tokio::spawn(async move {
        // Pass 1: normal timeouts (120s clone, 60s analysis)
        let scanned: Vec<(String, Option<RepoScanResult>)> = stream::iter(repos)
            .map(|slug| {
                let sem = &state_clone.index_semaphore;
                let bin = vibereport_bin.clone();
                async move {
                    let _permit = sem.acquire().await.ok()?;
                    let result = scan_single_repo(&slug, &bin, 120, 60).await;
                    Some((slug, result))
                }
            })
            .buffer_unordered(3)
            .filter_map(|r| async { r })
            .collect()
            .await;

        let mut results: Vec<RepoScanResult> = Vec::new();
        let mut failed_slugs: Vec<String> = Vec::new();
        for (slug, result) in scanned {
            match result {
                Some(r) => results.push(r),
                None => failed_slugs.push(slug),
            }
        }

        // Pass 2: retry failed repos with doubled timeouts (240s clone, 120s analysis)
        if !failed_slugs.is_empty() {
            tracing::info!(
                "Retrying {}/{} failed repos with extended timeouts",
                failed_slugs.len(),
                repo_count
            );

            let retry_results: Vec<RepoScanResult> = stream::iter(failed_slugs)
                .map(|slug| {
                    let sem = &state_clone.index_semaphore;
                    let bin = vibereport_bin.clone();
                    async move {
                        let _permit = sem.acquire().await.ok()?;
                        scan_single_repo(&slug, &bin, 240, 120).await
                    }
                })
                .buffer_unordered(3)
                .filter_map(|r| async { r })
                .collect()
                .await;

            tracing::info!("Retry recovered {} repos", retry_results.len());
            results.extend(retry_results);
        }

        let scan_date = chrono::Utc::now().format("%Y-%m-%d").to_string();
        tracing::info!(
            "Index scan complete: {}/{} repos scanned, posting results",
            results.len(),
            repo_count
        );

        // Post results back to CF API
        let client = reqwest::Client::new();
        let post_body = serde_json::json!({
            "scan_date": scan_date,
            "results": results,
        });

        match client
            .post(format!("{}/api/index-results", api_url))
            .header("Authorization", format!("Bearer {}", auth_token))
            .json(&post_body)
            .send()
            .await
        {
            Ok(res) => {
                let status = res.status();
                let body: serde_json::Value = res.json().await.unwrap_or_default();
                tracing::info!("Index results posted: status={}, body={}", status, body);
            }
            Err(e) => {
                tracing::error!("Failed to post index results: {}", e);
            }
        }
    });

    Ok(Json(serde_json::json!({
        "status": "started",
        "repos": repo_count,
        "quarter": quarter,
    })))
}

// ── Single repo scanner for index ──

async fn scan_single_repo(
    slug: &str,
    vibereport_bin: &str,
    clone_timeout_secs: u64,
    analyze_timeout_secs: u64,
) -> Option<RepoScanResult> {
    let uuid = Uuid::new_v4().to_string();
    let tmp_dir = format!("/tmp/vibereport-idx-{}", uuid);
    let repo_url = format!("https://github.com/{}.git", slug);

    let clone_fut = tokio::process::Command::new("git")
        .args(["clone", "--shallow-since=2026-01-01", &repo_url, &tmp_dir])
        .output();

    let clone = match tokio::time::timeout(
        std::time::Duration::from_secs(clone_timeout_secs),
        clone_fut,
    )
    .await
    {
        Ok(result) => result.ok()?,
        Err(_) => {
            let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
            tracing::warn!("Clone timed out for {} ({}s)", slug, clone_timeout_secs);
            return None;
        }
    };

    if !clone.status.success() {
        let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
        tracing::warn!("Clone failed for {}", slug);
        return None;
    }

    let analyze_fut = tokio::process::Command::new(vibereport_bin)
        .args([&tmp_dir, "--json", "--since", "2026-01-01", "--no-share"])
        .output();

    let analyze = match tokio::time::timeout(
        std::time::Duration::from_secs(analyze_timeout_secs),
        analyze_fut,
    )
    .await
    {
        Ok(result) => result.ok()?,
        Err(_) => {
            let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
            tracing::warn!(
                "Analysis timed out for {} ({}s)",
                slug,
                analyze_timeout_secs
            );
            return None;
        }
    };

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
    let vibereport_bin = std::env::var("VIBEREPORT_BIN").unwrap_or_else(|_| "vibereport".into());
    // FIX 2: Read API_URL from environment
    let api_url = std::env::var("API_URL")
        .unwrap_or_else(|_| "https://vibereport-api.clement-serizay.workers.dev".into());

    let state = Arc::new(AppState {
        user_semaphore: Semaphore::new(2),
        index_semaphore: Semaphore::new(3),
        auth_token,
        vibereport_bin,
        api_url,
    });

    let app = Router::new()
        .route("/scan", post(scan_handler))
        .route("/index-scan", post(index_scan_handler))
        .with_state(state);

    // FIX 6: Bind to 127.0.0.1 (cloudflared runs on the same machine)
    let addr = format!("127.0.0.1:{}", port);
    tracing::info!("VPS worker listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
