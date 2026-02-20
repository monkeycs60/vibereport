// SQL Schema (run in Turso console):
//
// CREATE TABLE IF NOT EXISTS reports (
//   id TEXT PRIMARY KEY,
//   repo_fingerprint TEXT,
//   github_username TEXT,
//   repo_name TEXT,
//   ai_ratio REAL NOT NULL,
//   ai_tool TEXT,
//   score_points INTEGER NOT NULL,
//   score_grade TEXT NOT NULL,
//   roast TEXT NOT NULL,
//   deps_count INTEGER DEFAULT 0,
//   has_tests INTEGER DEFAULT 0,
//   total_lines INTEGER DEFAULT 0,
//   languages TEXT DEFAULT '{}',
//   created_at TEXT DEFAULT (datetime('now')),
//   updated_at TEXT DEFAULT (datetime('now'))
// );
// CREATE UNIQUE INDEX IF NOT EXISTS idx_reports_fingerprint ON reports(repo_fingerprint) WHERE repo_fingerprint IS NOT NULL;
//
// CREATE TABLE IF NOT EXISTS scan_history (
//   id INTEGER PRIMARY KEY AUTOINCREMENT,
//   repo_fingerprint TEXT,
//   repo_name TEXT,
//   ai_ratio REAL NOT NULL,
//   score_points INTEGER NOT NULL,
//   scanned_at TEXT DEFAULT (datetime('now'))
// );
// CREATE INDEX IF NOT EXISTS idx_scan_history_date ON scan_history(scanned_at);

import { Hono } from 'hono'
import { cors } from 'hono/cors'
import { createClient } from '@libsql/client/web'

type Bindings = {
  TURSO_URL: string
  TURSO_AUTH_TOKEN: string
}

const app = new Hono<{ Bindings: Bindings }>()

// CORS for frontend
app.use('/*', cors({
  origin: ['https://vibereport.dev', 'http://localhost:4321'],
}))

// Global error handler
app.onError((err, c) => {
  console.error('API Error:', err.message)
  return c.json({ error: 'Internal server error' }, 500)
})

function getDb(env: Bindings) {
  return createClient({
    url: env.TURSO_URL,
    authToken: env.TURSO_AUTH_TOKEN,
  })
}

function generateId(): string {
  const bytes = new Uint8Array(12)
  crypto.getRandomValues(bytes)
  return Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('')
}

