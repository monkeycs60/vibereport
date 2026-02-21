use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::Semaphore;
use uuid::Uuid;

struct AppState {
    semaphore: Semaphore,
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

    // Acquire semaphore (max 5 concurrent clones)
    let _permit = state
        .semaphore
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

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let port = std::env::var("PORT").unwrap_or_else(|_| "3001".into());
    let auth_token = std::env::var("AUTH_TOKEN").expect("AUTH_TOKEN required");
    let vibereport_bin =
        std::env::var("VIBEREPORT_BIN").unwrap_or_else(|_| "vibereport".into());

    let state = Arc::new(AppState {
        semaphore: Semaphore::new(5),
        auth_token,
        vibereport_bin,
    });

    let app = Router::new()
        .route("/scan", post(scan_handler))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("VPS worker listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
