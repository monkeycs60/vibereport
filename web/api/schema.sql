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