// ── POST /api/reports — Submit a new report ──
app.post('/api/reports', async (c) => {
  let body: Record<string, unknown>
  try {
    body = await c.req.json()
  } catch {
    return c.json({ error: 'Invalid JSON body' }, 400)
  }

  // Validate required fields
  if (typeof body.ai_ratio !== 'number' || body.ai_ratio < 0 || body.ai_ratio > 1) {
    return c.json({ error: 'ai_ratio must be a number between 0 and 1' }, 400)
  }
  if (typeof body.score_points !== 'number' || !Number.isInteger(body.score_points) || body.score_points < 0 || body.score_points > 200) {
    return c.json({ error: 'score_points must be an integer between 0 and 200' }, 400)
  }
  if (typeof body.score_grade !== 'string' || body.score_grade.length > 5) {
    return c.json({ error: 'Invalid score_grade' }, 400)
  }
  if (typeof body.roast !== 'string' || body.roast.length === 0 || body.roast.length > 500) {
    return c.json({ error: 'roast must be a string under 500 chars' }, 400)
  }

  const db = getDb(c.env)
  const id = generateId()
  const fingerprint = typeof body.repo_fingerprint === 'string' ? body.repo_fingerprint : null
  const githubUsername = typeof body.github_username === 'string' ? body.github_username : null
  const repoName = typeof body.repo_name === 'string' ? body.repo_name : null
  const aiTool = typeof body.ai_tool === 'string' ? body.ai_tool : null
  const depsCount = typeof body.deps_count === 'number' ? body.deps_count : 0
  const hasTests = body.has_tests ? 1 : 0
  const totalLines = typeof body.total_lines === 'number' ? body.total_lines : 0
  const languages = typeof body.languages === 'string' ? body.languages : '{}'

  if (fingerprint) {
    // Upsert: update existing report if fingerprint matches
    await db.execute({
      sql: `INSERT INTO reports (id, repo_fingerprint, github_username, repo_name, ai_ratio, ai_tool, score_points, score_grade, roast, deps_count, has_tests, total_lines, languages, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
            ON CONFLICT(repo_fingerprint) DO UPDATE SET
              ai_ratio = excluded.ai_ratio,
              ai_tool = excluded.ai_tool,
              score_points = excluded.score_points,
              score_grade = excluded.score_grade,
              roast = excluded.roast,
              deps_count = excluded.deps_count,
              has_tests = excluded.has_tests,
              total_lines = excluded.total_lines,
              languages = excluded.languages,
              updated_at = datetime('now')`,
      args: [
        id, fingerprint, githubUsername, repoName,
        body.ai_ratio, aiTool, body.score_points, body.score_grade, body.roast,
        depsCount, hasTests, totalLines, languages,
      ],
    })
  } else {
    // No fingerprint: plain insert (for backwards compatibility)
    await db.execute({
      sql: `INSERT INTO reports (id, github_username, repo_name, ai_ratio, ai_tool, score_points, score_grade, roast, deps_count, has_tests, total_lines, languages)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      args: [
        id, githubUsername, repoName,
        body.ai_ratio, aiTool, body.score_points, body.score_grade, body.roast,
        depsCount, hasTests, totalLines, languages,
      ],
    })
  }

  // Always record in scan_history for trends
  await db.execute({
    sql: `INSERT INTO scan_history (repo_fingerprint, repo_name, ai_ratio, score_points)
          VALUES (?, ?, ?, ?)`,
    args: [fingerprint, repoName, body.ai_ratio, body.score_points],
  })

  // Get rank and total in one query
  const statsResult = await db.execute({
    sql: `SELECT
            (SELECT COUNT(*) FROM reports WHERE score_points > ?) as rank,
            (SELECT COUNT(*) FROM reports) as total`,
    args: [body.score_points],
  })
  const rank = (Number(statsResult.rows[0]?.rank) || 0) + 1
  const total = Number(statsResult.rows[0]?.total) || 1
  const percentile = ((total - rank) / total) * 100

  return c.json({
    id,
    url: `https://vibereport.dev/r/${id}`,
    rank,
    percentile: Math.round(percentile * 10) / 10,
  })
})

// ── GET /api/reports/:id — Get a single report ──
app.get('/api/reports/:id', async (c) => {
  const db = getDb(c.env)
  const result = await db.execute({
    sql: `SELECT * FROM reports WHERE id = ?`,
    args: [c.req.param('id')],
  })

  if (result.rows.length === 0) {
    return c.json({ error: 'Report not found' }, 404)
  }

  const row = result.rows[0]
  return c.json({ ...row, has_tests: Boolean(row.has_tests) })
})

// ── GET /api/leaderboard — Top scores, paginated ──
app.get('/api/leaderboard', async (c) => {
  const db = getDb(c.env)
  const page = parseInt(c.req.query('page') || '1')
  const limit = Math.min(parseInt(c.req.query('limit') || '20'), 100)
  const offset = (page - 1) * limit

  // Period filter
  const period = c.req.query('period')
  let whereClause = ''
  if (period === 'week') {
    whereClause = "WHERE created_at > datetime('now', '-7 days')"
  } else if (period === 'month') {
    whereClause = "WHERE created_at > datetime('now', '-30 days')"
  }

  const result = await db.execute({
    sql: `SELECT id, repo_name, github_username, ai_ratio, score_points, score_grade, roast, created_at
          FROM reports
          ${whereClause}
          ORDER BY score_points DESC, created_at DESC
          LIMIT ? OFFSET ?`,
    args: [limit, offset],
  })

  const countResult = await db.execute({
    sql: `SELECT COUNT(*) as total FROM reports ${whereClause}`,
    args: [],
  })

  return c.json({
    reports: result.rows,
    total: countResult.rows[0]?.total,
    page,
    limit,
  })
})

// ── GET /api/stats — Aggregate stats ──
app.get('/api/stats', async (c) => {
  const db = getDb(c.env)

  const result = await db.execute({
    sql: `SELECT
            COUNT(*) as total_reports,
            AVG(ai_ratio) as avg_ai_ratio,
            AVG(score_points) as avg_score,
            MAX(score_points) as max_score,
            SUM(total_lines) as total_lines_analyzed
          FROM reports`,
    args: [],
  })

  return c.json(result.rows[0] || {})
})

// ── GET /api/trends — Monthly scan trends ──
app.get('/api/trends', async (c) => {
  const db = getDb(c.env)

  // Period: 6m, 1y, all (default: 1y)
  const period = c.req.query('period') || '1y'
  let whereClause = ''
  if (period === '6m') {
    whereClause = "WHERE scanned_at > datetime('now', '-6 months')"
  } else if (period === '1y') {
    whereClause = "WHERE scanned_at > datetime('now', '-1 year')"
  }
  // 'all' = no where clause

  const result = await db.execute({
    sql: `SELECT
            strftime('%Y-%m', scanned_at) as month,
            AVG(ai_ratio) as avg_ai_ratio,
            COUNT(*) as total_scans,
            AVG(score_points) as avg_score
          FROM scan_history
          ${whereClause}
          GROUP BY strftime('%Y-%m', scanned_at)
          ORDER BY month ASC`,
    args: [],
  })

  return c.json({
    period,
    trends: result.rows,
  })
})

// ── GET /api/badge/:id.svg — Dynamic SVG badge ──
app.get('/api/badge/:id.svg', async (c) => {
  const id = (c.req.param('id') ?? '').replace('.svg', '')
  const db = getDb(c.env)

  const result = await db.execute({
    sql: `SELECT score_grade, ai_ratio FROM reports WHERE id = ?`,
    args: [id],
  })

  if (result.rows.length === 0) {
    return c.text('Not found', 404)
  }

  const report = result.rows[0]
  const grade = String(report.score_grade)
  const aiPct = Math.round(Number(report.ai_ratio) * 100)

  // Color based on grade
  const color = grade.startsWith('S') || grade.startsWith('A') ? '#9ece6a'
    : grade.startsWith('B') ? '#e0af68'
    : grade.startsWith('C') ? '#ff9e64'
    : '#f7768e'

  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="200" height="20">
    <rect width="200" height="20" rx="3" fill="#1a1b26"/>
    <rect x="120" width="80" height="20" rx="3" fill="${color}"/>
    <text x="60" y="14" fill="#c0caf5" font-family="monospace" font-size="11" text-anchor="middle">vibe score</text>
    <text x="160" y="14" fill="#1a1b26" font-family="monospace" font-size="11" font-weight="bold" text-anchor="middle">${grade} ${aiPct}%AI</text>
  </svg>`

  return c.body(svg, {
    headers: {
      'Content-Type': 'image/svg+xml',
      'Cache-Control': 'public, max-age=3600',
    },
  })
})

// ── Health check ──
app.get('/api/health', (c) => c.json({ status: 'ok' }))

export default app
