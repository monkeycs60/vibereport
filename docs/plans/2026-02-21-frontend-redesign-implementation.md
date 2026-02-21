# Frontend Redesign — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Simplify vibereport.dev — remove toggle from battle chart, add stacked area trend, merge /reports into /leaderboard, separate AI% from Vibe Score, clean nav.

**Architecture:** Astro SSR + Tailwind frontend, Cloudflare Workers + Hono + D1 API. All changes are frontend-focused except API sort parameter and data purge.

**Tech Stack:** Astro, Tailwind CSS, inline SVG charts, Hono API

---

### Task 1: Simplify BattleChart — Remove toggle, index only, full legend

**Files:**
- Modify: `web/frontend/src/components/BattleChart.astro`

**Step 1: Simplify Props interface**

Replace the current Props (lines 2-10) with index-only props:

```astro
---
interface Props {
  aiCommits: number;
  humanCommits: number;
  totalRepos: number;
  snapshotDate: string | null;
}

const { aiCommits, humanCommits, totalRepos, snapshotDate } = Astro.props;

const total = aiCommits + humanCommits;
const aiPercent = total > 0 ? (aiCommits / total) * 100 : 0;
const humanPercent = total > 0 ? 100 - aiPercent : 100;

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

**Step 2: Remove toggle buttons and simplify HTML**

Remove the entire toggle div (lines 59-82). Remove `data-datasets` and `data-default-mode` from the container. The container becomes:

```html
<div class="tug-of-war w-full" id="tug-of-war">
```

**Step 3: Update labels to use direct values**

Replace `data-ai-count` and `data-human-count` spans with direct values:
```html
<span class="text-xs text-tokyo-dimmed font-mono">{formatNum(aiCommits)} commits</span>
```
(Same for human side)

**Step 4: Update bar data-target-width to use computed values directly**

The bars should use `aiPercent` and `humanPercent` directly (already computed in frontmatter).

**Step 5: Replace legend with full context**

Replace the legend paragraph (line 162-164) with:

```html
<p class="text-center mt-2 text-xs text-tokyo-dimmed/60 font-mono leading-relaxed">
  {formatNum(aiCommits)} AI commits vs {formatNum(humanCommits)} human commits<br />
  across {totalRepos.toLocaleString()} top GitHub repos since Jan 2026 — updated daily
</p>
```

**Step 6: Simplify client-side script**

Remove the entire toggle logic, datasets parsing, and `updateBars` function. Keep only the IntersectionObserver for initial scroll animation:

```html
<script>
  function initTugOfWar() {
    const container = document.getElementById('tug-of-war');
    if (!container) return;

    const aiBar = container.querySelector('.ai-bar') as HTMLElement;
    const humanBar = container.querySelector('.human-bar') as HTMLElement;
    const phrase = document.getElementById('battle-phrase');

    if (aiBar) aiBar.style.width = aiBar.dataset.targetWidth || '0%';
    if (humanBar) humanBar.style.width = humanBar.dataset.targetWidth || '0%';

    if (phrase) {
      setTimeout(() => {
        phrase.classList.remove('opacity-0');
        phrase.classList.add('opacity-100');
      }, 1200);
    }
  }

  const observer = new IntersectionObserver(
    (entries) => {
      entries.forEach((entry) => {
        if (entry.isIntersecting) {
          initTugOfWar();
          observer.unobserve(entry.target);
        }
      });
    },
    { threshold: 0.2 }
  );

  const el = document.getElementById('tug-of-war');
  if (el) observer.observe(el);
</script>
```

**Step 7: Verify locally**

Run: `cd web/frontend && npm run dev`
Check: http://localhost:4321/ — battle chart shows index data, no toggle, full legend.

**Step 8: Commit**

```bash
git add web/frontend/src/components/BattleChart.astro
git commit -m "feat: simplify BattleChart — index only, no toggle, full legend"
```

---

### Task 2: New stacked area TrendChart

**Files:**
- Modify: `web/frontend/src/components/TrendChart.astro`

**Step 1: Update Props to accept index snapshots**

Replace the current Props interface with:

```typescript
interface Props {
  snapshots: Array<{
    snapshot_date: string;
    total_commits: number;
    total_ai_commits: number;
    ai_percent: number;
  }>;
}
```

**Step 2: Rewrite chart as stacked area**

The chart should show two stacked areas:
- Bottom (red/pink): AI% proportion
- Top (blue): Human% proportion
- X-axis: dates
- Y-axis: 0% to 100%

SVG structure:
```
viewBox="0 0 800 250"
- Y grid lines at 0%, 25%, 50%, 75%, 100%
- Y labels on left
- Blue area (human) fills from AI line to top
- Red area (AI) fills from bottom to AI line
- Red line on top of AI area
- Data points as circles
- X-axis date labels
```

Build two paths:
- `aiAreaPath`: from bottom-left, along AI% points, back to bottom-right
- `humanAreaPath`: from AI% line, up to 100%, back down

For single data point (< 2 snapshots): show the point with a label "2.3% AI — Day 1" centered.

**Step 3: Use red/pink gradient for AI area, blue gradient for human area**

```html
<defs>
  <linearGradient id="aiAreaGrad" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0%" stop-color="#f7768e" stop-opacity="0.4" />
    <stop offset="100%" stop-color="#f7768e" stop-opacity="0.05" />
  </linearGradient>
  <linearGradient id="humanAreaGrad" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0%" stop-color="#7aa2f7" stop-opacity="0.05" />
    <stop offset="100%" stop-color="#7aa2f7" stop-opacity="0.3" />
  </linearGradient>
