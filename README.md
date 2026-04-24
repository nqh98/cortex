<h1 align="center">
<img src="https://img.shields.io/badge/Cortex-v0.2.0-blue" alt="Cortex" />
<br />
Cortex — Local Code Context Engine for AI Assistants
</h1>

<p align="center">
<strong>Give your AI coding assistant deep understanding of your codebase.</strong>
<br />
Index source code locally with Tree-sitter, query it via <a href="https://modelcontextprotocol.io/">MCP</a>.
<br />
Works with <strong>Claude Code</strong>, <strong>Cline</strong>, <strong>Cursor</strong>, <strong>Windsurf</strong>, and any MCP-compatible AI tool.
</p>

<p align="center">
<a href="https://github.com/nqh98/cortex"><img src="https://img.shields.io/github/stars/nqh98/cortex?style=social" alt="GitHub stars" /></a>
<img src="https://img.shields.io/github/license/nqh98/cortex" alt="License: MIT" />
<img src="https://img.shields.io/badge/Rust-1.75+-orange" alt="Rust" />
<img src="https://img.shields.io/badge/MCP-Protocol-green" alt="MCP" />
<img src="https://img.shields.io/badge/SQLite-FTS5-blue" alt="SQLite" />
</p>

<p align="center">
<a href="#features">Features</a> &bull;
<a href="#installation">Installation</a> &bull;
<a href="#quick-start">Quick Start</a> &bull;
<a href="#mcp-tools">13 MCP Tools</a> &bull;
<a href="#supported-languages">Languages</a> &bull;
<a href="#how-it-works">Architecture</a>
</p>

---

