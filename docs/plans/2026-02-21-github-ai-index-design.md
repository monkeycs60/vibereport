# GitHub AI Index — Design Document

**Goal:** Add a daily-scanned GitHub AI Index (1000 top repos, fixed per quarter) alongside the existing community data, displayed via a toggle on the homepage tug of war.

## Two Data Pools

| | GitHub AI Index | Community |
|---|---|---|
| **Source** | 1000 repos pre-selected per quarter | Submitted via CLI or web scan |
| **Scan** | VPS cron, daily at 3h UTC | On demand |
| **Storage** | `index_panel`, `index_scans`, `index_snapshots` | `reports` table (existing) |
| **Tug of war** | AI% on 1000 fixed repos | Total AI vs Human commits (live snapshot) |
| **Trend chart** | Yes — same panel, comparable over time | No — just live counter |
| **Time window** | Commits since 2026-01-01 | Full repo history |

Pools are completely independent. User-submitted repos do NOT feed the index.

## Panel Selection (1x per quarter)

- Fetch 600 repos sorted by stars via GitHub Search API
- Fetch 600 repos sorted by recent activity (pushed > 2026-01-01)
- Merge, deduplicate, cut to exactly 1000
- Store in `index_panel` with `quarter` tag ("2026-Q1")
- ~12 GitHub API requests total, well within rate limits

## Cron Flow

```
CF Cron Trigger (daily 3h UTC)
  → POST /index-scan to VPS (just a trigger, auth Bearer)

VPS receives trigger:
  → GET /api/index-panel?quarter=2026-Q1 (fetch repo list)
  → git clone --bare --shallow-since=2026-01-01 each repo
  → vibereport --json analysis
  → Semaphore: 3 concurrent (separate from 2 user slots)
  → ~30 minutes for 1000 repos
  → POST /api/index-results with batch results (auth Bearer)

CF Worker receives results:
  → Write individual results to index_scans
  → Aggregate and write to index_snapshots
```

## New D1 Tables

```sql
CREATE TABLE index_panel (
  repo_slug TEXT NOT NULL,
  quarter TEXT NOT NULL,
  panel_source TEXT,
  stars INTEGER,
  PRIMARY KEY (repo_slug, quarter)
);

CREATE TABLE index_scans (
  repo_slug TEXT NOT NULL,
  scan_date TEXT NOT NULL,
  total_commits INTEGER,
  ai_commits INTEGER,
  PRIMARY KEY (repo_slug, scan_date)
);

CREATE TABLE index_snapshots (
  snapshot_date TEXT PRIMARY KEY,
  quarter TEXT,
  total_repos INTEGER,
  total_commits INTEGER,
  total_ai_commits INTEGER,
  ai_percent REAL
);
```

## New API Endpoints (CF Worker)

- `GET /api/index-panel?quarter=` — repo list for the quarter
- `POST /api/index-results` — VPS pushes scan results (auth Bearer)
- `GET /api/index-latest` — latest snapshot for frontend
- `GET /api/index-trend` — snapshots over time for trend chart

## New VPS Endpoint

- `POST /index-scan` — trigger full panel scan (auth Bearer, semaphore 3)

## Frontend

- BattleChart toggle: [GitHub Index] / [Community]
- GitHub Index: data from `/api/index-latest`, legend "Based on 1,000 top GitHub repos — updated daily"
- Community: data from `/api/stats`, legend "Across N repos scanned by the community"
- Trend chart: fed by `index_snapshots`, only shown for GitHub Index view

## VPS Concurrency

- Total 5 clone slots
- 3 reserved for index cron
- 2 reserved for user web scans
- Two separate semaphores in the VPS worker

## Benchmark Results (2026-02-21)

Most repos clone in 1-5s with --shallow-since=2026-01-01. Outliers (linux, rust) take 15-35s. 1000 repos with 3 concurrent slots ≈ 30 minutes.

## What Doesn't Change

- Leaderboard stays community-based
- CLI share unchanged
- User web scan unchanged
- `reports` table untouched