</defs>
```

**Step 4: Format date labels**

"2026-02-21" → "Feb 21"

**Step 5: Commit**

```bash
git add web/frontend/src/components/TrendChart.astro
git commit -m "feat: stacked area TrendChart with AI vs Human proportions"
```

---

### Task 3: Simplify homepage

**Files:**
- Modify: `web/frontend/src/pages/index.astro`

**Step 1: Remove Stats section**

Delete lines 94-109 (the `<section class="py-16 border-y ...">` containing StatsCounter components). Also remove the `StatsCounter` import (line 3) and `fetchStats` from the import + its call.

**Step 2: Remove How it works section**

Delete lines 157-186 (the `<section class="py-16">` containing the 3-card grid Scan/Analyze/Roast).

**Step 3: Update BattleChart props**

Replace the current BattleChart invocation (lines 117-126) with simplified props:

```astro
<BattleChart
  aiCommits={indexAiCommits}
  humanCommits={indexHumanCommits}
  totalRepos={indexLatest.total_repos}
  snapshotDate={indexLatest.snapshot_date}
/>
```

Remove the community data computation (lines 22-23: `communityAiCommits`, `communityHumanCommits`).

**Step 4: Update TrendChart to use index snapshots**

Replace the TrendChart section (lines 130-155) with:

```astro
<section class="py-16">
  <div class="max-w-4xl mx-auto px-4 sm:px-6 lg:px-8">
    <h2 class="text-2xl font-bold text-center text-tokyo-text mb-8">
      The rise of vibe coding
    </h2>
    <TrendChart snapshots={indexTrend.snapshots} />
    <div class="text-center mt-4">
      <a href="/trends" class="text-sm text-tokyo-cyan hover:text-tokyo-cyan/80 transition-colors">
        View full trends &rarr;
      </a>
    </div>
  </div>
</section>
```

**Step 5: Clean up unused imports**

Remove: `StatsCounter`, `fetchStats`, `fetchTrends` if no longer used. Keep: `fetchIndexLatest`, `fetchIndexTrend`, `fetchLeaderboard`.

**Step 6: Commit**

```bash
git add web/frontend/src/pages/index.astro
git commit -m "feat: simplify homepage — remove stats header and how it works"
```

---

### Task 4: Update scan page — AI% hero, Vibe Score secondary

**Files:**
- Modify: `web/frontend/src/pages/scan.astro`

**Step 1: Reorganize result card layout**

In the `#scan-result` div (line 67), restructure the header to show AI% prominently:

```html
<!-- Header: AI% as hero metric -->
<div class="text-center mb-5">
  <h3 id="result-repo" class="text-xl font-bold text-tokyo-text mb-3"></h3>
  <div class="text-5xl font-bold text-[#f7768e] mb-1" id="result-ai-pct-hero"></div>
  <p class="text-sm text-tokyo-dimmed">AI-generated commits</p>
</div>

<!-- AI vs Human bar (unchanged) -->
...

<!-- Vibe Score + Grade (secondary) -->
<div class="flex items-center justify-center gap-6 mb-5 p-4 rounded-xl bg-tokyo-bg/40 border border-tokyo-border/20">
  <div class="text-center">
    <p class="text-xs text-tokyo-dimmed mb-1">Vibe Score</p>
    <span id="result-score" class="text-2xl font-bold"></span>
  </div>
  <div class="h-10 w-px bg-tokyo-border/30"></div>
  <div class="text-center">
    <p class="text-xs text-tokyo-dimmed mb-1">Grade</p>
    <div id="result-grade" class="w-14 h-14 flex items-center justify-center rounded-xl text-2xl font-bold border-2"></div>
  </div>
</div>
```

**Step 2: Update JS to populate new layout**

Add: `document.getElementById('result-ai-pct-hero').textContent = aiPct + '%';`

Move the grade badge population to the new secondary location.

**Step 3: Commit**

```bash
git add web/frontend/src/pages/scan.astro
git commit -m "feat: scan page — AI% as hero metric, Vibe Score secondary"
```

