# GitHub AI Index — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a daily GitHub AI Index (1000 repos, fixed per quarter) with cron scanning via VPS, plus a community/index toggle on the homepage tug of war.

**Architecture:** CF Cron Trigger fires daily → triggers VPS scan of 1000 repos → VPS posts results back to CF Worker → stored in 3 new D1 tables → frontend shows toggle between index and community data.

**Tech Stack:** Cloudflare Workers (Hono) + D1, Axum (Rust VPS), Astro SSR frontend, GitHub Search API

---

### Task 1: D1 Schema Migration — Create 3 new tables

**Files:**
- Modify: `web/api/schema.sql`

**Step 1: Add new tables to schema.sql**

Append to `web/api/schema.sql`:

```sql
-- GitHub AI Index tables
CREATE TABLE IF NOT EXISTS index_panel (
  repo_slug TEXT NOT NULL,
  quarter TEXT NOT NULL,
  panel_source TEXT,
  stars INTEGER DEFAULT 0,
  PRIMARY KEY (repo_slug, quarter)
);

CREATE TABLE IF NOT EXISTS index_scans (
  repo_slug TEXT NOT NULL,
  scan_date TEXT NOT NULL,
  total_commits INTEGER DEFAULT 0,
  ai_commits INTEGER DEFAULT 0,
  PRIMARY KEY (repo_slug, scan_date)
);

CREATE TABLE IF NOT EXISTS index_snapshots (
  snapshot_date TEXT PRIMARY KEY,
  quarter TEXT,
  total_repos INTEGER DEFAULT 0,
  total_commits INTEGER DEFAULT 0,
  total_ai_commits INTEGER DEFAULT 0,
  ai_percent REAL DEFAULT 0
);
```

**Step 2: Apply migration to production D1**

Run:
```bash
cd web/api && npx wrangler d1 execute vibereport-db --remote --command "CREATE TABLE IF NOT EXISTS index_panel (repo_slug TEXT NOT NULL, quarter TEXT NOT NULL, panel_source TEXT, stars INTEGER DEFAULT 0, PRIMARY KEY (repo_slug, quarter));"
```
```bash
cd web/api && npx wrangler d1 execute vibereport-db --remote --command "CREATE TABLE IF NOT EXISTS index_scans (repo_slug TEXT NOT NULL, scan_date TEXT NOT NULL, total_commits INTEGER DEFAULT 0, ai_commits INTEGER DEFAULT 0, PRIMARY KEY (repo_slug, scan_date));"
```
```bash
cd web/api && npx wrangler d1 execute vibereport-db --remote --command "CREATE TABLE IF NOT EXISTS index_snapshots (snapshot_date TEXT PRIMARY KEY, quarter TEXT, total_repos INTEGER DEFAULT 0, total_commits INTEGER DEFAULT 0, total_ai_commits INTEGER DEFAULT 0, ai_percent REAL DEFAULT 0);"
```

**Step 3: Verify tables exist**

```bash
cd web/api && npx wrangler d1 execute vibereport-db --remote --command "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name;"
```

Expected: shows `index_panel`, `index_scans`, `index_snapshots` alongside existing tables.

**Step 4: Commit**

```bash
git add web/api/schema.sql && git commit -m "feat: add D1 tables for GitHub AI Index"
```

---

### Task 2: CF Worker — Index API endpoints

**Files:**
- Modify: `web/api/src/index.ts`

Add 4 new endpoints. The existing `export default app` must change to the `{ fetch, scheduled }` format (needed for Task 3), so do that here too.

**Step 1: Add `GET /api/index-panel` endpoint**

After the `/api/health` route, add:

```typescript
// ── GET /api/index-panel — Repo list for current quarter ──
app.get('/api/index-panel', async (c) => {
  const quarter = c.req.query('quarter') || getCurrentQuarter()
  const result = await c.env.DB.prepare(
    `SELECT repo_slug, panel_source, stars FROM index_panel WHERE quarter = ? ORDER BY stars DESC`
  ).bind(quarter).all()
  return c.json({ quarter, repos: result.results })
})
```

