CREATE TABLE IF NOT EXISTS reports (
  id TEXT PRIMARY KEY,
  github_username TEXT,
  repo_name TEXT,
  ai_ratio REAL NOT NULL,
  ai_tool TEXT,
  score_points INTEGER NOT NULL,
  score_grade TEXT NOT NULL,
  roast TEXT NOT NULL,
  deps_count INTEGER DEFAULT 0,
  has_tests INTEGER DEFAULT 0,
  total_lines INTEGER DEFAULT 0,
  languages TEXT DEFAULT '{}',
  created_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_score ON reports(score_points DESC);
CREATE INDEX IF NOT EXISTS idx_ai_ratio ON reports(ai_ratio DESC);
CREATE INDEX IF NOT EXISTS idx_created ON reports(created_at DESC);