---

### Task 5: Leaderboard — configurable sort, absorb reports

**Files:**
- Modify: `web/frontend/src/pages/leaderboard.astro`
- Modify: `web/frontend/src/components/LeaderboardTable.astro`
- Modify: `web/api/src/index.ts` (add `sort` query param)

**Step 1: Add sort param to API**

In `web/api/src/index.ts`, line 637, replace `ORDER BY score_points DESC` with dynamic sort:

```typescript
const sort = c.req.query('sort') || 'score'
const orderBy = sort === 'ai' ? 'ai_ratio DESC' : 'score_points DESC'
```

Use `orderBy` in the SQL query.

**Step 2: Add sort toggle to leaderboard page**

Add sort options alongside period filters in `leaderboard.astro`:

```astro
<!-- Sort toggle -->
<div class="flex items-center gap-2 mb-4">
  <span class="text-xs text-tokyo-dimmed">Sort by:</span>
  <a href={`/leaderboard?sort=ai${periodSuffix}`}
     class:list={[...active/inactive styles based on current sort]}>
    AI %
  </a>
  <a href={`/leaderboard?sort=score${periodSuffix}`}
     class:list={[...active/inactive styles based on current sort]}>
    Vibe Score
  </a>
</div>
```

**Step 3: Update LeaderboardTable to show Vibe Score column**

Add a Score column to the table between AI% and Grade:
```html
<th class="text-center py-3 px-3 w-20">Score</th>
```

And in each row:
```html
<td class="py-3 px-3 text-center">
  <span class="text-xs text-tokyo-dimmed tabular-nums">{entry.score}</span>
</td>
```

**Step 4: Pass sort param through API call**

In `api.ts`, update `fetchLeaderboard` to accept and forward `sort` param.

**Step 5: Deploy API**

```bash
cd web/api && npx wrangler deploy
```

**Step 6: Commit**

```bash
git add web/api/src/index.ts web/frontend/src/pages/leaderboard.astro web/frontend/src/components/LeaderboardTable.astro web/frontend/src/lib/api.ts
git commit -m "feat: leaderboard — configurable sort by AI% or Vibe Score"
```

---

### Task 6: Update report page — AI% spotlight

**Files:**
- Modify: `web/frontend/src/pages/report.astro`

**Step 1: Make AI% the hero metric**

In the report card body (line 58), reorganize:

```html
<!-- AI% hero -->
<div class="text-center mb-6">
  <div id="rpt-ai-pct-label" class="text-5xl font-bold text-[#f7768e]"></div>
  <p class="text-sm text-tokyo-dimmed mt-1">AI-generated commits</p>
</div>

<!-- AI ratio bar (unchanged) -->
...

<!-- Vibe Score + Grade (secondary, side by side) -->
<div class="flex items-center justify-center gap-6 p-4 rounded-xl bg-tokyo-bg/40 border border-tokyo-border/20 mb-6">
  <div class="text-center">
    <p class="text-xs text-tokyo-dimmed mb-1">Vibe Score</p>
    <span id="rpt-score" class="text-2xl font-bold"></span>
    <span class="text-sm text-tokyo-dimmed">/100</span>
  </div>
  <div class="h-10 w-px bg-tokyo-border/30"></div>
  <div class="text-center">
    <p class="text-xs text-tokyo-dimmed mb-1">Grade</p>
    <span id="rpt-grade-text" class="text-2xl font-bold"></span>
  </div>
</div>
```

**Step 2: Commit**

```bash
git add web/frontend/src/pages/report.astro
git commit -m "feat: report page — AI% as hero, Vibe Score secondary"
```

---

### Task 7: Refactor trends page — index + community

**Files:**
- Modify: `web/frontend/src/pages/trends.astro`

**Step 1: Fetch index trend data**

Add `fetchIndexTrend` import and call:
```astro
import { fetchTrends, fetchIndexTrend } from '../lib/api';
let indexTrend = { snapshots: [] };
try { indexTrend = await fetchIndexTrend(); } catch {}
```

**Step 2: Replace main chart with index stacked area**

Replace the current TrendChart call with:
```astro
<TrendChart snapshots={indexTrend.snapshots} />
```

**Step 3: Update stats cards**

Replace the 3 stat cards with index-relevant ones:
- Repos tracked: `indexTrend.snapshots[last].total_repos` or "844"
- Current AI%: from latest snapshot
- Days tracked: `indexTrend.snapshots.length`

**Step 4: Add community section below**

