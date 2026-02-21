CREATE TABLE IF NOT EXISTS reports (
  id TEXT PRIMARY KEY,
  repo_fingerprint TEXT UNIQUE,
  github_username TEXT,
  repo_name TEXT,
  ai_ratio REAL NOT NULL,
  ai_tool TEXT,
  score_points INTEGER NOT NULL,
  score_grade TEXT NOT NULL,
  roast TEXT NOT NULL,
  deps_count INTEGER DEFAULT 0,
  has_tests INTEGER DEFAULT 0,
  total_commits INTEGER DEFAULT 0,
  ai_commits INTEGER DEFAULT 0,
  total_lines INTEGER DEFAULT 0,
  languages TEXT DEFAULT '{}',
  vibe_score INTEGER DEFAULT 0,
  chaos_badges TEXT DEFAULT '[]',
  scan_source TEXT DEFAULT 'cli',
  period_start TEXT,
  period_end TEXT,
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS scan_history (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  repo_fingerprint TEXT,
  repo_name TEXT,
  ai_ratio REAL NOT NULL,
  score_points INTEGER NOT NULL,
  scanned_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_scan_history_date ON scan_history(scanned_at);

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