Add this helper function near the top (after `generateId`):

```typescript
function getCurrentQuarter(): string {
  const now = new Date()
  const q = Math.ceil((now.getMonth() + 1) / 3)
  return `${now.getFullYear()}-Q${q}`
}
```

**Step 2: Add `POST /api/index-results` endpoint**

```typescript
// ── POST /api/index-results — VPS pushes scan results (auth required) ──
app.post('/api/index-results', async (c) => {
  // Auth check
  const auth = c.req.header('authorization') || ''
  if (auth !== `Bearer ${c.env.VPS_AUTH_TOKEN}`) {
    return c.json({ error: 'Unauthorized' }, 401)
  }

  let body: { scan_date: string; results: Array<{ repo_slug: string; total_commits: number; ai_commits: number }> }
  try {
    body = await c.req.json()
  } catch {
    return c.json({ error: 'Invalid JSON' }, 400)
  }

  const db = c.env.DB
  const scanDate = body.scan_date

  // Insert individual scan results
  let totalRepos = 0
  let totalCommits = 0
  let totalAiCommits = 0

  for (const r of body.results) {
    await db.prepare(
      `INSERT INTO index_scans (repo_slug, scan_date, total_commits, ai_commits)
       VALUES (?, ?, ?, ?)
       ON CONFLICT(repo_slug, scan_date) DO UPDATE SET
         total_commits = excluded.total_commits,
         ai_commits = excluded.ai_commits`
    ).bind(r.repo_slug, scanDate, r.total_commits, r.ai_commits).run()
    totalRepos++
    totalCommits += r.total_commits
    totalAiCommits += r.ai_commits
  }

  // Compute and store snapshot
  const aiPercent = totalCommits > 0 ? (totalAiCommits / totalCommits) * 100 : 0
  const quarter = getCurrentQuarter()

  await db.prepare(
    `INSERT INTO index_snapshots (snapshot_date, quarter, total_repos, total_commits, total_ai_commits, ai_percent)
     VALUES (?, ?, ?, ?, ?, ?)
     ON CONFLICT(snapshot_date) DO UPDATE SET
       total_repos = excluded.total_repos,
       total_commits = excluded.total_commits,
       total_ai_commits = excluded.total_ai_commits,
       ai_percent = excluded.ai_percent`
  ).bind(scanDate, quarter, totalRepos, totalCommits, totalAiCommits, Math.round(aiPercent * 100) / 100).run()

  return c.json({ ok: true, snapshot: { scanDate, totalRepos, totalCommits, totalAiCommits, aiPercent: Math.round(aiPercent * 100) / 100 } })
})
```

**Step 3: Add `GET /api/index-latest` endpoint**

```typescript
// ── GET /api/index-latest — Latest index snapshot for frontend ──
app.get('/api/index-latest', async (c) => {
  const row = await c.env.DB.prepare(
    `SELECT * FROM index_snapshots ORDER BY snapshot_date DESC LIMIT 1`
  ).first()
  if (!row) {
    return c.json({ snapshot_date: null, total_repos: 0, total_commits: 0, total_ai_commits: 0, ai_percent: 0 })
  }
  return c.json(row)
})
```

**Step 4: Add `GET /api/index-trend` endpoint**

```typescript
// ── GET /api/index-trend — Index snapshots over time ──
app.get('/api/index-trend', async (c) => {
  const result = await c.env.DB.prepare(
    `SELECT snapshot_date, total_repos, total_commits, total_ai_commits, ai_percent
     FROM index_snapshots
     ORDER BY snapshot_date ASC`
  ).all()
  return c.json({ snapshots: result.results })
})
```

**Step 5: Change export format for cron support**

