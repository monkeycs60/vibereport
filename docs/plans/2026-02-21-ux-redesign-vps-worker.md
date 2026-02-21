# Vibereport UX Redesign + VPS Scan Worker

## 1. Scoring System Redesign

Two separate metrics:

**AI %** (factual) — `ai_commits / total_commits * 100`. Raw number, no weighting.

**Vibe Score** (fun, composite, basis for grade S+ to F):

| Indicator | Points | Detection |
|---|---|---|
| AI ratio | 0-60 | `ai_ratio * 60` |
| No tests | +20 | 0 test files |
| Few tests (<3) | +10 | 1-2 test files |
| .env in git | +20/file (max 60) | .env, .env.local, .env.prod etc. |
| Hardcoded secrets | +20/each (max 60) | API keys, tokens in code |
| No linting | +10 | No .eslintrc*, .prettierrc*, biome.json, deno.json |
| No CI/CD | +10 | No .github/workflows/, .gitlab-ci.yml, Jenkinsfile |
| Boomer AI | +10 | AI% > 0 but no .claude/, .cursorrules, AGENTS.md, copilot-instructions.md |
| node_modules in git | +15 | node_modules/ tracked |
| Mega commit | +10 | >50% of code in a single commit |
| Deps bloat | 0-10 | deps/100, capped |
| No .gitignore | +10 | Missing or < 3 lines |
| No README | +10 | No README.md or README |
| TODO flood (>20) | +5 | >20 TODO/FIXME/HACK in code |
| Single branch | +5 | Only main/master, no other branches |

Max theoretical: ~285 pts. Grade from Vibe Score (same thresholds as before).

### Grade thresholds (unchanged)
- S+ > 100, S >= 90, A+ >= 80, A >= 70, B+ >= 60, B >= 50, C+ >= 40, C >= 30, D >= 20, F < 20

### Roast system
Contextual roasts based on detected patterns:
- Boomer AI detected → "Uses AI like a boomer uses email."
- node_modules in git → "Committing node_modules. Bold strategy."
- No tests + high AI → "Vibe coded to production. No safety net."
- etc.

---

## 2. Homepage Redesign

### Stats section: 2 counters
- Repos scanned (COUNT from reports)
- Commits analyzed (SUM total_commits from reports)

### Tug of War (replaces BattleChart)
Horizontal bar, full width:
- Left side: AI (red/pink) with emoji + count
- Right side: Humans (blue) with emoji + count
- Bar split proportional to global AI vs Human commits
- Dynamic phrase based on ratio:
  - <5% → "Humans are winning... for now."
  - 5-20% → "The machines are gaining ground."
  - 20-50% → "It's anyone's game."
  - >50% → "AI has taken over."
- Animated on scroll

### Keep existing
- TrendChart (AI adoption over time) stays below tug of war
- Leaderboard stays
- How it works stays

---

## 3. Scan Result Redesign

When user scans a repo, result shows:
- Repo name + Grade badge (from Vibe Score)
- AI% in big bar (factual, left=AI right=Human)
- Vibe Score number next to grade
- Roast
- Chaos badges below: visual colored pills for each detected pattern
  - "no tests", ".env in git", "boomer AI", "node_modules", etc.
- Detailed breakdown expandable

---

## 4. VPS Scan Worker

### Architecture
```
Frontend (Astro)
    │ POST /api/scan {repo, since}
    ▼
Cloudflare Worker (proxy)
    │ POST /scan {repo, since}
    ▼
VPS Worker (Axum HTTP on vps-139a77b3.vps.ovh.net)
    ├─ git clone --bare --shallow-since
    ├─ vibereport /tmp/{uuid} --json --since
    ├─ POST /api/reports (store results)
    ├─ rm -rf /tmp/{uuid}
    └─ return JSON result
```

Fallback: if VPS is down, Cloudflare Worker uses existing GitHub API logic.

### VPS Worker details
- Axum HTTP server, single endpoint POST /scan
- Auth: shared secret (Authorization: Bearer {token})
- Concurrency: tokio bounded channel (cap 20), semaphore (5 concurrent clones)
- Overflow: 429 Too Many Requests
- Clone timeout: 60s
- Cleanup: always rm -rf after scan

### Clone strategy
- `git clone --bare --shallow-since="2025-01-01"` for git-only analysis
- Option B for MVP: bare clone = git analysis only (commits, AI ratio) + file tree walk for structure detection (no file content)
- Full scoring possible by checking file existence in bare clone tree

### Fingerprint
- VPS shallow clone: use normalized remote_url as fingerprint (not root commit)
- Normalize: extract github.com URL, use as unique key
- Both CLI and VPS upsert on same entry

### Deployment
- Binary compiled on VPS or cross-compiled
- systemd service (vibereport-worker)
- nginx reverse proxy with HTTPS on scan.vibereport.dev
- Rate limit: 30 req/min per IP

---

## 5. DB Schema Changes

```sql
ALTER TABLE reports ADD COLUMN period_start TEXT;
ALTER TABLE reports ADD COLUMN period_end TEXT;
ALTER TABLE reports ADD COLUMN scan_source TEXT DEFAULT 'cli';
-- New scoring fields
ALTER TABLE reports ADD COLUMN vibe_score INTEGER DEFAULT 0;
ALTER TABLE reports ADD COLUMN chaos_badges TEXT DEFAULT '[]';
```

---

## 6. CLI Changes

### New flag: --since
```
#[arg(long, default_value = "all")]
since: String,
```
Parse into Option<DateTime<Utc>> cutoff. Filter commits in walk loop.

### New detectors (src/project/)
- `lint_detect.rs` — check for linting config files
- `ci_detect.rs` — check for CI/CD config files
- `ai_config_detect.rs` — check for AI tool config files
- `vibe_detect.rs` — node_modules in git, mega commit, .gitignore check, README check, TODO flood, single branch
- Update `security.rs` — already handles .env and secrets

### Updated calculator
- Remove codebase size factor
- AI ratio: 0-60 (was 0-70)
- No tests: +20 (was +15)
- .env: +20/file max 60 (was +5/file max 20)
- Secrets: +20/each max 60 (was +3/each max 15)
- Add all new indicators

---

## 7. Implementation Order

### Phase 1: CLI scoring + new detectors
1. New detector modules
2. Updated calculator with new weights
3. --since flag
4. Tests
5. cargo clippy + cargo test

### Phase 2: Frontend redesign
1. Tug of war component (replace BattleChart)
2. 2-stat header
3. Scan result page with AI% + Vibe Score + chaos badges
4. Report detail page with breakdown
5. Use frontend-design skill for aesthetics

### Phase 3: VPS Worker
1. New Rust binary (Axum server)
2. Clone + analyze + upload pipeline
3. Deploy to VPS
4. nginx + systemd + HTTPS

### Phase 4: API integration
1. Cloudflare Worker proxy to VPS
2. DB schema migration
3. Fallback logic
4. New fields in API responses
