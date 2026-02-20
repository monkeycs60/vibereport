// SQL Schema: see schema.sql (apply with `wrangler d1 execute`)

import { Hono } from 'hono'
import { cors } from 'hono/cors'

type Bindings = {
  DB: D1Database
  GITHUB_TOKEN?: string
}

const app = new Hono<{ Bindings: Bindings }>()

// CORS for frontend
app.use('/*', cors({
  origin: (origin) => {
    if (!origin) return 'https://vibereport.dev'
    if (origin.startsWith('http://localhost:')) return origin
    if (origin === 'https://vibereport.dev') return origin
    if (origin.endsWith('.vibereport.pages.dev') || origin === 'https://vibereport.pages.dev') return origin
    if (origin.endsWith('.clement-serizay.workers.dev')) return origin
    if (origin === 'https://vibereport.vercel.app' || origin.endsWith('.vercel.app')) return origin
    return 'https://vibereport.dev'
  },
  allowMethods: ['GET', 'POST', 'OPTIONS'],
  allowHeaders: ['Content-Type'],
}))

// Global error handler
app.onError((err, c) => {
  console.error('API Error:', err.message)
  return c.json({ error: 'Internal server error' }, 500)
})

function generateId(): string {
  const bytes = new Uint8Array(12)
  crypto.getRandomValues(bytes)
  return Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('')
}

// ── GitHub commit analysis with parallel fetching ──
// Processes ALL commits without holding them all in memory.
// Fetches pages in parallel batches (20 concurrent requests).
// 100k commits ≈ 1000 pages ≈ 50 batches ≈ ~25 seconds.
interface ScanResult {
  totalScanned: number
  totalInRepo: number
  aiCommits: number
  toolCounts: Record<string, number>
  oldestCommitSha: string
}

const CONCURRENCY = 20  // parallel page fetches per batch
const MAX_PAGES = 999   // ~100k commits (Workers 1000 subrequest limit minus probe)

async function analyzeGitHubRepo(owner: string, repo: string, token?: string): Promise<ScanResult> {
  const headers: Record<string, string> = {
    'Accept': 'application/vnd.github.v3+json',
    'User-Agent': 'vibereport/1.0',
  }
  if (token) headers['Authorization'] = `Bearer ${token}`

  // 1. Probe: get total number of pages from Link header
  const probeRes = await fetch(
    `https://api.github.com/repos/${owner}/${repo}/commits?per_page=100&page=1`,
    { headers }
  )
  if (!probeRes.ok) throw new Error(`GitHub API error: ${probeRes.status}`)

  const firstPage = await probeRes.json() as any[]
  if (!Array.isArray(firstPage) || firstPage.length === 0) {
    throw new Error('404')
  }

  // Parse Link header to find last page
  let lastPage = 1
  const linkHeader = probeRes.headers.get('link') || ''
  const lastMatch = linkHeader.match(/page=(\d+)>;\s*rel="last"/)
  if (lastMatch) lastPage = parseInt(lastMatch[1])

  const totalInRepo = lastPage * 100 // approximate (last page may have fewer)

  // 2. Process page 1 immediately
  let totalScanned = 0
  let aiCommits = 0
  const toolCounts: Record<string, number> = {}
  let oldestCommitSha = ''

  function processPage(commits: any[]) {
    for (const commit of commits) {
      totalScanned++
      const msg = (commit.commit?.message || '').toLowerCase()
      const tool = detectAiTool(msg)
      if (tool !== 'Human') {
        aiCommits++
        toolCounts[tool] = (toolCounts[tool] || 0) + 1
      }
      oldestCommitSha = commit.sha // last processed = oldest
    }
  }

  processPage(firstPage)

  // 3. Fetch remaining pages in parallel batches (capped at MAX_PAGES)
  const pagesToFetch = Math.min(lastPage, MAX_PAGES)
  if (pagesToFetch > 1) {
    for (let batchStart = 2; batchStart <= pagesToFetch; batchStart += CONCURRENCY) {
      const batchEnd = Math.min(batchStart + CONCURRENCY - 1, pagesToFetch)
      const pageNumbers = Array.from(
        { length: batchEnd - batchStart + 1 },
        (_, i) => batchStart + i
      )

      const results = await Promise.all(
        pageNumbers.map(async (page) => {
          try {
            const res = await fetch(
              `https://api.github.com/repos/${owner}/${repo}/commits?per_page=100&page=${page}`,
              { headers }
            )
            if (!res.ok) return []
            const data = await res.json()
            return Array.isArray(data) ? data : []
          } catch {
            return []
          }
        })
      )

      // Process each page's commits immediately, then discard raw data
      let batchEmpty = true
      for (const pageCommits of results) {
        if (pageCommits.length > 0) batchEmpty = false
        processPage(pageCommits)
      }

      // If an entire batch returned empty, we've hit the end
      if (batchEmpty) break
    }
  }

  return { totalScanned, totalInRepo, aiCommits, toolCounts, oldestCommitSha }
}

