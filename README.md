# Cortex

[![Version](https://img.shields.io/badge/version-0.2.0-blue)](https://github.com/your-org/cortex)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

A local-first code context engine that indexes your source code, extracts semantic structures (functions, classes, structs), and exposes them to LLMs via the [Model Context Protocol (MCP)](https://modelcontextprotocol.io/).

**Privacy-first.** All indexing and queries happen on your machine. No code is sent to any external service.

**AI-Friendly.** All MCP tools return structured JSON for reliable programmatic access by AI agents.

## Features

- **Multi-language parsing** — Extracts symbols from Rust, Python, and JavaScript/TypeScript using Tree-sitter
- **Fast search** — SQLite-backed fuzzy search across all indexed symbols with sub-10ms queries
- **Code context** — Retrieve the full source code of any symbol with line numbers
- **MCP server** — Exposes tools directly to Claude and other MCP-compatible LLMs via stdio
- **File watching** — Incremental re-indexing on save (only changed files are re-parsed)
- **Directory tree** — Project structure overview for LLM context

## Installation

### Docker (recommended — no host dependencies)

#### Option 1: Easy one-command install with wrapper script

This is the simplest method — it installs a wrapper script that abstracts all Docker complexity:

```bash
./install.sh --project-path /path/to/your/project --install-wrapper
```

This will:
1. Build the Docker image (includes all features — embeddings, vector search, etc.)
2. Create a persistent Docker volume for the index
3. Install the `cortex-docker` wrapper to `~/.local/bin`
4. Auto-detect your Claude config and register Cortex as an MCP server

After installation, you can run Cortex commands easily:

```bash
cortex-docker index ./my-project
cortex-docker search "handler"
cortex-docker context src/main.rs main
cortex-docker watch .
cortex-docker serve  # For MCP
```

**Multi-repository support:**
- Each repository is stored with strict data isolation
- Use `cortex-docker list` to see all indexed repositories
- Use `--repo <name>` to work with a specific repository
- The most recently indexed repository becomes the default

#### Option 2: Basic install

```bash
./install.sh --project-path /path/to/your/project
```

This will:
1. Build the Docker image
2. Create a persistent Docker volume for the index
3. Auto-detect your Claude config and register Cortex as an MCP server

#### Option 3: Multi-project setup

For multiple projects with separate indexes:

```bash
./install.sh --project-path /path/to/project1 --project-name project1 --install-wrapper
./install.sh --project-path /path/to/project2 --project-name project2 --install-wrapper
```

Each project gets its own Docker volume and MCP server entry.

#### Manual Docker commands

If you prefer to run commands manually:

```bash
# Index a project
docker run --rm -v /path/to/your/project:/project -v cortex-data:/home/cortex/.cortex cortex index /project

# Search symbols
docker run --rm -v /path/to/your/project:/project -v cortex-data:/home/cortex/.cortex cortex search "handler"

# Persist the index between runs
docker volume create cortex-data
```

### From source

```bash
git clone https://github.com/<your-user>/cortex.git
cd cortex
cargo build --release
```

The binary will be at `target/release/cortex`.

#### Prerequisites (source build only)

- [Rust](https://rustup.rs/) (latest stable)
- For embedding/vector search features: `pkg-config` and `libssl-dev` (`openssl-devel` on Fedora)

## Quick Start

### 1. Index a project

```bash
cortex index ./my-project
```

```
Indexed 30 files (709 symbols, 0 unchanged, 0 failed)
```

### 2. Search for symbols

```bash
cortex search "handler"
cortex search "Parser" --kind struct
```

```
struct RustParser (src/parser/rust_parser.rs:5-5)
struct PythonParser (src/parser/python_parser.rs:5-5)
struct JsParser (src/parser/js_parser.rs:5-5)
```

**Using the Docker wrapper script:**

```bash
cortex-docker search "handler"
cortex-docker search "Parser" --kind struct
```

### 3. Get code context

```bash
cortex context src/parser/mod.rs get_parser
```

```
--- get_parser (function) ---
File: src/parser/mod.rs lines 13-19
Signature: fn get_parser(language: Language) -> Box<dyn Parser>

  13 | pub fn get_parser(language: Language) -> Box<dyn Parser> {
  14 |     match language {
  15 |         Language::Rust => Box::new(rust_parser::RustParser),
  16 |         Language::Python => Box::new(python_parser::PythonParser),
  17 |         Language::JavaScript => Box::new(js_parser::JsParser),
  18 |     }
  19 | }
```

### 4. Connect to Claude via MCP

Add Cortex to your Claude Desktop configuration (`~/Library/Application Support/Claude/claude_desktop_config.json` on macOS, or equivalent on Linux):

**Option A — Using the wrapper script (recommended):**

```json
{
  "mcpServers": {
    "cortex": {
      "command": "/home/username/.local/bin/cortex-docker",
      "args": ["serve"],
      "env": {
        "CORTEX_PROJECT": "/path/to/your/project",
        "CORTEX_VOLUME": "cortex-data"
      }
    }
  }
}
```

**Option B — Direct Docker command:**

```json
{
  "mcpServers": {
    "cortex": {
      "command": "docker",
      "args": ["run", "--rm", "-i", "-v", "/path/to/your/project:/project", "-v", "cortex-data:/home/cortex/.cortex", "cortex", "serve"]
    }
  }
}
```

**Option B — Local binary:**

```json
{
  "mcpServers": {
    "cortex": {
      "command": "/path/to/cortex",
      "args": ["serve"]
    }
  }
}
```

Claude will then have access to eight tools:
- **`search_symbols`** — Find functions, classes, structs by name (returns JSON)
- **`get_code_context`** — Read the source code of any indexed symbol (returns JSON)
- **`list_directory_structure`** — Browse the project file tree (returns JSON)
- **`index_project`** — Index or refresh a project (returns JSON)
- **`get_index_status`** — Check if a project is indexed (returns JSON)
- **`list_files`** — List files with filtering (returns JSON)
- **`list_symbol_kinds`** — Get available symbol types (returns JSON)
- **`get_symbol_stats`** — Get overall statistics (returns JSON)

### 5. Watch for changes

```bash
cortex watch ./my-project
# Or with Docker:
cortex-docker watch ./my-project
```

Automatically re-indexes files when you save them.

## Docker Usage Options

### 1. Wrapper Script (Recommended)

The wrapper script provides a simple interface:

```bash
./cortex-docker.sh <command> [args...]

# Or if installed to ~/.local/bin
cortex-docker <command> [args...]

# Commands:
cortex-docker index <path>        # Index a project
cortex-docker search <query>      # Search for symbols
cortex-docker context <file> <symbol>  # Get source code
cortex-docker serve               # Start MCP server
cortex-docker watch <path>        # Watch for changes
cortex-docker shell               # Open shell in container
cortex-docker clean               # Remove data volume
cortex-docker help                # Show help
```

### 2. Makefile Targets

Simple commands using make:

```bash
make docker-build        # Build Docker image
make docker-index        # Index current directory
make docker-search Q='query'  # Search
make docker-serve        # Start MCP server
make docker-watch        # Watch directory
make docker-shell        # Open shell in container
make docker-clean        # Remove data volume
make help                # Show all targets
```

### 3. Docker Compose

For users who prefer Docker Compose:

```bash
# Build and run
docker compose build
docker compose run --rm cortex index /project
docker compose run --rm cortex search "handler"

# Multi-project setup
docker compose -f docker-compose.multi-project.yml run --rm cortex-project1 index /project
```

### 4. Direct Docker Commands

For full control:

```bash
docker build -t cortex .
docker run --rm -v /path/to/project:/project -v cortex-data:/home/cortex/.cortex cortex index /project
```

## Multi-Repository Management

Cortex supports indexing multiple repositories with strict data isolation:

### Listing Repositories

```bash
cortex-docker list
# or
cortex-docker repos
```

Output shows all indexed repositories with the current one marked with `*`:

```
Indexed repositories:

    my-api
      Path: /path/to/my-api
      Status: Indexed
  * frontend (current)
      Path: /path/to/frontend
      Status: Indexed
```

### Conflict Detection

If you try to index a repository with a name that already exists but points to a different path, Cortex will warn you:

```bash
$ cortex-docker index /different/path --name my-api
Warning: Repository 'my-api' already exists at: /original/path
Warning: You're trying to index: /different/path

Options:
  1. Overwrite the existing repository (will lose data)
  2. Use a different name with --name <name>
  3. Cancel
Choose an option [1/2/3]:
```

To skip the confirmation and force overwrite, use the `--force` flag:

```bash
cortex-docker index /different/path --name my-api --force
```

### Working with Specific Repositories

Use the `--repo` (or `-r`) flag to specify which repository to use:

```bash
# Index with a custom name
cortex-docker index /path/to/project --name my-api

# Search in a specific repository
cortex-docker search "handler" --repo my-api

# Get context from a specific repository
cortex-docker context get_handler --repo my-api

# Clean a specific repository
cortex-docker clean --repo my-api
```

### Cleaning Up

```bash
# Clean a specific repository
cortex-docker clean --repo my-api

# Clean all repositories
cortex-docker clean --all

# Clean without specifying (prompts for confirmation)
cortex-docker clean
```

```toml
[database]
path = ".cortex/db.sqlite"

[indexing]
max_file_size_kb = 1024
supported_extensions = ["rs", "py", "js", "ts"]

[embeddings]
enabled = false
model = "AllMiniLML6V2"
batch_size = 32

[watcher]
debounce_ms = 500
```

See [`config.example.toml`](config.example.toml) for all options with defaults.

## MCP API Reference

All MCP tools return structured JSON responses for programmatic access by AI agents.

### search_symbols

Search for code symbols by name pattern matching.

**Request:**
```json
{
  "query": "parser",
  "kind": "struct",
  "limit": 50,
  "offset": 0
}
```

**Response:**
```json
{
  "symbols": [
    {
      "id": 123,
      "name": "RustParser",
      "kind": "struct",
      "file_path": "src/parser/rust_parser.rs",
      "project_root": "/path/to/project",
      "start_line": 5,
      "end_line": 5,
      "signature": "struct RustParser"
    }
  ],
  "total_count": 1,
  "has_more": false
}
```

### get_code_context

Retrieve full source code for a symbol.

**Request:**
```json
{
  "symbol_name": "get_parser",
  "file_path": "src/parser/mod.rs",
  "context_lines": 2
}
```

**Response:**
```json
{
  "symbol_name": "get_parser",
  "kind": "function",
  "file_path": "src/parser/mod.rs",
  "start_line": 14,
  "end_line": 21,
  "signature": "fn get_parser(language: Language) -> Box<dyn Parser>",
  "code": "pub fn get_parser(language: Language) -> Box<dyn Parser> {\n    match language {\n        Language::Rust => Box::new(rust_parser::RustParser),\n        ...\n    }\n}",
  "preview": "  14 | pub fn get_parser(language: Language) -> Box<dyn Parser> {\n  15 |     match language {\n  16 | ...",
  "context_before": ["  12 | /// Get parser for a language", "  13 | "],
  "context_after": ["  22 | }"]
}
```

### list_directory_structure

List directory structure with metadata.

**Request:**
```json
{
  "path": "/path/to/project",
  "max_depth": 3,
  "extension": "rs"
}
```

**Response:**
```json
{
  "root": "project",
  "entries": [
    {
      "name": "main.rs",
      "path": "src/main.rs",
      "entry_type": "file",
      "extension": "rs",
      "language": "rust",
      "size": 2048,
      "depth": 1
    }
  ],
  "file_count": 24,
  "directory_count": 8
}
```

### Error Format

All errors follow a consistent JSON format:

```json
{
  "error": {
    "code": "symbol_not_found",
    "message": "Symbol 'my_func' not found in src/main.rs",
    "details": null
  }
}
```

Error codes:
- `invalid_parameters` - Invalid or missing required parameters
- `database_error` - Database connection or query failed
- `symbol_not_found` - No matching symbol found
- `file_not_found` - Requested file doesn't exist
- `invalid_path` - Invalid directory path
- `indexing_failed` - Indexing operation failed
- `ambiguous_symbol` - Multiple symbols match the query
- `serialization_error` - Failed to serialize response

## CLI Reference

```
cortex index <PATH>       Index a project directory
cortex search <QUERY>     Search for symbols (use --kind to filter)
cortex context <FILE> <SYMBOL>  Get source code for a symbol
cortex serve              Start the MCP server (stdio transport)
cortex watch <PATH>       Watch a directory and re-index on changes
```

## How It Works

```
┌─────────────┐    ┌─────────────┐    ┌──────────────┐    ┌─────────┐
│  Directory  │───▶│  Scanner    │───▶│   Parser     │───▶│  SQLite │
│  Walker     │    │ (.gitignore)│    │ (tree-sitter)│    │  Index  │
└─────────────┘    └─────────────┘    └──────────────┘    └─────────┘
                                                            │
                                       ┌────────────────────┘
                                       ▼
                                  ┌─────────┐
                                  │   MCP   │◀──── Claude / LLM
                                  │  Server │      via stdio
                                  └─────────┘
```

1. **Scanner** walks the directory tree respecting `.gitignore` rules
2. **Parser** generates an AST for each file using Tree-sitter and extracts symbol metadata (name, kind, line range, signature)
3. **Indexer** stores symbols in SQLite with file content hashes for incremental updates
4. **MCP Server** exposes query tools to LLMs over stdio JSON-RPC

## Supported Symbol Types

| Rust | Python | JavaScript |
|------|--------|------------|
| `fn` (functions) | `def` (functions) | `function` declarations |
| `struct` | `class` | `class` |
| `impl` (with methods) | Methods inside classes | Methods, constructor |
| `trait` | Module docstrings | Arrow functions (`const fn = () =>`) |
| `enum` | | `export` functions/classes |
| `const` | | |
| `mod` | | |

## Development

```bash
# Build
cargo build

# Run tests
cargo test

# Check formatting
cargo fmt --check

# Lint
cargo clippy

# Run locally
cargo run -- index .
cargo run -- search "Parser"
cargo run -- serve
```

### Feature flags

| Flag | Description | Extra dependencies |
|------|-------------|--------------------|
| `embeddings` | Local embedding generation + vector search via LanceDB | `pkg-config`, `libssl-dev` |

```bash
cargo build --features embeddings
```

## Architecture

```
src/
├── main.rs           CLI entry point
├── config.rs         TOML configuration
├── error.rs          Error types
├── models/           Data types (Symbol, FileRecord, Language)
├── scanner/          Directory walking with .gitignore
├── parser/           Tree-sitter parsers per language
├── indexer/          SQLite storage and indexing pipeline
├── query/            Symbol search and code context retrieval
├── watcher/          File change detection via notify
├── embeddings/       Local embedding generation (optional)
└── mcp_server/       MCP tool server
```

## License

MIT — see [LICENSE](LICENSE).
