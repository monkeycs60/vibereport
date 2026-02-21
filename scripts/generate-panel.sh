#!/bin/bash
# Generate the quarterly index panel (1000 repos)
# Usage: ./scripts/generate-panel.sh [QUARTER]
# Requires: GITHUB_TOKEN env var, wrangler CLI, jq
#
# Fetches top 600 repos by stars + top 600 by recent activity from GitHub Search API,
# deduplicates by repo slug, cuts to exactly 1000 repos, and upserts into D1 index_panel.

set -euo pipefail

QUARTER="${1:-$(date +%Y)-Q$(( ($(date +%-m) - 1) / 3 + 1 ))}"
DB_NAME="vibereport-db"
TOKEN="${GITHUB_TOKEN:?Set GITHUB_TOKEN env var}"
API_DIR="/home/clement/Desktop/vibereport/web/api"

echo "=== Generating panel for $QUARTER ==="
echo ""

# We use temp files for deduplication since associative arrays don't survive subshells well
TMPFILE=$(mktemp /tmp/panel-repos-XXXXXX)
trap 'rm -f "$TMPFILE"' EXIT

# --- Fetch top 600 by stars ---
echo "Fetching top repos by stars (6 pages)..."
for page in $(seq 1 6); do
  echo "  Stars page $page/6..."
  response=$(curl -sf -H "Authorization: Bearer $TOKEN" \
    -H "Accept: application/vnd.github.v3+json" \
    "https://api.github.com/search/repositories?q=stars:%3E1000&sort=stars&order=desc&per_page=100&page=$page")

  # Parse each repo: full_name and stargazers_count
  echo "$response" | jq -r '.items[] | "\(.full_name)|\(.stargazers_count)"' | while IFS='|' read -r slug stars; do
    echo "${slug}|stars|${stars}" >> "$TMPFILE"
  done

  count=$(wc -l < "$TMPFILE")
  echo "    Cumulative raw entries: $count"
  sleep 2  # Rate limit: 30 req/min for search
done

# --- Fetch top 600 by recent activity ---
echo ""
echo "Fetching top repos by activity (6 pages)..."
for page in $(seq 1 6); do
  echo "  Activity page $page/6..."
  response=$(curl -sf -H "Authorization: Bearer $TOKEN" \
    -H "Accept: application/vnd.github.v3+json" \
    "https://api.github.com/search/repositories?q=pushed:%3E2026-01-01+stars:%3E100&sort=updated&order=desc&per_page=100&page=$page")

  echo "$response" | jq -r '.items[] | "\(.full_name)|\(.stargazers_count)"' | while IFS='|' read -r slug stars; do
    echo "${slug}|activity|${stars}" >> "$TMPFILE"
  done

  count=$(wc -l < "$TMPFILE")
  echo "    Cumulative raw entries: $count"
  sleep 2
done

# --- Deduplicate by repo slug (keep first occurrence) ---
echo ""
echo "Deduplicating..."
DEDUPED=$(mktemp /tmp/panel-deduped-XXXXXX)
trap 'rm -f "$TMPFILE" "$DEDUPED"' EXIT
awk -F'|' '!seen[$1]++' "$TMPFILE" > "$DEDUPED"

TOTAL=$(wc -l < "$DEDUPED")
echo "Found $TOTAL unique repos after dedup"

# --- Cut to exactly 1000 ---
if [[ $TOTAL -gt 1000 ]]; then
  echo "Cutting to 1000 repos..."
  head -n 1000 "$DEDUPED" > "${DEDUPED}.cut"
  mv "${DEDUPED}.cut" "$DEDUPED"
  TOTAL=1000
elif [[ $TOTAL -lt 1000 ]]; then
  echo "WARNING: Only $TOTAL unique repos found (target: 1000)"
fi

# --- Build SQL file and batch insert into D1 ---
echo ""
echo "Building SQL batch for $TOTAL repos..."
SQLFILE=$(mktemp /tmp/panel-sql-XXXXXX.sql)
trap 'rm -f "$TMPFILE" "$DEDUPED" "$SQLFILE"' EXIT

COUNT=0
while IFS='|' read -r slug source stars; do
  safe_slug="${slug//\'/\'\'}"
  safe_source="${source//\'/\'\'}"
  echo "INSERT INTO index_panel (repo_slug, quarter, panel_source, stars) VALUES ('$safe_slug', '$QUARTER', '$safe_source', $stars) ON CONFLICT(repo_slug, quarter) DO UPDATE SET panel_source = excluded.panel_source, stars = excluded.stars;" >> "$SQLFILE"
  COUNT=$((COUNT + 1))
done < "$DEDUPED"

echo "Executing batch SQL ($COUNT statements)..."
cd "$API_DIR" && npx wrangler d1 execute "$DB_NAME" --remote --file "$SQLFILE"

echo ""
echo "Done! Inserted $COUNT repos for $QUARTER"

# --- Verify final count ---
echo ""
echo "Verifying..."
cd "$API_DIR" && npx wrangler d1 execute "$DB_NAME" --remote \
  --command "SELECT quarter, panel_source, COUNT(*) as count FROM index_panel WHERE quarter = '$QUARTER' GROUP BY panel_source;"

echo ""
cd "$API_DIR" && npx wrangler d1 execute "$DB_NAME" --remote \
  --command "SELECT COUNT(*) as total FROM index_panel WHERE quarter = '$QUARTER';"

echo ""
echo "=== Panel generation complete for $QUARTER ==="