Replace the last line:
```typescript
// OLD:
export default app
```
With:
```typescript
// NEW: Export with scheduled handler for cron trigger
export default {
  fetch: app.fetch,
  async scheduled(event: ScheduledEvent, env: Bindings, ctx: ExecutionContext) {
    // Trigger VPS index scan
    if (!env.VPS_SCAN_URL || !env.VPS_AUTH_TOKEN) return
    try {
      await fetch(`${env.VPS_SCAN_URL}/index-scan`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${env.VPS_AUTH_TOKEN}`,
        },
        body: JSON.stringify({ api_url: 'https://vibereport-api.clement-serizay.workers.dev' }),
      })
    } catch (err: any) {
      console.error('Cron trigger failed:', err.message)
    }
  },
}
```

**Step 6: Add `Authorization` to CORS allowHeaders**

In the CORS config, update:
```typescript
  allowHeaders: ['Content-Type', 'Authorization'],
```

**Step 7: Verify build**

```bash
cd web/api && npx wrangler deploy --dry-run
```

Expected: no errors.

**Step 8: Commit**

```bash
git add web/api/src/index.ts && git commit -m "feat: add index API endpoints + cron scheduled handler"
```

---

### Task 3: CF Worker — Cron trigger in wrangler.toml

**Files:**
- Modify: `web/api/wrangler.toml`

**Step 1: Add cron trigger**

Add to `web/api/wrangler.toml`:

```toml
[triggers]
crons = ["0 3 * * *"]
```

This fires daily at 3:00 UTC.

**Step 2: Deploy**

```bash
cd web/api && npx wrangler deploy
```

Expected: deploy succeeds, output shows the cron trigger registered.

**Step 3: Commit**

```bash
git add web/api/wrangler.toml && git commit -m "feat: add daily cron trigger for index scan (3h UTC)"
```

---

### Task 4: VPS Worker — `POST /index-scan` endpoint with split semaphore

**Files:**
- Modify: `vps-worker/src/main.rs`

**Step 1: Add second semaphore to AppState**

Replace the `AppState` struct:

```rust
struct AppState {
    user_semaphore: Semaphore,   // 2 slots for user web scans
    index_semaphore: Semaphore,  // 3 slots for index cron
    auth_token: String,
    vibereport_bin: String,
}
```

Update `main()`:
```rust
let state = Arc::new(AppState {
    user_semaphore: Semaphore::new(2),
    index_semaphore: Semaphore::new(3),
    auth_token,
    vibereport_bin,
});
```

Update `scan_handler` to use `state.user_semaphore` instead of `state.semaphore`.

**Step 2: Add request/response types for index scan**

```rust
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
```

**Step 3: Add the index scan handler**

```rust
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
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to fetch panel: {}", e)))?;

    let panel: serde_json::Value = panel_res.json().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Panel parse error: {}", e)))?;

    let repos: Vec<String> = panel["repos"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|r| r["repo_slug"].as_str().map(String::from))
        .collect();

    if repos.is_empty() {
        return Ok(Json(serde_json::json!({ "error": "No repos in panel", "quarter": quarter })));
    }

    tracing::info!("Index scan starting: {} repos for {}", repos.len(), quarter);

    // 2. Scan repos with index semaphore (3 concurrent)
    let results: Vec<RepoScanResult> = futures::stream::iter(repos)
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
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to post results: {}", e)))?;

    let response: serde_json::Value = post_res.json().await.unwrap_or_default();

    tracing::info!("Index scan complete: {} repos scanned", results.len());

    Ok(Json(serde_json::json!({
        "scanned": results.len(),
        "scan_date": scan_date,
        "api_response": response,
    })))
}

