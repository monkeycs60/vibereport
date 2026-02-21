# Frontend Redesign — Design Document

**Goal:** Simplify and clarify vibereport.dev — remove confusion between Index and Community data, make AI% the hero metric, and streamline navigation.

## Summary of Changes

| What | Before | After |
|---|---|---|
| Homepage sections | 7 (hero, stats, battle, trend, how it works, leaderboard, CTA) | 5 (hero, battle, trend, leaderboard, CTA) |
| Battle Chart | Toggle between GitHub Index / Community | GitHub Index only, no toggle |
| Battle legend | "Based on 844 top GitHub repos" | Full context: commits counts, repo count, time window, update frequency |
| Trend chart | Line chart, community data (nearly empty) | Stacked area chart, index daily snapshots |
| Scoring display | Vibe Score only (confusing mix) | Two separate metrics: AI% (factual) + Vibe Score (judgment) |
| Navigation | 6 links: home, leaderboard, trends, reports, scan, github | 4 links: scan, leaderboard, trends, github |
| /reports page | Separate listing of all scans | Merged into /leaderboard |
| Leaderboard sort | By vibe score only | Configurable: AI% or Vibe Score |
| Test data | 17 community reports | Purged |

## Homepage Structure

### 1. Hero (unchanged)
- Title: VIBE REPORT
- Subtitle: "The Spotify Wrapped for your code"
- Scan input + button
- `cargo install vibereport`

### 2. Battle Chart (simplified)
- **No toggle.** Shows GitHub Index data only.
- Tug of war bar: AI (red/pink, left) vs Humans (blue, right)
- Labels: emoji + name + commit count on each side
- Percentage inside bars
- Dynamic phrase ("Humans are winning... for now.")
- **Legend (full context):**
  ```
  15,839 AI commits vs 661,052 human commits
  across 844 top GitHub repos since Jan 2026 — updated daily
  ```

### 3. Trend Chart (new: stacked area)
- Title: "The rise of vibe coding"
- **Stacked area chart** fed by `index_snapshots` (daily)
- X-axis: dates (day by day)
- Y-axis: 0% to 100%
- Bottom zone (red/pink): AI proportion
- Top zone (blue): Human proportion
- When < 2 data points: show single point with label "Day 1 of tracking"
- Link: "View full trends →"

### 4. Leaderboard Top 5 (unchanged structure)
- Shows top 5 community reports by default sort

### 5. CTA (unchanged)
- "Ready to expose your codebase?"
- Scan a repo online / cargo install vibereport

### Removed Sections
- **Stats header** (repos scanned / commits analyzed): removed — the battle chart provides better stats
- **How it works** (Scan/Analyze/Roast): removed — the hero already explains the concept

## Page Changes

### /scan — Two Separate Metrics
The scan result card reorganized:
1. **AI%** as hero metric (big number, prominent bar)
2. **Vibe Score + Grade** as secondary (fun judgment)
3. Chaos badges (unchanged)
4. Languages (unchanged)
5. Roast (unchanged)
6. Share buttons (unchanged)

### /leaderboard — Absorbs /reports
- **Sort toggle**: "Sort by AI%" / "Sort by Vibe Score"
- Each entry shows: grade | repo name | AI% | Vibe Score | roast snippet
- Period filters remain (All time, This month, This week)
- Pagination unchanged

### /report/:id — AI% in Spotlight
- AI% as primary metric (big percentage + bar)
- Vibe Score + Grade as secondary
- Score breakdown showing point sources (pills/chips)
- Roast, languages, share — unchanged

### /trends — Full Index + Community
- **Top section**: Stacked area chart of GitHub Index (large format)
- Filters: 7d, 30d, 90d, All
- Key stats: repos tracked, total commits, current AI%, trend direction
- **Bottom section**: Community stats (grows over time)

### /reports — Removed
- URL redirects to /leaderboard (or just removed from nav)

## Navigation
```
logo (→ /)  |  scan  |  leaderboard  |  trends  |  github
```

## Data Actions

### Purge test reports
Delete all 17 community reports from the `reports` table (test data from development).

### Backfill index snapshots
Run 2 additional scans to create snapshots for Feb 19 and Feb 20, giving us 3 data points for the initial trend chart.

## What Doesn't Change
- The scan page flow (input → loading → result)
- The report detail page structure (just reordered)
- The CLI behavior
- The API endpoints
- The VPS worker
- The cron job
- The Tokyo Night theme and design system
