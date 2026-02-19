# Vibereport

Rust CLI tool. "The Spotify Wrapped for your code."

## Conventions
- Use `thiserror` pattern for errors (enum VibereportError)
- All git operations go through `gix` crate, never shell out to `git`
- Module structure: git/, project/, score/, render/
- Tests: unit tests in same file (#[cfg(test)] mod tests), integration tests in tests/
- Run tests: `cargo test`
- Run lints: `cargo clippy -- -D warnings`
- Format: `cargo fmt`

## Architecture
- src/git/ — git log parsing, AI commit detection
- src/project/ — dependency counting, test detection, language stats
- src/score/ — composite score calculation, roast tagline selection
- src/render/ — terminal output (ratatui), SVG export, JSON export
- src/share/ — upload to vibereport.dev API (behind "share" feature flag)
- src/scanner/ — multi-repo discovery (--scan-all) + remote GitHub clone

## Scan Modes
1. Single repo (default): `vibereport` or `vibereport /path/to/repo`
2. Multi-repo: `vibereport --scan-all ~/projects` — finds all git repos recursively
3. Remote GitHub: `vibereport github:user/repo` — shallow clone + analyze