// Server-side AI tool detection (mirrors src/git/ai_detect.rs logic)
function detectAiTool(msg: string): string {
  if (msg.includes('co-authored-by: claude') || msg.includes('noreply@anthropic.com') || msg.includes('generated with claude code')) return 'Claude Code'
  if (msg.includes('co-authored-by: cursor')) return 'Cursor'
  if (msg.includes('co-authored-by: aider') || msg.includes('noreply@aider.chat') || msg.includes('aider: ')) return 'Aider'
  if (msg.includes('co-authored-by: codex') || msg.includes('generated by codex') || msg.includes('codex-cli')) return 'Codex CLI'
  if (msg.includes('co-authored-by: copilot') || msg.includes('github-copilot')) return 'GitHub Copilot'
  if (msg.includes('co-authored-by: gemini') || (msg.includes('noreply@google.com') && msg.includes('gemini'))) return 'Gemini CLI'
  return 'Human'
}

// Server-side grade calculation (mirrors src/score/calculator.rs)
function gradeFromPoints(points: number): string {
  if (points > 100) return 'S+'
  if (points >= 90) return 'S'
  if (points >= 80) return 'A+'
  if (points >= 70) return 'A'
  if (points >= 60) return 'B+'
  if (points >= 50) return 'B'
  if (points >= 40) return 'C+'
  if (points >= 30) return 'C'
  if (points >= 20) return 'D'
  return 'F'
}

// Server-side roast selection (simplified from src/score/roast.rs)
function pickRoast(points: number, aiRatio: number): string {
  if (aiRatio > 0.95) return "You're the project manager now."
  if (aiRatio === 0) return "Write code like it's 2019."
  if (points > 100) return 'Beyond vibe. You are the vibe.'
  if (points >= 90) return 'The AI is the senior dev here.'
  if (points >= 80) return 'You prompt, Claude delivers.'
  if (points >= 70) return 'More vibes than version control.'
  if (points >= 60) return 'Solid vibe-to-code ratio.'
  if (points >= 50) return 'Half human, half machine.'
  if (points >= 40) return 'Training wheels still on.'
  if (points >= 30) return 'Mostly artisanal, free-range code.'
  if (points >= 20) return 'You actually read the docs?'
  return 'Handcrafted with mass-produced tears.'
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

  const db = c.env.DB
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
    await db.prepare(
      `INSERT INTO reports (id, repo_fingerprint, github_username, repo_name, ai_ratio, ai_tool, score_points, score_grade, roast, deps_count, has_tests, total_lines, languages, updated_at)
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
         updated_at = datetime('now')`
    ).bind(
      id, fingerprint, githubUsername, repoName,
      body.ai_ratio, aiTool, body.score_points, body.score_grade, body.roast,
      depsCount, hasTests, totalLines, languages,
    ).run()
  } else {
    // No fingerprint: plain insert (for backwards compatibility)
    await db.prepare(
      `INSERT INTO reports (id, github_username, repo_name, ai_ratio, ai_tool, score_points, score_grade, roast, deps_count, has_tests, total_lines, languages)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`
    ).bind(
      id, githubUsername, repoName,
      body.ai_ratio, aiTool, body.score_points, body.score_grade, body.roast,
      depsCount, hasTests, totalLines, languages,
    ).run()
  }

  // Always record in scan_history for trends
  await db.prepare(
    `INSERT INTO scan_history (repo_fingerprint, repo_name, ai_ratio, score_points)
     VALUES (?, ?, ?, ?)`
  ).bind(fingerprint, repoName, body.ai_ratio, body.score_points).run()

  // Get rank and total in one query
  const statsResult = await db.prepare(
    `SELECT
       (SELECT COUNT(*) FROM reports WHERE score_points > ?) as rank,
       (SELECT COUNT(*) FROM reports) as total`
  ).bind(body.score_points).first()
  const rank = (Number(statsResult?.rank) || 0) + 1
  const total = Number(statsResult?.total) || 1
  const percentile = ((total - rank) / total) * 100

  return c.json({
    id,
    url: `https://vibereport.dev/r/${id}`,
    rank,
    percentile: Math.round(percentile * 10) / 10,
  })
})

