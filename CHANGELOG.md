# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-04-23

### Added

- Directory scanner with `.gitignore` support via the `ignore` crate
- Tree-sitter parsers for Rust, Python, and JavaScript/TypeScript
- Symbol extraction: functions, structs, classes, impls, traits, enums, constants, modules, methods
- SQLite-backed storage with hash-based incremental indexing
- CLI: `cortex index`, `cortex search`, `cortex context`, `cortex serve`, `cortex watch`
- Symbol search with name and kind filtering
- Code context retrieval with line-numbered output
- MCP server exposing `search_symbols`, `get_code_context`, `list_directory_structure` tools
- File watcher with debounced incremental re-indexing
- TOML configuration with sensible defaults
- Structured logging via `tracing`
- 17 unit tests covering all three language parsers

[0.1.0]: https://github.com/<your-user>/cortex/releases/tag/v0.1.0
