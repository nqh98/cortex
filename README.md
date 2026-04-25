<h1 align="center">
<img src="https://img.shields.io/badge/Cortex-v0.2.1-blue" alt="Cortex" />
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
- **Multi-language** — Rust, Python, JavaScript, TypeScript, Java
- **Auto-reindex** — detects stale indexes and re-indexes automatically before queries
- **11 symbol kinds** — functions, structs, classes, interfaces, type aliases, enums, traits, impls, constants, modules, methods
- **Import tracking** — stores import statements per file, supports outgoing/incoming dependency queries
- **Full-text search** — FTS5 with BM25 ranking across symbol names, signatures, and documentation
- **Reference finding** — finds all usages of a symbol across the project with classification (import, call, type usage)
- **Content search** — regex grep across indexed files with context lines
- **Document symbols** — lists all symbols in a file with parent-child hierarchy
- **Per-project indexes** — each project stores its own index in `.cortex/index.sqlite` alongside the code

## Installation

### Option A: Download a release binary (recommended, no Rust needed)

1. Download the latest binary for your platform from [Releases](https://github.com/nqh98/cortex/releases):

```bash
# Linux (x86_64)
curl -fsSL https://github.com/nqh98/cortex/releases/latest/download/cortex-linux-x86_64.tar.gz | tar xz
sudo mv cortex /usr/local/bin/

# macOS (Apple Silicon)
curl -fsSL https://github.com/nqh98/cortex/releases/latest/download/cortex-macos-aarch64.tar.gz | tar xz
sudo mv cortex /usr/local/bin/

# macOS (Intel)
curl -fsSL https://github.com/nqh98/cortex/releases/latest/download/cortex-macos-x86_64.tar.gz | tar xz
sudo mv cortex /usr/local/bin/
```

2. Verify it works:

```bash
cortex --help
```

3. Connect to your AI assistant. Add to your MCP config (e.g. `~/.claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "cortex": {
      "command": "cortex",
      "args": ["serve"]
    }
  }
}
```

4. Restart your AI tool. Cortex auto-indexes your project on first query.

### Option B: Install script (auto-downloads or builds from source)

Clone and run the installer:

```bash
git clone https://github.com/nqh98/cortex.git
cd cortex

# Global install — binary goes to ~/.local/bin/cortex
./install.sh

# Local install — everything stays inside your project
./install.sh local /path/to/your/project
```

The installer auto-detects the best source: prebuilt binary first, falls back to building from source if download fails.

| | Global | Local |
|---|---|---|
| **Command** | `./install.sh` | `./install.sh local /path/to/project` |
| **Binary** | `~/.local/bin/cortex` | `<project>/.cortex/bin/cortex` |
| **Index** | `<project>/.cortex/index.sqlite` | `<project>/.cortex/index.sqlite` |
| **MCP config** | `~/.claude/settings.json` | `<project>/.claude/settings.local.json` |
| **CLAUDE.md** | `~/.claude/CLAUDE.md` | `<project>/CLAUDE.md` |
| **Scope** | All projects | Single project |
| **Uninstall** | Remove binary + `~/.cortex` | `rm -rf .cortex CLAUDE.md` |

Install script options:

```bash
./install.sh --version v0.2.1                  # Specific version
./install.sh --url https://example.com/cortex   # Custom binary URL
./install.sh --build                            # Force source build
```

### Prerequisites

- **Download only**: `curl` (pre-installed on most systems)
- **Install script**: `curl` or `wget`, plus `jq`
- **Source build**: [Rust](https://rustup.rs/) (latest stable), plus `jq`

## Quick Start

### 1. Connect to your AI assistant

After installing, Cortex runs as an MCP server. If you used `./install.sh`, this is already configured. If you downloaded the binary manually, add to your MCP config:

```json
{
  "mcpServers": {
    "cortex": {
      "command": "cortex",
      "args": ["serve"]
    }
  }
}
```

**Config file locations:**
- **Claude Code CLI**: `~/.claude/settings.json`
- **Claude Desktop**: `~/.config/claude/claude_desktop_config.json` (Linux) or `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS)
- **Cursor / Windsurf**: Project's `.cursor/mcp.json` or `.windsurf/mcp.json`

No need to manually index — Cortex auto-detects stale indexes and re-indexes before queries.

### 2. CLI usage (optional)

```bash
cortex index ./my-project                  # Index a project
cortex search "handler" -p ./my-project    # Search symbols
cortex search "Parser" --kind struct -p ./my-project
cortex context get_parser -p ./my-project  # Get symbol source
cortex watch ./my-project                  # Auto-reindex on file changes
cortex list                                # List indexed projects
cortex reset /path/to/project              # Clear a project's index
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
2. `search_symbols("RfqBuyerService", project_root)` — find the symbol
3. `get_code_context(symbol_name="RfqBuyerService", project_root)` — read the implementation
4. `find_references(symbol_name="RfqBuyerService", path)` — find all usages
5. `get_imports(file_path="src/app/rfq/services/rfq-buyer.service.ts", project_root)` — trace dependencies
6. `search_content(pattern="TODO", path)` — grep for patterns

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

| Rust | Python | JavaScript / TypeScript | Java |
|------|--------|------------------------|------|
| `fn` (function) | `def` (function) | `function` declaration | class |
| `struct` | `class` | `class` | interface |
| `impl` + methods | class methods | methods, constructor | enum |
| `trait` | | arrow function (`const fn = () =>`) | methods |
| `enum` | | `interface` | constructors |
| `const` | | `type` alias | |
| `mod` | | `export` functions/classes | |

## How It Works

```
Source Files ──▶ Tree-sitter Parser ──▶ SQLite Index ──▶ MCP Server ──▶ AI Assistant
                 (per language)         (per-project,     (stdio        (Claude, Cline,
                                        symbols +         JSON-RPC)     Cursor, etc.)
                                        imports + FTS)
```

1. **Scanner** walks the directory respecting `.gitignore`
2. **Parser** generates ASTs with Tree-sitter, extracts symbols and import statements
3. **Indexer** stores in SQLite with file hashes for incremental updates, FTS5 for text search
4. **MCP Server** serves 13 tools over stdio, auto-reindexes when stale

## Architecture

```
src/
├── main.rs           CLI entry point (index, search, list, clean, serve, watch)
├── config.rs         TOML configuration + per-project path resolution
├── error.rs          Error types
├── models/           Symbol, Import, Language, SymbolKind
├── scanner/          Directory walking with .gitignore
├── parser/           Tree-sitter parsers (Rust, Python, JS, TS, Java)
├── indexer/          SQLite storage, migrations, indexing pipeline
├── query/            Search, context, references, content, semantic, imports
├── watcher/          File change detection via notify
└── mcp_server/       MCP tool server with 13 tools
```

## Configuration

Each project stores its data locally in `.cortex/`:

```
<project>/
├── .cortex/
│   ├── bin/cortex          # Binary (local mode only)
│   ├── index.sqlite        # Symbol index database
│   └── reports/            # Task reports
├── .claude/
│   └── settings.local.json # MCP config (local mode only)
└── CLAUDE.md               # Cortex tool preferences (local mode only)
```

Global install stores the binary and project registry centrally:

```
~/.cortex/
├── config.toml       # Optional config overrides
├── projects.json     # Registry of indexed projects
~/.local/bin/
└── cortex            # Binary (global mode)
```

Default config (auto-generated at `~/.cortex/config.toml`):

```toml
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
