# Contributing to vibereport

Thanks for your interest in contributing!

## Getting Started

```bash
git clone https://github.com/monkeycs60/vibereport.git
cd vibereport
cargo build
cargo test
```

## Development

- **Run**: `cargo run -- /path/to/repo`
- **Test**: `cargo test`
- **Lint**: `cargo clippy -- -D warnings`
- **Format**: `cargo fmt`

CI runs all three checks — make sure they pass before opening a PR.

## What to Contribute

- Bug fixes
- New AI tool detection signatures (see `src/git/ai_detect.rs`)
- Improved scoring heuristics
- Frontend improvements (see `web/frontend/`)
- Documentation

## Pull Requests

1. Fork the repo and create a branch from `master`
2. Make your changes
3. Run `cargo test && cargo clippy -- -D warnings && cargo fmt`
4. Open a PR with a clear description of what and why

## Code Style

- Use `thiserror` for error types
- Git operations go through `gix`, not shell commands
- Keep modules focused: `git/`, `project/`, `score/`, `render/`

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