// ── POST /api/scan — Scan a public GitHub repo ──
app.post('/api/scan', async (c) => {
  let body: Record<string, unknown>
  try {
    body = await c.req.json()
  } catch {
    return c.json({ error: 'Invalid JSON body' }, 400)
  }

  const repoInput = typeof body.repo === 'string' ? body.repo.trim() : ''
  if (!repoInput) {
    return c.json({ error: 'repo is required' }, 400)
  }

  // Parse repo reference: "github:user/repo", "https://github.com/user/repo", "user/repo"
  const match = repoInput
    .replace(/^github:/, '')
    .replace(/^https?:\/\/github\.com\//, '')
    .replace(/\.git$/, '')
    .replace(/\/$/, '')
  const parts = match.split('/')
  if (parts.length < 2 || !parts[0] || !parts[1]) {
    return c.json({ error: 'Invalid repo format. Use user/repo or https://github.com/user/repo' }, 400)
  }
  const owner = parts[0]
  const repo = parts[1]

  try {
    // Analyze ALL commits via parallel fetching (20 pages at a time)
    const scan = await analyzeGitHubRepo(owner, repo, c.env.GITHUB_TOKEN)

    const totalCommits = scan.totalScanned
    const aiCommits = scan.aiCommits
    const totalInRepo = scan.totalInRepo
    const toolCounts = scan.toolCounts
    const aiRatio = totalCommits > 0 ? aiCommits / totalCommits : 0
    const humanCommits = totalCommits - aiCommits

    // Determine primary AI tool
    const primaryTool = Object.entries(toolCounts)
      .sort(([, a], [, b]) => b - a)[0]?.[0] || 'Human'

    // Simple vibe score calculation (server-side simplified version)
    let points = Math.floor(aiRatio * 70)
    if (aiRatio > 0.9) points += 15
    else if (aiRatio > 0.7) points += 10
    else if (aiRatio > 0.5) points += 5

    const grade = gradeFromPoints(points)
    const roast = pickRoast(points, aiRatio)

    // Compute fingerprint from oldest commit
    const fingerprint = scan.oldestCommitSha
      ? `${scan.oldestCommitSha}:https://github.com/${owner}/${repo}`
      : null

    // Save to database (upsert if fingerprint exists)
    const db = c.env.DB
    const id = generateId()
    const repoName = repo
    const githubUsername = owner

    if (fingerprint) {
      await db.prepare(
        `INSERT INTO reports (id, repo_fingerprint, github_username, repo_name, ai_ratio, ai_tool, score_points, score_grade, roast, total_commits, ai_commits, deps_count, has_tests, total_lines, languages, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 0, 0, '{}', datetime('now'))
         ON CONFLICT(repo_fingerprint) DO UPDATE SET
           ai_ratio = excluded.ai_ratio,
           ai_tool = excluded.ai_tool,
           score_points = excluded.score_points,
           score_grade = excluded.score_grade,
           roast = excluded.roast,
           total_commits = excluded.total_commits,
           ai_commits = excluded.ai_commits,
           updated_at = datetime('now')`
      ).bind(id, fingerprint, githubUsername, repoName, aiRatio, primaryTool, points, grade, roast, totalCommits, aiCommits).run()
    } else {
      await db.prepare(
        `INSERT INTO reports (id, github_username, repo_name, ai_ratio, ai_tool, score_points, score_grade, roast, total_commits, ai_commits)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`
      ).bind(id, githubUsername, repoName, aiRatio, primaryTool, points, grade, roast, totalCommits, aiCommits).run()
    }

    // Record in scan_history
    await db.prepare(
      `INSERT INTO scan_history (repo_fingerprint, repo_name, ai_ratio, score_points) VALUES (?, ?, ?, ?)`
    ).bind(fingerprint, repoName, aiRatio, points).run()

    // Get the actual report ID (might be the existing one if upserted)
    let reportId = id
    if (fingerprint) {
      const existing = await db.prepare(
        `SELECT id FROM reports WHERE repo_fingerprint = ?`
      ).bind(fingerprint).first()
      if (existing) {
        reportId = String(existing.id)
      }
    }

    return c.json({
      id: reportId,
      repo_name: `${owner}/${repo}`,
      ai_ratio: aiRatio,
      total_commits: totalCommits,
      ai_commits: aiCommits,
      human_commits: humanCommits,
      ai_tools: toolCounts,
      score: points,
      grade,
      roast,
      url: `https://vibereport.dev/r/${reportId}`,
    })
  } catch (err: any) {
    if (err.message?.includes('404')) {
      return c.json({ error: `Repository ${owner}/${repo} not found or is private` }, 404)
    }
    return c.json({ error: `Failed to scan: ${err.message}` }, 500)
  }
})

// ── GET /api/reports/list — List all reports, newest first ──
app.get('/api/reports/list', async (c) => {
  const db = c.env.DB
  const page = parseInt(c.req.query('page') || '1')
  const limit = Math.min(parseInt(c.req.query('limit') || '20'), 100)
  const offset = (page - 1) * limit

  const result = await db.prepare(
    `SELECT id, github_username, repo_name, ai_ratio, score_points, score_grade, roast, created_at, updated_at
     FROM reports
     ORDER BY COALESCE(updated_at, created_at) DESC
     LIMIT ? OFFSET ?`
  ).bind(limit, offset).all()

  const countResult = await db.prepare(
    `SELECT COUNT(*) as total FROM reports`
  ).first()

  return c.json({
    reports: result.results,
    total: countResult?.total,
    page,
    limit,
  })
})

// ── GET /api/reports/:id — Get a single report ──
app.get('/api/reports/:id', async (c) => {
  const row = await c.env.DB.prepare(
    `SELECT * FROM reports WHERE id = ?`
  ).bind(c.req.param('id')).first()

  if (!row) {
    return c.json({ error: 'Report not found' }, 404)
  }

  // Map DB columns to frontend-expected field names
  const languages = (() => { try { return JSON.parse(String(row.languages || '{}')) } catch { return {} } })()
  return c.json({
    id: row.id,
    repo_name: row.repo_name ? `${row.github_username || ''}/${row.repo_name}` : row.github_username,
    ai_ratio: row.ai_ratio,
    grade: row.score_grade,
    score: row.score_points,
    roast: row.roast,
    total_commits: row.total_commits || 0,
    ai_commits: row.ai_commits || 0,
    total_lines: row.total_lines,
    has_tests: Boolean(row.has_tests),
    languages,
    created_at: row.created_at,
  })
})

// ── GET /api/leaderboard — Top scores, paginated ──
app.get('/api/leaderboard', async (c) => {
  const db = c.env.DB
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

  const result = await db.prepare(
    `SELECT id, repo_name, github_username, ai_ratio, score_points, score_grade, roast, created_at
     FROM reports
     ${whereClause}
     ORDER BY score_points DESC, created_at DESC
     LIMIT ? OFFSET ?`
  ).bind(limit, offset).all()

  const countResult = await db.prepare(
    `SELECT COUNT(*) as total FROM reports ${whereClause}`
  ).first()

  return c.json({
    reports: result.results,
    total: countResult?.total,
    page,
    limit,
  })
})

// ── GET /api/stats — Aggregate stats ──
app.get('/api/stats', async (c) => {
  const result = await c.env.DB.prepare(
    `SELECT
       COUNT(*) as total_reports,
       AVG(ai_ratio) as avg_ai_ratio,
       AVG(score_points) as avg_score,
       MAX(score_points) as max_score,
       SUM(total_lines) as total_lines_analyzed
     FROM reports`
  ).first()

  return c.json(result || {})
})

// ── GET /api/trends — Monthly scan trends ──
app.get('/api/trends', async (c) => {
  const db = c.env.DB

  const period = c.req.query('period') || '1y'

  // Use reports table (deduplicated) for consistent averages with /api/stats
  const dateCol = "COALESCE(updated_at, created_at)"
  const trendWhere = period === '6m'
    ? `WHERE ${dateCol} > datetime('now', '-6 months')`
    : period === '1y'
    ? `WHERE ${dateCol} > datetime('now', '-1 year')`
    : ''

  const result = await db.prepare(
    `SELECT
       strftime('%Y-%m', ${dateCol}) as month,
       AVG(ai_ratio) as avg_ai_ratio,
       COUNT(*) as total_scans,
       AVG(score_points) as avg_score
     FROM reports
     ${trendWhere}
     GROUP BY strftime('%Y-%m', ${dateCol})
     ORDER BY month ASC`
  ).all()

  return c.json({
    period,
    trends: result.results,
  })
})

// ── GET /api/badge/:id.svg — Dynamic SVG badge ──
app.get('/api/badge/:id.svg', async (c) => {
  const id = (c.req.param('id') ?? '').replace('.svg', '')

  const report = await c.env.DB.prepare(
    `SELECT score_grade, ai_ratio FROM reports WHERE id = ?`
  ).bind(id).first()

  if (!report) {
    return c.text('Not found', 404)
  }
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
