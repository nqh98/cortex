# Cortex

A local-first code context engine that indexes your source code, extracts semantic structures (functions, classes, structs), and exposes them to LLMs via the [Model Context Protocol (MCP)](https://modelcontextprotocol.io/).

**Privacy-first.** All indexing and queries happen on your machine. No code is sent to any external service.

## Features

- **Multi-language parsing** — Extracts symbols from Rust, Python, and JavaScript/TypeScript using Tree-sitter
- **Fast search** — SQLite-backed fuzzy search across all indexed symbols with sub-10ms queries
- **Code context** — Retrieve the full source code of any symbol with line numbers
- **MCP server** — Exposes tools directly to Claude and other MCP-compatible LLMs via stdio
- **File watching** — Incremental re-indexing on save (only changed files are re-parsed)
- **Directory tree** — Project structure overview for LLM context

## Installation

### From source

```bash
git clone https://github.com/<your-user>/cortex.git
cd cortex
cargo build --release
```

The binary will be at `target/release/cortex`.

### Prerequisites

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

Claude will then have access to three tools:
- **`search_symbols`** — Find functions, classes, structs by name
- **`get_code_context`** — Read the source code of any indexed symbol
- **`list_directory_structure`** — Browse the project file tree

### 5. Watch for changes

```bash
cortex watch ./my-project
```

Automatically re-indexes files when you save them.

## Configuration

Create a `config.toml` in your project root (or wherever you run `cortex`):

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
┌─────────────┐    ┌────────────┐    ┌────────────┐    ┌─────────┐
│  Directory   │───▶│  Scanner   │───▶│   Parser   │───▶│  SQLite │
│  Walker      │    │ (.gitignore)│   │ (tree-sitter)│   │  Index  │
└─────────────┘    └────────────┘    └────────────┘    └─────────┘
                                                            │
                                       ┌───────────────────┘
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