async fn scan_single_repo(slug: &str, vibereport_bin: &str) -> Option<RepoScanResult> {
    let uuid = Uuid::new_v4().to_string();
    let tmp_dir = format!("/tmp/vibereport-idx-{}", uuid);
    let repo_url = format!("https://github.com/{}.git", slug);

    // Clone
    let clone = tokio::process::Command::new("git")
        .args(["clone", "--bare", "--shallow-since=2026-01-01", &repo_url, &tmp_dir])
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

fn get_current_quarter() -> String {
    let now = chrono::Utc::now();
    let q = (now.month() - 1) / 3 + 1;
    format!("{}-Q{}", now.year(), q)
}
```

**Step 4: Add route and dependencies**

In `main()`, add the route:
```rust
let app = Router::new()
    .route("/scan", post(scan_handler))
    .route("/index-scan", post(index_scan_handler))
    .with_state(state);
```

In `vps-worker/Cargo.toml`, add:
```toml
futures = "0.3"
chrono = "0.4"
```

**Step 5: Verify build**

```bash
cd /home/clement/Desktop/vibereport && cargo build -p vps-worker
```

Expected: compiles with no errors.

**Step 6: Commit**

```bash
git add vps-worker/ && git commit -m "feat: add /index-scan endpoint with split semaphore (3 index + 2 user)"
```

---

### Task 5: Panel Generation Script

**Files:**
- Create: `scripts/generate-panel.sh`

This script fetches top repos from GitHub Search API and inserts them into D1 via wrangler.

**Step 1: Create the script**

```bash
#!/bin/bash
# Generate the quarterly index panel (1000 repos)
# Usage: ./scripts/generate-panel.sh [QUARTER]
# Requires: GITHUB_TOKEN env var, wrangler CLI

set -euo pipefail

QUARTER="${1:-$(date +%Y)-Q$(( ($(date +%-m) - 1) / 3 + 1 ))}"
DB_NAME="vibereport-db"
TOKEN="${GITHUB_TOKEN:?Set GITHUB_TOKEN env var}"

echo "Generating panel for $QUARTER"

declare -A SEEN
REPOS=()

# Fetch top 600 by stars
echo "Fetching top repos by stars..."
for page in $(seq 1 6); do
  while IFS= read -r line; do
    slug=$(echo "$line" | jq -r '.full_name')
    stars=$(echo "$line" | jq -r '.stargazers_count')
    if [[ -z "${SEEN[$slug]:-}" ]]; then
      SEEN[$slug]=1
      REPOS+=("$slug|stars|$stars")
    fi
  done < <(curl -s -H "Authorization: Bearer $TOKEN" \
    -H "Accept: application/vnd.github.v3+json" \
    "https://api.github.com/search/repositories?q=stars:>1000&sort=stars&order=desc&per_page=100&page=$page" \
    | jq -c '.items[]')
  sleep 2  # Rate limit: 30 req/min for search
done

# Fetch top 600 by recent activity
echo "Fetching top repos by activity..."
for page in $(seq 1 6); do
  while IFS= read -r line; do
    slug=$(echo "$line" | jq -r '.full_name')
    stars=$(echo "$line" | jq -r '.stargazers_count')
    if [[ -z "${SEEN[$slug]:-}" ]]; then
      SEEN[$slug]=1
      REPOS+=("$slug|activity|$stars")
    fi
  done < <(curl -s -H "Authorization: Bearer $TOKEN" \
    -H "Accept: application/vnd.github.v3+json" \
    "https://api.github.com/search/repositories?q=pushed:>2026-01-01+stars:>100&sort=updated&order=desc&per_page=100&page=$page" \
    | jq -c '.items[]')
  sleep 2
done

echo "Found ${#REPOS[@]} unique repos, cutting to 1000..."

# Take first 1000
COUNT=0
for entry in "${REPOS[@]}"; do
  if [[ $COUNT -ge 1000 ]]; then break; fi
  IFS='|' read -r slug source stars <<< "$entry"
  # Escape single quotes in slug
  safe_slug=$(echo "$slug" | sed "s/'/''/g")
  cd /home/clement/Desktop/vibereport/web/api && npx wrangler d1 execute "$DB_NAME" --remote \
    --command "INSERT INTO index_panel (repo_slug, quarter, panel_source, stars) VALUES ('$safe_slug', '$QUARTER', '$source', $stars) ON CONFLICT(repo_slug, quarter) DO UPDATE SET panel_source = excluded.panel_source, stars = excluded.stars;"
  COUNT=$((COUNT + 1))
  if [[ $((COUNT % 100)) -eq 0 ]]; then echo "  Inserted $COUNT repos..."; fi
done

echo "Done! Inserted $COUNT repos for $QUARTER"

# Verify
cd /home/clement/Desktop/vibereport/web/api && npx wrangler d1 execute "$DB_NAME" --remote \
  --command "SELECT quarter, panel_source, COUNT(*) as count FROM index_panel WHERE quarter = '$QUARTER' GROUP BY panel_source;"
```

**Step 2: Make executable and test**

```bash
chmod +x scripts/generate-panel.sh
```

Don't run it yet — wait until all endpoints are deployed. This is a manual quarterly operation.

**Step 3: Commit**

```bash
git add scripts/generate-panel.sh && git commit -m "feat: add panel generation script for quarterly index"
```

---

### Task 6: Frontend — BattleChart toggle

**Files:**
- Modify: `web/frontend/src/components/BattleChart.astro`

**Step 1: Update Props to accept both datasets**

Replace the Props interface and frontmatter:

```typescript
---
interface Props {
  indexAiCommits: number;
  indexHumanCommits: number;
  indexTotalRepos: number;
  indexSnapshotDate: string | null;
  communityAiCommits: number;
  communityHumanCommits: number;
  communityTotalRepos: number;
}

const {
  indexAiCommits, indexHumanCommits, indexTotalRepos, indexSnapshotDate,
  communityAiCommits, communityHumanCommits, communityTotalRepos,
} = Astro.props;

// Pre-compute both datasets as JSON for client-side toggle
const datasets = {
  index: {
    ai: indexAiCommits,
    human: indexHumanCommits,
    total: indexAiCommits + indexHumanCommits,
    repos: indexTotalRepos,
    label: indexSnapshotDate
      ? `Based on ${indexTotalRepos.toLocaleString()} top GitHub repos — updated ${indexSnapshotDate}`
      : `Based on ${indexTotalRepos.toLocaleString()} top GitHub repos`,
  },
  community: {
    ai: communityAiCommits,
    human: communityHumanCommits,
    total: communityAiCommits + communityHumanCommits,
    repos: communityTotalRepos,
    label: `Across ${communityTotalRepos.toLocaleString()} repos scanned by the community`,
  },
};

// Default to index if data available, else community
const defaultMode = indexAiCommits + indexHumanCommits > 0 ? 'index' : 'community';
const defaultData = datasets[defaultMode];
const aiPercent = defaultData.total > 0 ? (defaultData.ai / defaultData.total) * 100 : 0;
const humanPercent = defaultData.total > 0 ? 100 - aiPercent : 100;

function formatNum(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function getPhrase(pct: number): string {
  if (pct < 5) return 'Humans are winning... for now.';
  if (pct < 20) return 'The machines are gaining ground.';
  if (pct < 50) return "It's anyone's game.";
  return 'AI has taken over.';
}
---
```

**Step 2: Add toggle buttons to the HTML**

After the opening `<div class="tug-of-war w-full" id="tug-of-war">`, add:

```html
  <!-- Toggle -->
  <div class="flex items-center justify-center gap-1 mb-6">
    <button
      class="toggle-btn px-3 py-1.5 text-xs font-mono rounded-l-md border border-tokyo-border/40 transition-all"
      data-mode="index"
      class:list={[defaultMode === 'index' ? 'bg-tokyo-cyan/20 text-tokyo-cyan border-tokyo-cyan/40' : 'bg-tokyo-surface/40 text-tokyo-dimmed hover:text-tokyo-text']}
    >
      GitHub Index
    </button>
    <button
      class="toggle-btn px-3 py-1.5 text-xs font-mono rounded-r-md border border-tokyo-border/40 transition-all"
      data-mode="community"
      class:list={[defaultMode === 'community' ? 'bg-tokyo-cyan/20 text-tokyo-cyan border-tokyo-cyan/40' : 'bg-tokyo-surface/40 text-tokyo-dimmed hover:text-tokyo-text']}
    >
      Community
    </button>
  </div>
```

**Step 3: Add legend below the phrase**

After the battle-phrase paragraph, add:

```html
  <!-- Legend -->
  <p class="text-center mt-2 text-xs text-tokyo-dimmed/60 font-mono" id="battle-legend">
    {defaultData.label}
  </p>
```

**Step 4: Embed datasets as data attribute**

On the root div, add a data attribute:
```html
<div class="tug-of-war w-full" id="tug-of-war" data-datasets={JSON.stringify(datasets)} data-default-mode={defaultMode}>
```

**Step 5: Update the client-side script**

Replace the `<script>` block. The toggle buttons switch datasets client-side by updating the bar widths, numbers, phrase, and legend:

```html
<script>
  function initTugOfWar() {
    const container = document.getElementById('tug-of-war');
    if (!container) return;

    const datasets = JSON.parse(container.dataset.datasets || '{}');
    let currentMode = container.dataset.defaultMode || 'index';

    const aiBar = container.querySelector('.ai-bar') as HTMLElement;
    const humanBar = container.querySelector('.human-bar') as HTMLElement;
    const phrase = document.getElementById('battle-phrase');
    const legend = document.getElementById('battle-legend');
    const aiLabel = container.querySelector('[data-ai-count]') as HTMLElement;
    const humanLabel = container.querySelector('[data-human-count]') as HTMLElement;

    function formatNum(n: number): string {
      if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
      if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
      return String(n);
    }

    function getPhrase(pct: number): string {
      if (pct < 5) return 'Humans are winning... for now.';
      if (pct < 20) return 'The machines are gaining ground.';
      if (pct < 50) return "It's anyone's game.";
      return 'AI has taken over.';
    }

    function updateBars(mode: string) {
      const d = datasets[mode];
      if (!d || !aiBar || !humanBar) return;

      const aiPct = d.total > 0 ? (d.ai / d.total) * 100 : 0;
      const humanPct = d.total > 0 ? 100 - aiPct : 100;

      aiBar.style.width = `${aiPct}%`;
      humanBar.style.width = `${humanPct}%`;

      // Update percentage labels inside bars
      const aiPctLabel = aiBar.querySelector('span');
      const humanPctLabel = humanBar.querySelector('span');
      if (aiPctLabel) aiPctLabel.textContent = `${Math.round(aiPct)}%`;
      if (humanPctLabel) humanPctLabel.textContent = `${Math.round(humanPct)}%`;

      // Update commit counts
      if (aiLabel) aiLabel.textContent = `${formatNum(d.ai)} commits`;
      if (humanLabel) humanLabel.textContent = `${formatNum(d.human)} commits`;

      // Update phrase and legend
      if (phrase) phrase.textContent = getPhrase(aiPct);
      if (legend) legend.textContent = d.label;

      // Update toggle button styles
      container.querySelectorAll('.toggle-btn').forEach((btn) => {
        const el = btn as HTMLElement;
        if (el.dataset.mode === mode) {
          el.className = 'toggle-btn px-3 py-1.5 text-xs font-mono rounded-' + (mode === 'index' ? 'l' : 'r') + '-md border border-tokyo-cyan/40 bg-tokyo-cyan/20 text-tokyo-cyan transition-all';
        } else {
          el.className = 'toggle-btn px-3 py-1.5 text-xs font-mono rounded-' + (el.dataset.mode === 'index' ? 'l' : 'r') + '-md border border-tokyo-border/40 bg-tokyo-surface/40 text-tokyo-dimmed hover:text-tokyo-text transition-all';
        }
      });
    }

    // Toggle click handlers
    container.querySelectorAll('.toggle-btn').forEach((btn) => {
      btn.addEventListener('click', () => {
        const mode = (btn as HTMLElement).dataset.mode || 'index';
        if (mode !== currentMode) {
          currentMode = mode;
          updateBars(mode);
        }
      });
    });

    // Initial animation on scroll
    const observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting) {
            requestAnimationFrame(() => updateBars(currentMode));
            if (phrase) {
              setTimeout(() => {
                phrase.classList.remove('opacity-0');
                phrase.classList.add('opacity-100');
              }, 1200);
            }
            observer.unobserve(entry.target);
          }
        });
      },
      { threshold: 0.2 }
    );

    observer.observe(container);
  }

  document.addEventListener('astro:page-load', initTugOfWar);
  initTugOfWar();