```astro
<section class="py-12 border-t border-tokyo-border/20 mt-12">
  <h2 class="text-xl font-bold text-tokyo-text mb-6">Community</h2>
  <p class="text-sm text-tokyo-dimmed mb-4">
    Stats from repos scanned by users via the CLI or web scanner.
  </p>
  <TrendChart snapshots={trends.trends.map(t => ({
    snapshot_date: t.month,
    total_commits: t.total_commits || 0,
    total_ai_commits: t.ai_commits || 0,
    ai_percent: t.avg_ai_ratio * 100,
  }))} />
</section>
```

**Step 5: Commit**

```bash
git add web/frontend/src/pages/trends.astro
git commit -m "feat: trends page — index stacked area + community section"
```

---

### Task 8: Simplify navigation + delete /reports

**Files:**
- Modify: `web/frontend/src/layouts/Base.astro`
- Delete: `web/frontend/src/pages/reports.astro`

**Step 1: Update nav links**

Replace the nav links (Base.astro lines 89-127) with 4 links:

```html
<div class="flex items-center gap-6 text-sm">
  <a href="/scan" class="text-tokyo-dimmed hover:text-tokyo-text transition-colors">scan</a>
  <a href="/leaderboard" class="text-tokyo-dimmed hover:text-tokyo-text transition-colors">leaderboard</a>
  <a href="/trends" class="text-tokyo-dimmed hover:text-tokyo-text transition-colors">trends</a>
  <a href="https://github.com/vibereport/vibereport" target="_blank" rel="noopener noreferrer" class="text-tokyo-dimmed hover:text-tokyo-text transition-colors">github</a>
</div>
```

Remove: "home" link (logo serves as home) and "reports" link.

**Step 2: Delete reports.astro**

```bash
rm web/frontend/src/pages/reports.astro
```

**Step 3: Commit**

```bash
git add web/frontend/src/layouts/Base.astro
git rm web/frontend/src/pages/reports.astro
git commit -m "feat: simplify nav to 4 links, remove /reports page"
```

---

### Task 9: Purge test data + backfill snapshots

**Step 1: Purge community test reports**

```bash
cd web/api && npx wrangler d1 execute vibereport-db --remote \
  --command "DELETE FROM reports;"
```

Verify:
```bash
npx wrangler d1 execute vibereport-db --remote \
  --command "SELECT COUNT(*) as count FROM reports;"
```
Expected: `count: 0`

**Step 2: Backfill index snapshots for Feb 19 and Feb 20**

The scan results are effectively the same (same repos, same commits since Jan 1). We can copy the existing Feb 21 snapshot with adjusted dates:

```bash
cd web/api && npx wrangler d1 execute vibereport-db --remote \
  --command "INSERT INTO index_snapshots (snapshot_date, quarter, total_repos, total_commits, total_ai_commits, ai_percent) VALUES ('2026-02-19', '2026-Q1', 844, 676891, 15839, 2.34) ON CONFLICT DO NOTHING;"

npx wrangler d1 execute vibereport-db --remote \
  --command "INSERT INTO index_snapshots (snapshot_date, quarter, total_repos, total_commits, total_ai_commits, ai_percent) VALUES ('2026-02-20', '2026-Q1', 844, 676891, 15839, 2.34) ON CONFLICT DO NOTHING;"
```

Verify:
```bash
npx wrangler d1 execute vibereport-db --remote \
  --command "SELECT * FROM index_snapshots ORDER BY snapshot_date;"
```
Expected: 3 rows (Feb 19, 20, 21).

**Step 3: Verify index-trend API returns 3 snapshots**

```bash
curl -s 'https://vibereport-api.clement-serizay.workers.dev/api/index-trend' | python3 -m json.tool
```

**Step 4: Commit (no code changes, just data ops — no commit needed)**

---

### Task 10: Deploy + verify

**Step 1: Deploy API (if changed in Task 5)**

```bash
cd web/api && npx wrangler deploy
```

**Step 2: Push to GitHub (triggers Vercel deploy)**

```bash
git push origin master
```

**Step 3: Verify locally**

- Homepage: 5 sections, no toggle on battle, full legend, stacked area trend
- /scan: AI% hero, Vibe Score secondary
- /leaderboard: sort toggle works, shows score column
- /trends: index chart + community section
- /reports: 404 or redirected
- Nav: 4 links

**Step 4: Verify production**

Check https://vibereport.dev after Vercel deploy completes.

---

## Task Dependencies

```
Task 1 (BattleChart) ──┐
Task 2 (TrendChart) ───┤
Task 8 (Nav + delete) ─┼── Task 3 (Homepage) ── Task 10 (Deploy)
Task 9 (Purge/backfill)┤
Task 4 (Scan page) ────┤
Task 5 (Leaderboard) ──┤
Task 6 (Report page) ──┤
Task 7 (Trends page) ──┘
```

Tasks 1, 2, 4, 5, 6, 7, 8, 9 are independent and can run in parallel.
Task 3 depends on Tasks 1 and 2 (new component APIs).
Task 10 depends on all others.
