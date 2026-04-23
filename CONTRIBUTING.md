# Contributing to Cortex

Thanks for your interest in contributing! This guide covers the basics.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/<your-user>/cortex.git`
3. Create a feature branch: `git checkout -b my-feature`
4. Build: `cargo build`
5. Test: `cargo test`

## Development Setup

**Requirements:**
- Rust stable (via [rustup](https://rustup.rs/))
- `pkg-config` and `libssl-dev` (only for `embeddings` feature)

**Optional:**
```bash
# For embedding/vector search support
cargo build --features embeddings
```

## Making Changes

### Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy` and fix any warnings
- Keep the build warning-free: `cargo check` should produce zero warnings

### Testing

- Add unit tests for new parsing logic
- Test against real source code snippets (see existing tests in `src/parser/`)
- Run the full suite: `cargo test --lib`

### Commit Messages

- Use present tense, imperative mood: "Add Python parser" not "Added Python parser"
- Keep the first line under 72 characters
- Reference issues when applicable: "Fix directory scanning (#12)"

### Pull Requests

- Open against the `main` branch
- Include a clear description of what changed and why
- Keep PRs focused — one feature or fix per PR
- Ensure CI passes (build, test, clippy, fmt)

## Adding a Language Parser

Cortex uses Tree-sitter for AST parsing. To add support for a new language:

1. Add the `tree-sitter-<lang>` dependency to `Cargo.toml`
2. Create `src/parser/<lang>_parser.rs`
3. Implement the `Parser` trait (see `rust_parser.rs` as a reference)
4. Add the language to `Language` enum in `src/models/symbol.rs`
5. Register the parser in `src/parser/mod.rs`
6. Add unit tests with real code snippets
7. Update the README

## Reporting Issues

- **Bugs:** Open an issue with reproduction steps, expected vs actual behavior, and your OS/Rust version
- **Feature requests:** Describe the use case, not just the solution
- **Questions:** Start a discussion

## Code of Conduct

See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md). Be respectful, be constructive.
