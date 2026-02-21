// SQL Schema: see schema.sql (apply with `wrangler d1 execute`)

import { Hono } from 'hono'
import { cors } from 'hono/cors'

// ── Rate limiting (in-memory, per-isolate) ──
const rateLimitMap = new Map<string, { count: number; resetAt: number }>();

function checkRateLimit(ip: string, limit: number, windowMs: number): boolean {
  const now = Date.now();
  const entry = rateLimitMap.get(ip);
  if (!entry || now > entry.resetAt) {
    rateLimitMap.set(ip, { count: 1, resetAt: now + windowMs });
    return true;
  }
  if (entry.count >= limit) return false;
  entry.count++;
  return true;
}

type Bindings = {
  DB: D1Database
  GITHUB_TOKEN?: string
  VPS_SCAN_URL?: string
  VPS_AUTH_TOKEN?: string
}

const app = new Hono<{ Bindings: Bindings }>()

// CORS for frontend
app.use('/*', cors({
  origin: (origin) => {
    if (!origin) return 'https://www.vibereport.dev'
    if (origin.startsWith('http://localhost:')) return origin
    if (origin === 'https://vibereport.dev' || origin === 'https://www.vibereport.dev') return origin
    if (origin.endsWith('.vibereport.pages.dev') || origin === 'https://vibereport.pages.dev') return origin
    if (origin.endsWith('.clement-serizay.workers.dev')) return origin
    return 'https://www.vibereport.dev'
  },
  allowMethods: ['GET', 'POST', 'OPTIONS'],
  allowHeaders: ['Content-Type', 'Authorization'],
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

function getCurrentQuarter(): string {
  const now = new Date()
  const q = Math.ceil((now.getMonth() + 1) / 3)
  return `${now.getFullYear()}-Q${q}`
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
const WEB_MAX_PAGES = 50 // ~5000 commits max per web-triggered scan

async function analyzeGitHubRepo(owner: string, repo: string, token?: string, maxPages: number = MAX_PAGES): Promise<ScanResult> {
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

  // 3. Fetch remaining pages in parallel batches (capped at maxPages)
  const pagesToFetch = Math.min(lastPage, maxPages)
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

// Valid chaos badges allowlist
const VALID_BADGES = ['env-in-git', 'hardcoded-secrets', 'no-tests', 'dependency-hell', 'no-linting', 'no-ci', 'boomer-ai', 'node-modules-in-git', 'mega-commit', 'no-gitignore', 'no-readme', 'todo-flood', 'single-branch'];

// ── POST /api/reports — Submit a new report ──
app.post('/api/reports', async (c) => {
  const ip = c.req.header('cf-connecting-ip') || 'unknown';
  if (!checkRateLimit(ip, 10, 60000)) {
    return c.json({ error: 'Rate limit exceeded, try again later' }, 429);
  }

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

  // FIX 4: Input validation on string fields
  if (typeof body.github_username === 'string' && body.github_username.length > 255) {
    return c.json({ error: 'github_username must be under 255 chars' }, 400)
  }
  if (typeof body.repo_name === 'string' && body.repo_name.length > 255) {
    return c.json({ error: 'repo_name must be under 255 chars' }, 400)
  }
  if (typeof body.ai_tool === 'string' && body.ai_tool.length > 100) {
    return c.json({ error: 'ai_tool must be under 100 chars' }, 400)
  }
  if (typeof body.languages === 'string') {
    if (body.languages.length > 10000) {
      return c.json({ error: 'languages must be under 10000 chars' }, 400)
    }
    try { JSON.parse(body.languages) } catch {
      return c.json({ error: 'languages must be valid JSON' }, 400)
    }
  }
  if (typeof body.chaos_badges === 'string' && body.chaos_badges.length > 5000) {
    return c.json({ error: 'chaos_badges must be under 5000 chars' }, 400)
  }
  if (typeof body.repo_fingerprint === 'string' && body.repo_fingerprint.length > 500) {
    return c.json({ error: 'repo_fingerprint must be under 500 chars' }, 400)
  }

  // FIX 2: Server-side re-derive score_grade and roast (ignore client values)
  const scoreGrade = gradeFromPoints(body.score_points as number)
  const roast = pickRoast(body.score_points as number, body.ai_ratio as number)

  // FIX 2: Validate chaos_badges against allowlist
  let validatedBadges = '[]'
  if (typeof body.chaos_badges === 'string') {
    try {
      const parsed = JSON.parse(body.chaos_badges)
      if (Array.isArray(parsed)) {
        const filtered = parsed.filter((b: unknown) => typeof b === 'string' && VALID_BADGES.includes(b))
        validatedBadges = JSON.stringify(filtered)
      }
    } catch {
      // Invalid JSON, use empty array
    }
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
  const totalCommits = typeof body.total_commits === 'number' ? body.total_commits : 0
  const aiCommits = typeof body.ai_commits === 'number' ? body.ai_commits : 0
  const vibeScore = typeof body.vibe_score === 'number' ? body.vibe_score : (body.score_points as number)

  if (fingerprint) {
    // Upsert: update existing report if fingerprint matches
    await db.prepare(
      `INSERT INTO reports (id, repo_fingerprint, github_username, repo_name, ai_ratio, ai_tool, score_points, score_grade, roast, deps_count, has_tests, total_lines, languages, total_commits, ai_commits, vibe_score, chaos_badges, scan_source, updated_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'cli', datetime('now'))
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
         total_commits = excluded.total_commits,
         ai_commits = excluded.ai_commits,
         vibe_score = excluded.vibe_score,
         chaos_badges = excluded.chaos_badges,
         scan_source = 'cli',
         updated_at = datetime('now')`
    ).bind(
      id, fingerprint, githubUsername, repoName,
      body.ai_ratio, aiTool, body.score_points, scoreGrade, roast,
      depsCount, hasTests, totalLines, languages,
      totalCommits, aiCommits, vibeScore, validatedBadges,
    ).run()
  } else {
    // No fingerprint: plain insert (for backwards compatibility)
    await db.prepare(
      `INSERT INTO reports (id, github_username, repo_name, ai_ratio, ai_tool, score_points, score_grade, roast, deps_count, has_tests, total_lines, languages, total_commits, ai_commits, vibe_score, chaos_badges, scan_source)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'cli')`
    ).bind(
      id, githubUsername, repoName,
      body.ai_ratio, aiTool, body.score_points, scoreGrade, roast,
      depsCount, hasTests, totalLines, languages,
      totalCommits, aiCommits, vibeScore, validatedBadges,
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
    url: `https://www.vibereport.dev/r/${id}`,
    rank,
    percentile: Math.round(percentile * 10) / 10,
  })
})

// ── POST /api/scan — Scan a public GitHub repo ──
app.post('/api/scan', async (c) => {
  const ip = c.req.header('cf-connecting-ip') || 'unknown';
  if (!checkRateLimit(ip, 5, 60000)) {
    return c.json({ error: 'Rate limit exceeded, try again later' }, 429);
  }

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

  // FIX 9: Check for cached recent scan (within 10 minutes)
  const db = c.env.DB
  const cached = await db.prepare(
    `SELECT sh.repo_fingerprint, sh.repo_name, sh.ai_ratio, sh.score_points, sh.scanned_at,
            r.id, r.ai_tool, r.total_commits, r.ai_commits, r.score_grade, r.roast, r.chaos_badges, r.scan_source
     FROM scan_history sh
     LEFT JOIN reports r ON r.repo_fingerprint = sh.repo_fingerprint
     WHERE sh.repo_name = ? AND sh.scanned_at > datetime('now', '-10 minutes')
     ORDER BY sh.scanned_at DESC LIMIT 1`
  ).bind(repo).first()
  if (cached && cached.id) {
    const chaosBadges = (() => { try { return JSON.parse(String(cached.chaos_badges || '[]')) } catch { return [] } })()
    return c.json({
      id: cached.id,
      repo_name: `${owner}/${repo}`,
      ai_ratio: cached.ai_ratio,
      total_commits: cached.total_commits || 0,
      ai_commits: cached.ai_commits || 0,
      human_commits: (Number(cached.total_commits) || 0) - (Number(cached.ai_commits) || 0),
      ai_tools: {},
      score: cached.score_points,
      grade: cached.score_grade,
      roast: cached.roast,
      chaos_badges: chaosBadges,
      scan_source: cached.scan_source || 'cached',
      url: `https://www.vibereport.dev/r/${cached.id}`,
    })
  }

  // ── Try VPS worker first (full git clone analysis with vibe detectors) ──
  const vpsUrl = c.env.VPS_SCAN_URL
  const vpsToken = c.env.VPS_AUTH_TOKEN
  if (vpsUrl && vpsToken) {
    try {
      const vpsRes = await fetch(`${vpsUrl}/scan`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${vpsToken}`,
        },
        body: JSON.stringify({ repo: repoInput, since: '2025-01-01' }),
        signal: AbortSignal.timeout(45000),
      })
      if (vpsRes.ok) {
        const vpsData = await vpsRes.json() as Record<string, any>

        // Map VPS vibereport --json output to scan response format
        const aiRatio: number = typeof vpsData.ai_ratio === 'number' ? vpsData.ai_ratio : 0
        const totalCommits: number = typeof vpsData.total_commits === 'number' ? vpsData.total_commits : 0
        const aiCommits: number = typeof vpsData.ai_commits === 'number' ? vpsData.ai_commits : 0
        const humanCommits: number = typeof vpsData.human_commits === 'number' ? vpsData.human_commits : totalCommits - aiCommits
        const vibeScore: number = typeof vpsData.vibe_score === 'number' ? vpsData.vibe_score : (typeof vpsData.score === 'number' ? vpsData.score : 0)
        const grade: string = typeof vpsData.grade === 'string' ? vpsData.grade : gradeFromPoints(vibeScore)
        const roast: string = typeof vpsData.roast === 'string' ? vpsData.roast : pickRoast(vibeScore, aiRatio)

        // Map ai_tools from VPS format [{tool, commits}] to {tool: count} for response
        const toolCounts: Record<string, number> = {}
        if (Array.isArray(vpsData.ai_tools)) {
          for (const t of vpsData.ai_tools) {
            if (t.tool && t.tool !== 'Human') toolCounts[t.tool] = t.commits || 0
          }
        }
        const primaryTool = Object.entries(toolCounts)
          .sort(([, a], [, b]) => b - a)[0]?.[0] || 'Human'

        // Derive chaos badges from VPS vibe object
        const chaosBadges: string[] = []
        const vibe = vpsData.vibe
        if (vibe && typeof vibe === 'object') {
          if (vibe.no_linting) chaosBadges.push('no-linting')
          if (vibe.no_ci_cd) chaosBadges.push('no-ci-cd')
          if (vibe.boomer_ai) chaosBadges.push('boomer-ai')
          if (vibe.node_modules_in_git) chaosBadges.push('node-modules')
          if (vibe.no_gitignore) chaosBadges.push('no-gitignore')
          if (vibe.no_readme) chaosBadges.push('no-readme')
          if (vibe.todo_flood) chaosBadges.push('todo-flood')
          if (vibe.single_branch) chaosBadges.push('single-branch')
          if (vibe.mega_commit) chaosBadges.push('mega-commit')
        }
        // Add badges from other data
        const tests = vpsData.tests
        if (tests && typeof tests === 'object' && !tests.has_tests) chaosBadges.push('no-tests')
        const security = vpsData.security
        if (security && typeof security === 'object' && security.env_in_git) chaosBadges.push('env-in-git')

        // Compute fingerprint — VPS doesn't return oldest commit sha, use repo URL
        const fingerprint = `vps:https://github.com/${owner}/${repo}`

        // Store in DB with scan_source = 'web_vps'
        const id = generateId()

        await db.prepare(
          `INSERT INTO reports (id, repo_fingerprint, github_username, repo_name, ai_ratio, ai_tool, score_points, score_grade, roast, total_commits, ai_commits, deps_count, has_tests, total_lines, languages, vibe_score, chaos_badges, scan_source, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'web_vps', datetime('now'))
           ON CONFLICT(repo_fingerprint) DO UPDATE SET
             ai_ratio = excluded.ai_ratio,
             ai_tool = excluded.ai_tool,
             score_points = excluded.score_points,
             score_grade = excluded.score_grade,
             roast = excluded.roast,
             total_commits = excluded.total_commits,
             ai_commits = excluded.ai_commits,
             deps_count = excluded.deps_count,
             has_tests = excluded.has_tests,
             total_lines = excluded.total_lines,
             languages = excluded.languages,
             vibe_score = excluded.vibe_score,
             chaos_badges = excluded.chaos_badges,
             scan_source = 'web_vps',
             updated_at = datetime('now')`
        ).bind(
          id, fingerprint, owner, repo, aiRatio, primaryTool, vibeScore, grade, roast,
          totalCommits, aiCommits,
          typeof vpsData.deps?.total === 'number' ? vpsData.deps.total : 0,
          tests?.has_tests ? 1 : 0,
          typeof vpsData.total_lines === 'number' ? vpsData.total_lines : 0,
          typeof vpsData.languages === 'object' ? JSON.stringify(vpsData.languages) : '{}',
          vibeScore,
          JSON.stringify(chaosBadges),
        ).run()

        // Record in scan_history
        await db.prepare(
          `INSERT INTO scan_history (repo_fingerprint, repo_name, ai_ratio, score_points) VALUES (?, ?, ?, ?)`
        ).bind(fingerprint, repo, aiRatio, vibeScore).run()

        // Get the actual report ID (might be existing if upserted)
        let reportId = id
        const existing = await db.prepare(
          `SELECT id FROM reports WHERE repo_fingerprint = ?`
        ).bind(fingerprint).first()
        if (existing) {
          reportId = String(existing.id)
        }

        return c.json({
          id: reportId,
          repo_name: `${owner}/${repo}`,
          ai_ratio: aiRatio,
          total_commits: totalCommits,
          ai_commits: aiCommits,
          human_commits: humanCommits,
          ai_tools: toolCounts,
          score: vibeScore,
          grade,
          roast,
          chaos_badges: chaosBadges,
          scan_source: 'web_vps',
          url: `https://www.vibereport.dev/r/${reportId}`,
        })
      }
      // VPS returned non-ok status — fall through to GitHub API
      console.log(`VPS scan returned ${vpsRes.status}, falling back to GitHub API`)
    } catch (vpsErr: any) {
      // VPS timeout or network error — fall through to GitHub API
      console.log(`VPS scan failed: ${vpsErr.message}, falling back to GitHub API`)
    }
  }

  // ── Fallback: GitHub API commit analysis ──
  try {
    // Analyze commits via parallel fetching (capped at WEB_MAX_PAGES for web scans)
    const scan = await analyzeGitHubRepo(owner, repo, c.env.GITHUB_TOKEN, WEB_MAX_PAGES)

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
    const id = generateId()
    const repoName = repo
    const githubUsername = owner

    if (fingerprint) {
      await db.prepare(
        `INSERT INTO reports (id, repo_fingerprint, github_username, repo_name, ai_ratio, ai_tool, score_points, score_grade, roast, total_commits, ai_commits, deps_count, has_tests, total_lines, languages, scan_source, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 0, 0, '{}', 'web_github', datetime('now'))
         ON CONFLICT(repo_fingerprint) DO UPDATE SET
           ai_ratio = excluded.ai_ratio,
           ai_tool = excluded.ai_tool,
           score_points = excluded.score_points,
           score_grade = excluded.score_grade,
           roast = excluded.roast,
           total_commits = excluded.total_commits,
           ai_commits = excluded.ai_commits,
           scan_source = 'web_github',
           updated_at = datetime('now')`
      ).bind(id, fingerprint, githubUsername, repoName, aiRatio, primaryTool, points, grade, roast, totalCommits, aiCommits).run()
    } else {
      await db.prepare(
        `INSERT INTO reports (id, github_username, repo_name, ai_ratio, ai_tool, score_points, score_grade, roast, total_commits, ai_commits, scan_source)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'web_github')`
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

    // Derive chaos badges from available data
    // Full badge detection happens in the CLI/VPS worker with filesystem access.
    // Here we derive what we can from commit-level analysis only.
    const chaosBadges: string[] = []
    if (aiRatio > 0 && Object.keys(toolCounts).length > 0) {
      // Check if AI is used but no dedicated AI tool config is detectable
      // (we can't check filesystem from GitHub API, so skip boomer-ai)
    }
    // These badges require filesystem access — only VPS worker can detect them:
    // env-in-git, hardcoded-secrets, no-tests, no-linting, no-ci-cd,
    // boomer-ai, node-modules, no-gitignore, no-readme, todo-flood,
    // single-branch, mega-commit

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
      chaos_badges: chaosBadges,
      scan_source: 'web_github',
      url: `https://www.vibereport.dev/r/${reportId}`,
    })
  } catch (err: any) {
    if (err.message?.includes('404')) {
      return c.json({ error: `Repository ${owner}/${repo} not found or is private` }, 404)
    }
    console.error('Scan failed:', err.message)
    return c.json({ error: 'Scan failed, please try again later' }, 500)
  }
})

// ── GET /api/reports/list — List all reports, newest first ──
app.get('/api/reports/list', async (c) => {
  const ip = c.req.header('cf-connecting-ip') || 'unknown';
  if (!checkRateLimit(ip, 60, 60000)) {
    return c.json({ error: 'Rate limit exceeded, try again later' }, 429);
  }

  const db = c.env.DB
  const page = Math.max(1, parseInt(c.req.query('page') || '1') || 1)
  const limit = Math.min(Math.max(1, parseInt(c.req.query('limit') || '20') || 20), 100)
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
  const ip = c.req.header('cf-connecting-ip') || 'unknown';
  if (!checkRateLimit(ip, 60, 60000)) {
    return c.json({ error: 'Rate limit exceeded, try again later' }, 429);
  }

  const row = await c.env.DB.prepare(
    `SELECT * FROM reports WHERE id = ?`
  ).bind(c.req.param('id')).first()

  if (!row) {
    return c.json({ error: 'Report not found' }, 404)
  }

  // Map DB columns to frontend-expected field names
  const languages = (() => { try { return JSON.parse(String(row.languages || '{}')) } catch { return {} } })()
  const chaosBadges = (() => { try { return JSON.parse(String(row.chaos_badges || '[]')) } catch { return [] } })()
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
    deps_count: row.deps_count || 0,
    chaos_badges: chaosBadges,
    scan_source: row.scan_source || 'cli',
    languages,
    created_at: row.created_at,
  })
})

// ── GET /api/leaderboard — Top scores, paginated ──
app.get('/api/leaderboard', async (c) => {
  const ip = c.req.header('cf-connecting-ip') || 'unknown';
  if (!checkRateLimit(ip, 60, 60000)) {
    return c.json({ error: 'Rate limit exceeded, try again later' }, 429);
  }

  const db = c.env.DB
  const page = Math.max(1, parseInt(c.req.query('page') || '1') || 1)
  const limit = Math.min(Math.max(1, parseInt(c.req.query('limit') || '20') || 20), 100)
  const offset = (page - 1) * limit

  // Sort param
  const sort = c.req.query('sort') || 'score'
  const orderBy = sort === 'ai' ? 'ai_ratio DESC' : 'score_points DESC'

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
     ORDER BY ${orderBy}, created_at DESC
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
  const ip = c.req.header('cf-connecting-ip') || 'unknown';
  if (!checkRateLimit(ip, 60, 60000)) {
    return c.json({ error: 'Rate limit exceeded, try again later' }, 429);
  }

  const result = await c.env.DB.prepare(
    `SELECT
       COUNT(*) as total_reports,
       AVG(ai_ratio) as avg_ai_ratio,
       AVG(score_points) as avg_score,
       MAX(score_points) as max_score,
       SUM(total_commits) as total_commits,
       SUM(ai_commits) as total_ai_commits
     FROM reports`
  ).first()

  return c.json(result || {})
})

// ── GET /api/trends — Monthly scan trends ──
app.get('/api/trends', async (c) => {
  const ip = c.req.header('cf-connecting-ip') || 'unknown';
  if (!checkRateLimit(ip, 60, 60000)) {
    return c.json({ error: 'Rate limit exceeded, try again later' }, 429);
  }

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
       AVG(score_points) as avg_score,
       SUM(total_commits) as total_commits,
       SUM(ai_commits) as ai_commits
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
  const ip = c.req.header('cf-connecting-ip') || 'unknown';
  if (!checkRateLimit(ip, 60, 60000)) {
    return c.json({ error: 'Rate limit exceeded, try again later' }, 429);
  }

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

// ── GET /api/index-panel — Repo list for current quarter ──
app.get('/api/index-panel', async (c) => {
  const ip = c.req.header('cf-connecting-ip') || 'unknown';
  if (!checkRateLimit(ip, 60, 60000)) {
    return c.json({ error: 'Rate limit exceeded, try again later' }, 429);
  }

  const quarter = c.req.query('quarter') || getCurrentQuarter()
  const result = await c.env.DB.prepare(
    `SELECT repo_slug, panel_source, stars FROM index_panel WHERE quarter = ? ORDER BY stars DESC`
  ).bind(quarter).all()
  return c.json({ quarter, repos: result.results })
})

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

  // FIX 6: Validate scan_date format
  if (!/^\d{4}-\d{2}-\d{2}$/.test(scanDate)) {
    return c.json({ error: 'scan_date must match YYYY-MM-DD format' }, 400)
  }

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

// ── GET /api/index-latest — Latest index snapshot for frontend ──
app.get('/api/index-latest', async (c) => {
  const ip = c.req.header('cf-connecting-ip') || 'unknown';
  if (!checkRateLimit(ip, 60, 60000)) {
    return c.json({ error: 'Rate limit exceeded, try again later' }, 429);
  }

  const row = await c.env.DB.prepare(
    `SELECT * FROM index_snapshots ORDER BY snapshot_date DESC LIMIT 1`
  ).first()
  if (!row) {
    return c.json({ snapshot_date: null, total_repos: 0, total_commits: 0, total_ai_commits: 0, ai_percent: 0 })
  }
  return c.json(row)
})

// ── GET /api/index-trend — Index snapshots over time ──
app.get('/api/index-trend', async (c) => {
  const ip = c.req.header('cf-connecting-ip') || 'unknown';
  if (!checkRateLimit(ip, 60, 60000)) {
    return c.json({ error: 'Rate limit exceeded, try again later' }, 429);
  }

  const result = await c.env.DB.prepare(
    `SELECT snapshot_date, total_repos, total_commits, total_ai_commits, ai_percent
     FROM index_snapshots
     ORDER BY snapshot_date ASC`
  ).all()
  return c.json({ snapshots: result.results })
})

// ── Health check ──
app.get('/api/health', (c) => c.json({ status: 'ok' }))

// Export with scheduled handler for cron trigger
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
        body: JSON.stringify({}),
      })
    } catch (err: any) {
      console.error('Cron trigger failed:', err.message)
    }
  },
}