**Cortex** is a local-first code context engine that parses source files with [Tree-sitter](https://tree-sitter.github.io/), stores symbols and imports in [SQLite](https://www.sqlite.org/) with [FTS5](https://www.sqlite.org/fts5.html) full-text search, and exposes **13 query tools** over [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) via stdio JSON-RPC.

Everything runs **locally** — no cloud services, no API keys, no data leaves your machine.

## Why Cortex?

| | Cortex |
|---|---|
| **Setup** | One command: `./install.sh` |
| **Indexing** | Incremental via file hashing + auto-reindex |
| **Search** | FTS5 BM25 + symbol name tokenization |
| **Privacy** | 100% local, no telemetry |
| **Speed** | Tree-sitter parsing, SQLite storage |
| **AI Integration** | Native MCP protocol (Claude, Cline, Cursor, etc.) |

## Features

- **13 MCP tools** — symbol search, code retrieval, content grep, reference finding, import analysis, full-text search, and more
- **Multi-language** — Rust, Python, JavaScript, TypeScript (including TSX/JSX)
- **Auto-reindex** — detects stale indexes and re-indexes automatically before queries
- **11 symbol kinds** — functions, structs, classes, interfaces, type aliases, enums, traits, impls, constants, modules, methods
- **Import tracking** — stores import statements per file, supports outgoing/incoming dependency queries
- **Full-text search** — FTS5 with BM25 ranking across symbol names, signatures, and documentation
- **Reference finding** — finds all usages of a symbol across the project with classification (import, call, type usage)
- **Content search** — regex grep across indexed files with context lines
- **Document symbols** — lists all symbols in a file with parent-child hierarchy
- **Multi-project** — index multiple repos in a single database, query them independently

## Installation

```bash
git clone https://github.com/nqh98/cortex.git
cd cortex
./install.sh
```

This builds the binary, installs to `~/.local/bin`, and registers with Claude's MCP config.

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- `jq` (for MCP config setup)

## Quick Start

### 1. Connect to Claude via MCP

`./install.sh` configures this automatically. Manual setup — add to your MCP config (`~/.claude/claude_desktop_config.json` or equivalent):

```json
{
  "mcpServers": {
    "cortex": {
      "command": "/home/username/.local/bin/cortex",
      "args": ["serve"]
    }
  }
}
```

No need to manually index — Cortex auto-detects stale indexes and re-indexes before queries.

### 2. CLI usage (optional)

```bash
cortex index ./my-project        # Index a project
cortex search "handler"          # Search symbols
cortex search "Parser" --kind struct
cortex context get_parser        # Get source code
cortex watch ./my-project        # Auto-reindex on file changes
cortex list                      # List indexed projects + DB size
cortex clean <name>              # Remove index by project name
cortex clean all                 # Remove all indexes
cortex reset                     # Clear all indexes
```

## MCP Tools

| Tool | Description |
|------|-------------|
| `search_symbols` | Find symbols by name with kind filter and pagination |
| `get_code_context` | Retrieve full source code for a symbol by name |
| `list_document_symbols` | List all symbols in a file with parent-child hierarchy |
| `search_content` | Grep file contents by regex or plain text with context lines |
| `find_references` | Find all references to a symbol across the project |
| `search_by_semantic` | Full-text search across symbol names, signatures, docs |
| `get_imports` | Analyze import dependencies (outgoing/incoming) for a file |
| `list_directory_structure` | Browse project directory tree |
| `list_files` | List files with extension filter |
| `index_project` | Index or refresh a project |
| `get_index_status` | Check if a project is indexed |
| `list_symbol_kinds` | Get available symbol type filters |
| `get_symbol_stats` | Get index statistics by kind and language |

### Typical Workflow

1. `get_index_status(path)` — triggers auto-index if needed
2. `search_symbols("RfqBuyerService")` — find the symbol
3. `get_code_context(symbol_name="RfqBuyerService")` — read the implementation
4. `find_references(symbol_name="RfqBuyerService")` — find all usages
5. `get_imports(file_path="src/app/rfq/services/rfq-buyer.service.ts")` — trace dependencies
6. `search_content(pattern="TODO")` — grep for patterns

### Auto-Reindex

Cortex checks for stale indexes before each query. If source files have changed since the last index, it re-indexes automatically. This has a 30-second cooldown per project to avoid unnecessary work.

### Error Format

All errors return structured JSON:

```json
{
  "error": {
    "code": "symbol_not_found",
    "message": "Symbol 'my_func' not found"
  }
}
```

## Supported Languages

| Rust | Python | JavaScript / TypeScript |
|------|--------|------------------------|
| `fn` (function) | `def` (function) | `function` declaration |
| `struct` | `class` | `class` |
| `impl` + methods | class methods | methods, constructor |
| `trait` | | arrow function (`const fn = () =>`) |
| `enum` | | `interface` |
| `const` | | `type` alias |
| `mod` | | `export` functions/classes |

## How It Works

```
Source Files ──▶ Tree-sitter Parser ──▶ SQLite Index ──▶ MCP Server ──▶ AI Assistant
                 (per language)         (symbols +       (stdio        (Claude, Cline,
                                        imports + FTS)   JSON-RPC)     Cursor, etc.)
```

1. **Scanner** walks the directory respecting `.gitignore`
2. **Parser** generates ASTs with Tree-sitter, extracts symbols and import statements
3. **Indexer** stores in SQLite with file hashes for incremental updates, FTS5 for text search
4. **MCP Server** serves 13 tools over stdio, auto-reindexes when stale

## Architecture

```
src/
├── main.rs           CLI entry point (index, search, list, clean, serve, watch)
├── config.rs         TOML configuration
├── error.rs          Error types
├── models/           Symbol, Import, Language, SymbolKind
├── scanner/          Directory walking with .gitignore
├── parser/           Tree-sitter parsers (Rust, Python, JS, TS)
├── indexer/          SQLite storage, migrations, indexing pipeline
├── query/            Search, context, references, content, semantic, imports
├── watcher/          File change detection via notify
└── mcp_server/       MCP tool server with 13 tools
```

## Configuration

Data is stored in `~/.cortex/`:

```
~/.cortex/
├── config.toml     # Optional config overrides
└── db.sqlite       # Symbol index database
```

Default config (auto-generated):

```toml
[database]
path = "/home/username/.cortex/db.sqlite"

[indexing]
max_file_size_kb = 1024
supported_extensions = ["rs", "py", "js", "ts", "tsx", "jsx"]

[embeddings]
enabled = false
model = "AllMiniLML6V2"
batch_size = 32

[watcher]
debounce_ms = 500
```

## Development

```bash
cargo build              # Build
cargo test               # Run tests
cargo run -- index .     # Index this project
cargo run -- serve       # Start MCP server
```

## License

MIT
