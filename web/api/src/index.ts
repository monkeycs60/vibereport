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

function getDb(env: Bindings) {
  return createClient({
    url: env.TURSO_URL,
    authToken: env.TURSO_AUTH_TOKEN,
  })
}

function generateId(): string {
  return Math.random().toString(36).substring(2, 10)
}

// ── POST /api/reports — Submit a new report ──
app.post('/api/reports', async (c) => {
  const body = await c.req.json()
  const db = getDb(c.env)
  const id = generateId()

  await db.execute({
    sql: `INSERT INTO reports (id, github_username, repo_name, ai_ratio, ai_tool, score_points, score_grade, roast, deps_count, has_tests, total_lines, languages)
          VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
    args: [
      id,
      body.github_username || null,
      body.repo_name || null,
      body.ai_ratio,
      body.ai_tool || null,
      body.score_points,
      body.score_grade,
      body.roast,
      body.deps_count || 0,
      body.has_tests ? 1 : 0,
      body.total_lines || 0,
      body.languages || '{}',
    ],
  })

  // Get rank
  const rankResult = await db.execute({
    sql: `SELECT COUNT(*) as rank FROM reports WHERE score_points > ?`,
    args: [body.score_points],
  })
  const rank = (Number(rankResult.rows[0]?.rank) || 0) + 1

  // Get total count for percentile
  const countResult = await db.execute({
    sql: `SELECT COUNT(*) as total FROM reports`,
    args: [],
  })
  const total = Number(countResult.rows[0]?.total) || 1
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

  return c.json(result.rows[0])
})

// ── GET /api/leaderboard — Top scores, paginated ──
app.get('/api/leaderboard', async (c) => {
  const db = getDb(c.env)
  const page = parseInt(c.req.query('page') || '1')
  const limit = Math.min(parseInt(c.req.query('limit') || '20'), 100)
  const offset = (page - 1) * limit

  const result = await db.execute({
    sql: `SELECT id, repo_name, github_username, ai_ratio, score_points, score_grade, roast, created_at
          FROM reports
          ORDER BY score_points DESC, created_at DESC
          LIMIT ? OFFSET ?`,
    args: [limit, offset],
  })

  const countResult = await db.execute({
    sql: `SELECT COUNT(*) as total FROM reports`,
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

export default app