</script>
```

**Step 6: Add data attributes to commit count spans**

Update the AI and Human labels to have data attributes for JS targeting:
```html
<span class="text-xs text-tokyo-dimmed font-mono" data-ai-count>{formatNum(defaultData.ai)} commits</span>
```
```html
<span class="text-xs text-tokyo-dimmed font-mono" data-human-count>{formatNum(defaultData.human)} commits</span>
```

**Step 7: Commit**

```bash
git add web/frontend/src/components/BattleChart.astro && git commit -m "feat: add GitHub Index / Community toggle to BattleChart"
```

---

### Task 7: Frontend — Data loading + trend chart integration

**Files:**
- Modify: `web/frontend/src/lib/api.ts`
- Modify: `web/frontend/src/pages/index.astro`

**Step 1: Add `fetchIndexLatest` and `fetchIndexTrend` to api.ts**

Add to `web/frontend/src/lib/api.ts`:

```typescript
export interface IndexSnapshot {
  snapshot_date: string | null;
  quarter: string;
  total_repos: number;
  total_commits: number;
  total_ai_commits: number;
  ai_percent: number;
}

export interface IndexTrendResponse {
  snapshots: IndexSnapshot[];
}

export async function fetchIndexLatest(): Promise<IndexSnapshot> {
  try {
    const res = await fetch(`${API_URL}/api/index-latest`);
    if (!res.ok) throw new Error('Failed to fetch index');
    return res.json();
  } catch {
    return { snapshot_date: null, quarter: '', total_repos: 0, total_commits: 0, total_ai_commits: 0, ai_percent: 0 };
  }
}

