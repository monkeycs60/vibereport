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
struct IndexScanRequest {
    /// Optional list of scan dates (YYYY-MM-DD) to post results for.
    scan_dates: Option<Vec<String>>,
    /// Alternative: generate all dates in [from_date, to_date] range (inclusive).
    from_date: Option<String>,
    to_date: Option<String>,
}

#[derive(serde::Serialize, Clone)]
struct RepoScanResult {
    repo_slug: String,
    total_commits: u64,
    ai_commits: u64,
}

/// Per-repo daily commit breakdown (from vibereport --json daily_commits field).
#[derive(Clone)]
struct RepoDailyBreakdown {
    repo_slug: String,
    /// Non-cumulative daily counts, sorted oldest-first: [{date, total, ai}, ...]
    days: Vec<DayEntry>,
}

#[derive(serde::Deserialize, Clone)]
struct DayEntry {
    date: String,
    total: u64,
    ai: u64,
}

// ── Index scan handler ──

async fn index_scan_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<IndexScanRequest>,
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

    // Build scan_dates: from_date/to_date range > explicit scan_dates > today
    let scan_dates: Vec<String> =
        if let (Some(from), Some(to)) = (req.from_date.as_deref(), req.to_date.as_deref()) {
            if !SINCE_DATE_RE.is_match(from) || !SINCE_DATE_RE.is_match(to) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "from_date/to_date must match YYYY-MM-DD".into(),
                ));
            }
            generate_date_range(from, to).ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    "Invalid date range (check dates are valid and from <= to)".into(),
                )
            })?
        } else if let Some(dates) = req.scan_dates {
            for d in &dates {
                if !SINCE_DATE_RE.is_match(d) {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        format!("Invalid scan_date format: {}, expected YYYY-MM-DD", d),
                    ));
                }
            }
            if dates.is_empty() {
                vec![chrono::Utc::now().format("%Y-%m-%d").to_string()]
            } else {
                dates
            }
        } else {
            vec![chrono::Utc::now().format("%Y-%m-%d").to_string()]
        };

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
    let scan_dates_for_response = scan_dates.clone();

    let is_backfill = scan_dates.len() > 1;

    tokio::spawn(async move {
        // Scan all repos (pass 1 + pass 2 retry).
        // In backfill mode, use scan_single_repo_daily to get per-day breakdown.
        let scanned: Vec<(String, Option<serde_json::Value>)> = stream::iter(repos)
            .map(|slug| {
                let sem = &state_clone.index_semaphore;
                let bin = vibereport_bin.clone();
                async move {
                    let _permit = sem.acquire().await.ok()?;
                    let result = scan_single_repo_raw(&slug, &bin, 120, 60).await;
                    Some((slug, result))
                }
            })
            .buffer_unordered(3)
            .filter_map(|r| async { r })
            .collect()
            .await;

        let mut raw_results: Vec<(String, serde_json::Value)> = Vec::new();
        let mut failed_slugs: Vec<String> = Vec::new();
        for (slug, result) in scanned {
            match result {
                Some(data) => raw_results.push((slug, data)),
                None => failed_slugs.push(slug),
            }
        }

        // Pass 2: retry failed repos with doubled timeouts
        if !failed_slugs.is_empty() {
            tracing::info!(
                "Retrying {}/{} failed repos with extended timeouts",
                failed_slugs.len(),
                repo_count
            );

            let retry_results: Vec<(String, serde_json::Value)> = stream::iter(failed_slugs)
                .map(|slug| {
                    let sem = &state_clone.index_semaphore;
                    let bin = vibereport_bin.clone();
                    async move {
                        let _permit = sem.acquire().await.ok()?;
                        let data = scan_single_repo_raw(&slug, &bin, 240, 120).await?;
                        Some((slug, data))
                    }
                })
                .buffer_unordered(3)
                .filter_map(|r| async { r })
                .collect()
                .await;

            tracing::info!("Retry recovered {} repos", retry_results.len());
            raw_results.extend(retry_results);
        }

        tracing::info!(
            "Index scan complete: {}/{} repos scanned, posting results for {} date(s)",
            raw_results.len(),
            repo_count,
            scan_dates.len()
        );

        let client = reqwest::Client::new();

        if is_backfill {
            // Backfill mode: use daily_commits to compute cumulative per-date results.
            // For each scan_date, each repo's result = sum of daily_commits entries <= that date.
            let mut repo_dailies: Vec<RepoDailyBreakdown> = Vec::new();
            for (slug, data) in &raw_results {
                let days: Vec<DayEntry> = data["daily_commits"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| serde_json::from_value(v.clone()).ok())
                            .collect()
                    })
                    .unwrap_or_default();
                repo_dailies.push(RepoDailyBreakdown {
                    repo_slug: slug.clone(),
                    days,
                });
            }

            for scan_date in &scan_dates {
                // For each repo, sum daily entries with date <= scan_date
                let results: Vec<RepoScanResult> = repo_dailies
                    .iter()
                    .map(|rd| {
                        let (total, ai) = rd.days.iter().fold((0u64, 0u64), |(t, a), day| {
                            if day.date.as_str() <= scan_date.as_str() {
                                (t + day.total, a + day.ai)
                            } else {
                                (t, a)
                            }
                        });
                        RepoScanResult {
                            repo_slug: rd.repo_slug.clone(),
                            total_commits: total,
                            ai_commits: ai,
                        }
                    })
                    // Skip repos with 0 commits for this date
                    .filter(|r| r.total_commits > 0)
                    .collect();

                post_results(&client, &api_url, &auth_token, scan_date, &results).await;
            }
        } else {
            // Normal mode: single date, use totals directly
            let results: Vec<RepoScanResult> = raw_results
                .iter()
                .map(|(slug, data)| RepoScanResult {
                    repo_slug: slug.clone(),
                    total_commits: data["total_commits"].as_u64().unwrap_or(0),
                    ai_commits: data["ai_commits"].as_u64().unwrap_or(0),
                })
                .collect();

            let scan_date = &scan_dates[0];
            post_results(&client, &api_url, &auth_token, scan_date, &results).await;
        }
    });

    Ok(Json(serde_json::json!({
        "status": "started",
        "repos": repo_count,
        "quarter": quarter,
        "scan_dates": scan_dates_for_response,
    })))
}

// ── Single repo scanner for index (returns raw JSON from vibereport) ──

async fn scan_single_repo_raw(
    slug: &str,
    vibereport_bin: &str,
    clone_timeout_secs: u64,
    analyze_timeout_secs: u64,
) -> Option<serde_json::Value> {
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
    serde_json::from_str(&stdout).ok()
}

// ── Post results helper ──

async fn post_results(
    client: &reqwest::Client,
    api_url: &str,
    auth_token: &str,
    scan_date: &str,
    results: &[RepoScanResult],
) {
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
            tracing::info!(
                "Index results posted for {}: status={}, body={}",
                scan_date,
                status,
                body
            );
        }
        Err(e) => {
            tracing::error!("Failed to post index results for {}: {}", scan_date, e);
        }
    }
}

// ── Date range helper ──

fn generate_date_range(from: &str, to: &str) -> Option<Vec<String>> {
    let start = chrono::NaiveDate::parse_from_str(from, "%Y-%m-%d").ok()?;
    let end = chrono::NaiveDate::parse_from_str(to, "%Y-%m-%d").ok()?;
    if start > end {
        return None;
    }
    let mut dates = Vec::new();
    let mut current = start;
    while current <= end {
        dates.push(current.format("%Y-%m-%d").to_string());
        current += chrono::Duration::days(1);
    }
    Some(dates)
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
