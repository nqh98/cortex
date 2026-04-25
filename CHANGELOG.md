# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-04-25

### Added

- Java language support (classes, interfaces, enums, methods, constructors, fields, Javadoc)
- Report export/synthesis tools (`export_report`, `synthesize_reports`) for task tracking
- `Interface` and `TypeAlias` symbol kinds
- `list`, `clean` CLI commands for managing indexed projects
- Auto-reindex on stale indexes (30s cooldown) before MCP queries
- Prebuilt binary install via `./install.sh` (no Rust toolchain required)
- Local install mode: `./install.sh local /path/to/project`
- Per-project indexes stored in `<project>/.cortex/index.sqlite`
- Project registry for tracking indexed repos
- Multi-pool MCP server with per-project database connections

### Changed

- Switched from shared global database to per-project `.cortex/index.sqlite` files
- CLI `search` and `context` commands now require `-p/--path` flag
- CLI `reset` command now requires a project path (no longer accepts omitting path)
- `install.sh` supports `--version`, `--url`, and `--build` flags
- Improved indexing accuracy for Rust, Python, JS/TS parsers
- Structured JSON output for all MCP tool responses

### Fixed

- Fixed clippy warnings across all parsers and scanner
- Fixed CI: added protobuf-compiler dependency
- Added retry logic for ort-sys CDN 504 failures

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

[0.1.0]: https://github.com/nqh98/cortex/releases/tag/v0.1.0

[0.2.0]: https://github.com/nqh98/cortex/releases/tag/v0.2.0