export async function fetchIndexTrend(): Promise<IndexTrendResponse> {
  try {
    const res = await fetch(`${API_URL}/api/index-trend`);
    if (!res.ok) throw new Error('Failed to fetch index trend');
    return res.json();
  } catch {
    return { snapshots: [] };
  }
}
```

**Step 2: Update index.astro to fetch both datasets**

Replace the frontmatter in `web/frontend/src/pages/index.astro`:

```typescript
---
import Base from '../layouts/Base.astro';
import StatsCounter from '../components/StatsCounter.astro';
import LeaderboardTable from '../components/LeaderboardTable.astro';
import TrendChart from '../components/TrendChart.astro';
import BattleChart from '../components/BattleChart.astro';
import { fetchStats, fetchLeaderboard, fetchTrends, fetchIndexLatest, fetchIndexTrend } from '../lib/api';

let stats = { total_reports: 0, total_commits: 0, total_ai_commits: 0 };
let leaderboard = { entries: [], total: 0, page: 1, limit: 5 };
let trends = { period: '1y', trends: [] as Array<{ month: string; avg_ai_ratio: number; total_scans: number; avg_score: number; total_commits: number; ai_commits: number }> };
let indexLatest = { snapshot_date: null as string | null, quarter: '', total_repos: 0, total_commits: 0, total_ai_commits: 0, ai_percent: 0 };
let indexTrend = { snapshots: [] as Array<{ snapshot_date: string; total_repos: number; total_commits: number; total_ai_commits: number; ai_percent: number }> };

try { stats = await fetchStats(); } catch {}
try { leaderboard = await fetchLeaderboard(1, 5); } catch {}
try { trends = await fetchTrends(); } catch {}
try { indexLatest = await fetchIndexLatest(); } catch {}
try { indexTrend = await fetchIndexTrend(); } catch {}

// Community data (from trends)
const communityAiCommits = trends.trends.reduce((s, t) => s + (t.ai_commits || 0), 0);
const communityHumanCommits = trends.trends.reduce((s, t) => s + ((t.total_commits || 0) - (t.ai_commits || 0)), 0);

// Index data
const indexAiCommits = indexLatest.total_ai_commits || 0;
const indexHumanCommits = (indexLatest.total_commits || 0) - indexAiCommits;
---
```

**Step 3: Update BattleChart invocation**

Replace the BattleChart component call:

```html
<BattleChart
  indexAiCommits={indexAiCommits}
  indexHumanCommits={indexHumanCommits}
  indexTotalRepos={indexLatest.total_repos}
  indexSnapshotDate={indexLatest.snapshot_date}
  communityAiCommits={communityAiCommits}
  communityHumanCommits={communityHumanCommits}
  communityTotalRepos={stats.total_reports}
/>
```

**Step 4: Update trend chart section — show index trend when available**

Replace the trend section with:

```html
  <!-- Trend -->
  <section class="py-16">
    <div class="max-w-4xl mx-auto px-4 sm:px-6 lg:px-8">
      <h2 class="text-2xl font-bold text-center text-tokyo-text mb-8">
        The rise of vibe coding
      </h2>
      {indexTrend.snapshots.length >= 2 ? (
        <TrendChart trends={indexTrend.snapshots.map(s => ({
          month: s.snapshot_date,
          avg_ai_ratio: s.ai_percent / 100,
          total_scans: s.total_repos,
          total_commits: s.total_commits,
          ai_commits: s.total_ai_commits,
        }))} />
      ) : (
        <TrendChart trends={trends.trends} />
      )}
      <div class="text-center mt-4">
        <a
          href="/trends"
          class="text-sm text-tokyo-cyan hover:text-tokyo-cyan/80 transition-colors"
        >
          View full trends &rarr;
        </a>
      </div>
    </div>
  </section>
```

**Step 5: Verify frontend builds**

```bash
cd /home/clement/Desktop/vibereport/web/frontend && npm run build
```

Expected: build succeeds.

**Step 6: Commit**

```bash
git add web/frontend/src/lib/api.ts web/frontend/src/pages/index.astro && git commit -m "feat: load index data on homepage, toggle between index and community"
```

---

### Task 8: Deploy + seed panel + end-to-end test

**Step 1: Deploy CF Worker**

```bash
cd /home/clement/Desktop/vibereport/web/api && npx wrangler deploy
```

**Step 2: Build and deploy VPS worker**

```bash
ssh ubuntu@vps-139a77b3.vps.ovh.net 'cd ~/vibereport && git pull && cargo build --release -p vps-worker && sudo systemctl restart vibereport-worker'
```

**Step 3: Generate Q1 2026 panel**

```bash
cd /home/clement/Desktop/vibereport && GITHUB_TOKEN=<token> ./scripts/generate-panel.sh 2026-Q1
```

**Step 4: Trigger a manual index scan to test**

```bash
curl -X POST https://scan.vibereport.dev/index-scan \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $VPS_AUTH_TOKEN" \
  -d '{"api_url": "https://vibereport-api.clement-serizay.workers.dev"}'
```

**Step 5: Verify results**

```bash
cd /home/clement/Desktop/vibereport/web/api && npx wrangler d1 execute vibereport-db --remote --command "SELECT * FROM index_snapshots ORDER BY snapshot_date DESC LIMIT 1;"
```

Expected: shows a snapshot with total_repos, total_commits, total_ai_commits, ai_percent.

**Step 6: Verify frontend**

Open `http://localhost:4321/` — the tug of war should now show the toggle with "GitHub Index" selected and real data.

**Step 7: Push to GitHub to trigger Vercel deploy**

```bash
git push
```

**Step 8: Commit any final fixes**

```bash
git add -A && git commit -m "chore: deploy index feature end-to-end"
```

---

## Task Dependencies

```
Task 1 (D1 schema) → Task 2 (CF endpoints) → Task 3 (cron trigger)
                   → Task 5 (panel script)
Task 4 (VPS endpoint) — independent, can parallel with Task 2
Task 6 (BattleChart toggle) — independent, can parallel with Tasks 2-4
Task 7 (frontend data loading) — depends on Task 2 (needs api.ts types) + Task 6 (needs new BattleChart)
Task 8 (deploy + test) — depends on all above
```

**Parallel tracks:**
- Track A: Task 1 → Task 2 → Task 3
- Track B: Task 4 (VPS)
- Track C: Task 5 (panel script)
- Track D: Task 6 → Task 7
- Final: Task 8 (after all tracks complete)
