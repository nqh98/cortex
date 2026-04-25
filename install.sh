#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Load a template file from templates/ directory.
# Returns content via stdout if the file exists, exits 1 otherwise.
# Used to avoid hardcoding templates when running from a cloned repo.
load_template() {
    local name="$1"
    local file_path="$SCRIPT_DIR/templates/${name}"
    if [[ -f "$file_path" ]]; then
        cat "$file_path"
    else
        return 1
    fi
}

MODE="global"
INSTALL_DIR=""
TARGET_DIR=""
CORTEX_DIR="${HOME}/.cortex"
BUILD_FROM_SOURCE=false
DOWNLOAD_URL=""

# GitHub releases base URL — update when you publish releases
RELEASES_URL="https://github.com/nqh98/cortex/releases"

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${GREEN}==>${NC} $*"; }
log_warn() { echo -e "${YELLOW}Warning:${NC} $*"; }
log_error() { echo -e "${RED}Error:${NC} $*"; }
log_step() { echo -e "${BLUE}==>${NC} $*"; }

detect_platform() {
    local os arch
    os="$(uname -s | tr '[:upper:]' '[:lower:]')"
    arch="$(uname -m)"
    # Normalize
    case "$os" in
        linux) os="linux" ;;
        darwin) os="macos" ;;
        *) log_error "Unsupported OS: $os"; exit 1 ;;
    esac
    case "$arch" in
        x86_64|amd64) arch="x86_64" ;;
        aarch64|arm64) arch="aarch64" ;;
        *) log_error "Unsupported architecture: $arch"; exit 1 ;;
    esac
    echo "${os}-${arch}"
}

usage() {
    cat <<EOF
Usage: $0 [MODE] [OPTIONS]

Cortex - Local-first code context engine with MCP integration.

MODES:
  global                Install globally (default)
                          Binary:   ~/.local/bin/cortex
                          Config:   ~/.cortex/
                          MCP:      registered in global Claude settings
                          CLAUDE.md: installed to ~/.claude/CLAUDE.md

  local <path>          Install into a target repository
                          Binary:   <path>/.cortex/bin/cortex
                          Config:   <path>/.cortex/config.toml
                          Index:    <path>/.cortex/index.sqlite
                          MCP:      registered in <path>/.claude/settings.local.json
                          CLAUDE.md: installed to <path>/CLAUDE.md

SOURCES (pick one):
  --url <url>           Download a prebuilt binary from URL
  --version <tag>       Download a prebuilt binary from GitHub releases (e.g. v0.2.0)
  --build               Build from source (requires Rust)
  (default)             Auto-detect: use prebuilt if available, fall back to source build

OPTIONS:
  --install-dir <path>  Custom directory for binary installation (global mode only)
  -h, --help            Show this help message

EXAMPLES:
  $0                                         # Global, auto-detect binary source
  $0 local .                                 # Install into current directory
  $0 --url https://example.com/cortex        # Use a specific prebuilt binary
  $0 --version v0.2.0                        # Download from GitHub releases
  $0 --build                                 # Force build from source

After installation:
  cortex index /path/to/project   # Index a repo
  cortex search "handler"         # Search symbols
  cortex context get_parser       # Get symbol source
  cortex serve                    # Start MCP server
EOF
    exit 0
}

# ── Parse arguments ────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
    case $1 in
        global) MODE="global"; shift ;;
        local)
            MODE="local"
            if [[ $# -lt 2 ]] || [[ "$2" == --* ]]; then
                log_error "local mode requires a target directory: $0 local <path>"
                exit 1
            fi
            TARGET_DIR="$(cd "$2" 2>/dev/null && pwd)" || {
                log_error "Directory not found: $2"
                exit 1
            }
            shift 2
            ;;
        --url) DOWNLOAD_URL="$2"; shift 2 ;;
        --version) DOWNLOAD_URL="RELEASE:$(detect_platform):$2"; shift 2 ;;
        --build) BUILD_FROM_SOURCE=true; shift ;;
        --install-dir) INSTALL_DIR="$2"; shift 2 ;;
        -h|--help) usage ;;
        *) log_error "Unknown option: $1"; usage ;;
    esac
done

# Set defaults based on mode
if [[ "$MODE" == "global" ]]; then
    INSTALL_DIR="${INSTALL_DIR:-${HOME}/.local/bin}"
    CORTEX_DIR="${HOME}/.cortex"
else
    INSTALL_DIR="${TARGET_DIR}/.cortex/bin"
    CORTEX_DIR="${TARGET_DIR}/.cortex"
fi

# ── Prerequisite checks ────────────────────────────────────────────────
if ! command -v jq &>/dev/null; then
    log_error "jq is not installed. Install it first:"
    echo "  Ubuntu/Debian: sudo apt install jq"
    echo "  macOS: brew install jq"
    exit 1
fi

HAS_RUST=false
if command -v cargo &>/dev/null; then
    HAS_RUST=true
fi

# ── Determine how to get the binary ────────────────────────────────────
BINARY_SOURCE=""   # "download" | "build" | "existing"

if [[ -n "$DOWNLOAD_URL" ]]; then
    if ! command -v curl &>/dev/null && ! command -v wget &>/dev/null; then
        log_error "curl or wget is required to download prebuilt binaries"
        exit 1
    fi
    BINARY_SOURCE="download"
elif [[ "$BUILD_FROM_SOURCE" == true ]]; then
    if [[ "$HAS_RUST" != true ]]; then
        log_error "Rust/Cargo is required for --build. Install it: https://rustup.rs"
        exit 1
    fi
    BINARY_SOURCE="build"
else
    # Auto-detect: prefer prebuilt if available, fall back to source build
    if command -v curl &>/dev/null || command -v wget &>/dev/null; then
        BINARY_SOURCE="download"
    elif [[ "$HAS_RUST" == true ]]; then
        BINARY_SOURCE="build"
    else
        log_error "Cannot install: no curl/wget for prebuilt binary, and no Rust for source build."
        echo ""
        echo "Install one of:"
        echo "  curl:     sudo apt install curl"
        echo "  Rust:     https://rustup.rs"
        exit 1
    fi
fi

# ── Step 1: Obtain the binary ──────────────────────────────────────────
BINARY_PATH=""

if [[ "$BINARY_SOURCE" == "download" ]]; then
    # Resolve the download URL
    if [[ "$DOWNLOAD_URL" == RELEASE:* ]]; then
        # Format: RELEASE:<platform>:<version>
        IFS=: read -r _ PLATFORM TAG <<< "$DOWNLOAD_URL"
        DOWNLOAD_URL="${RELEASES_URL}/download/${TAG}/cortex-${PLATFORM}.tar.gz"
    fi

    # If no explicit URL was given, try the latest release
    if [[ -z "$DOWNLOAD_URL" ]]; then
        PLATFORM="$(detect_platform)"
        DOWNLOAD_URL="${RELEASES_URL}/latest/download/cortex-${PLATFORM}.tar.gz"
    fi

    log_step "Downloading prebuilt binary..."
    echo "  URL: $DOWNLOAD_URL"

    TMP_DIR="$(mktemp -d)"
    trap 'rm -rf "$TMP_DIR"' EXIT

    if command -v curl &>/dev/null; then
        HTTP_CODE=$(curl -fsSL -w "%{http_code}" -o "$TMP_DIR/cortex.tar.gz" "$DOWNLOAD_URL" 2>/dev/null) || HTTP_CODE="000"
    else
        HTTP_CODE=$(wget -q -O "$TMP_DIR/cortex.tar.gz" "$DOWNLOAD_URL" 2>&1 | grep -c "saved" || echo "000")
        # wget doesn't return HTTP codes the same way; check if file exists and is non-empty
        if [[ -f "$TMP_DIR/cortex.tar.gz" ]] && [[ -s "$TMP_DIR/cortex.tar.gz" ]]; then
            HTTP_CODE="200"
        else
            HTTP_CODE="404"
        fi
    fi

    if [[ "$HTTP_CODE" == "200" ]]; then
        tar xzf "$TMP_DIR/cortex.tar.gz" -C "$TMP_DIR" 2>/dev/null || {
            # Maybe it's a raw binary, not a tarball
            mv "$TMP_DIR/cortex.tar.gz" "$TMP_DIR/cortex" 2>/dev/null
        }
        if [[ -f "$TMP_DIR/cortex" ]]; then
            BINARY_PATH="$TMP_DIR/cortex"
            log_info "Downloaded prebuilt binary"
        else
            log_error "Downloaded archive does not contain a 'cortex' binary"
            exit 1
        fi
    else
        # Download failed — fall back to source build if possible
        log_warn "Prebuilt binary not available (HTTP $HTTP_CODE)"
        if [[ "$HAS_RUST" == true ]]; then
            log_info "Falling back to source build..."
            BINARY_SOURCE="build"
        else
            log_error "Prebuilt binary download failed and Rust is not installed."
            echo "  Download: $DOWNLOAD_URL"
            echo "  Install Rust: https://rustup.rs"
            exit 1
        fi
    fi
fi

if [[ "$BINARY_SOURCE" == "build" ]]; then
    log_step "Building Cortex from source..."
    cd "$SCRIPT_DIR"
    cargo build --release 2>&1 | tail -1
    BINARY_PATH="$SCRIPT_DIR/target/release/cortex"
    log_info "Built from source"
fi

# ── Step 2: Install binary ─────────────────────────────────────────────
mkdir -p "$INSTALL_DIR"
cp "$BINARY_PATH" "$INSTALL_DIR/cortex"
chmod +x "$INSTALL_DIR/cortex"
log_info "Binary installed to: $INSTALL_DIR/cortex"

if [[ "$MODE" == "global" ]] && [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    log_warn "$INSTALL_DIR is not in your PATH."
    echo ""
    echo "Add this to your shell profile (~/.bashrc, ~/.zshrc, etc.):"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
fi

# ── Step 3: Create data directory ──────────────────────────────────────
mkdir -p "$CORTEX_DIR"
log_info "Config directory: $CORTEX_DIR"

# ── Step 4: Configure MCP ──────────────────────────────────────────────
log_step "Configuring MCP server..."

CORTEX_BIN="$INSTALL_DIR/cortex"

if [[ "$MODE" == "global" ]]; then
    # Global: register in Claude's global settings
    detect_claude_config() {
        local candidates=()
        candidates+=("$HOME/.claude/settings.json")
        candidates+=("$HOME/Library/Application Support/Claude/claude_desktop_config.json")
        candidates+=("$HOME/.config/claude/claude_desktop_config.json")

        for f in "${candidates[@]}"; do
            if [[ -f "$f" ]]; then
                echo "$f"
                return
            fi
        done
        echo "$HOME/.claude/settings.json"
    }

    CONFIG_FILE="$(detect_claude_config)"
    CONFIG_DIR="$(dirname "$CONFIG_FILE")"
    mkdir -p "$CONFIG_DIR"

    MCP_ENTRY=$(jq -n \
        --arg bin_path "$CORTEX_BIN" \
        '{
            "command": "\($bin_path)",
            "args": ["serve"]
        }')

    if [[ -f "$CONFIG_FILE" ]]; then
        UPDATED=$(jq --argjson entry "$MCP_ENTRY" --arg name "cortex" \
            '.mcpServers = (.mcpServers // {}) + {($name): $entry}' \
            "$CONFIG_FILE")
        echo "$UPDATED" > "$CONFIG_FILE"
    else
        jq -n --argjson entry "$MCP_ENTRY" --arg name "cortex" \
            '{"mcpServers": {($name): $entry}}' \
            > "$CONFIG_FILE"
    fi

    echo "  MCP config: $CONFIG_FILE"
else
    # Local: register in project's .claude/settings.local.json
    CLAUDE_DIR="${TARGET_DIR}/.claude"
    mkdir -p "$CLAUDE_DIR"
    CONFIG_FILE="${CLAUDE_DIR}/settings.local.json"

    MCP_ENTRY=$(jq -n \
        --arg bin_path "$CORTEX_BIN" \
        '{
            "command": "\($bin_path)",
            "args": ["serve"]
        }')

    if [[ -f "$CONFIG_FILE" ]]; then
        UPDATED=$(jq --argjson entry "$MCP_ENTRY" --arg name "cortex" \
            '.mcpServers = (.mcpServers // {}) + {($name): $entry}' \
            "$CONFIG_FILE")
        echo "$UPDATED" > "$CONFIG_FILE"
    else
        jq -n --argjson entry "$MCP_ENTRY" --arg name "cortex" \
            '{"mcpServers": {($name): $entry}}' \
            > "$CONFIG_FILE"
    fi

    echo "  MCP config: $CONFIG_FILE"
fi

# ── Step 5: Install Cortex tool preferences (CLAUDE.md) ────────────────
log_step "Installing Cortex tool preferences..."

if ! CORTEX_PREFS="$(load_template claude-md.md)"; then
    # Fallback: embedded copy for curl-piped installs (no templates/ on disk).
    # Keep in sync with templates/claude-md.md — run: make sync-templates
    CORTEX_PREFS='# Cortex MCP Tools (MANDATORY)

You should **prefer using Cortex tools** instead of Bash (`grep`, `rg`, `find`, etc.) for code exploration whenever possible.

### Tool Mapping

| Instead of this (Bash)                        | Prefer using this (Cortex)                 |
| --------------------------------------------- | ------------------------------------------ |
| `grep -rn "pattern"` / `rg "pattern"`         | `search_content` with the same pattern     |
| `grep -rn "fn foo"` / searching for a symbol  | `search_symbols` by name                   |
| Searching for a concept like "error handling" | `search_by_keyword` with the concept       |
| Reading a function/class body                 | `get_code_context` with the symbol name    |
| `grep -rn "import Foo"` / finding usages      | `find_references` for the symbol           |
| Reading a file to see its structure           | `list_document_symbols` for that file      |
| `find . -name "*.rs"` / exploring layout      | `list_files` or `list_directory_structure` |
| Understanding what imports what               | `get_imports` for the file                 |

### Usage Guidelines

* Use `search_symbols` for **exact symbol lookups**
* Use `search_content` for **literal text or patterns**
* Use `search_by_keyword` for **semantic or conceptual queries**
* Combine tools when needed (e.g., search → then `get_code_context`)
* Prefer structured exploration over raw text search when possible

### Fallback Rule

If a Cortex tool returns insufficient results, fails, or cannot express the query, you MAY fall back to Bash tools. Do NOT get stuck — proceed with best-effort completion.

### Allowed Tools

* `Read` — reading a specific known file path in full
* `Edit` / `Write` — modifying or creating files
* `Bash` — builds, tests, git commands, or fallback search when justified

Cortex auto-reindexes when source files change (30s cooldown). Call `index_project` if the index seems stale.
'
fi

if [[ "$MODE" == "global" ]]; then
    CLAUDE_MD="${HOME}/.claude/CLAUDE.md"
else
    CLAUDE_MD="${TARGET_DIR}/CLAUDE.md"
fi

mkdir -p "$(dirname "$CLAUDE_MD")"

if [[ -f "$CLAUDE_MD" ]]; then
    # Remove any existing Cortex preferences block, then append new one
    TEMP=$(mktemp)
    sed '/^# Cortex MCP Tools (MANDATORY)/,/^$/d' "$CLAUDE_MD" > "$TEMP"
    { cat "$TEMP"; echo ""; echo "$CORTEX_PREFS"; } > "$CLAUDE_MD"
    rm -f "$TEMP"
    log_info "Updated Cortex preferences in: $CLAUDE_MD"
else
    echo "$CORTEX_PREFS" > "$CLAUDE_MD"
    log_info "Created: $CLAUDE_MD"
fi

# ── Step 6: Install /cortex-task slash command ──────────────────────────
log_step "Installing /cortex-task slash command..."

if ! CORTEX_TASK_SKILL="$(load_template cortex-task.md)"; then
    # Fallback: embedded copy for curl-piped installs (no templates/ on disk).
    # Keep in sync with templates/cortex-task.md — run: make sync-templates
    CORTEX_TASK_SKILL='Perform the following task using Cortex tools, then export a report: $ARGUMENTS

## Cortex Tool Rules (MANDATORY)

You should **prefer using Cortex tools** instead of Bash (`grep`, `rg`, `find`, etc.) for code exploration whenever possible.

### Tool Mapping

| Instead of this (Bash)                        | Prefer using this (Cortex)                 |
| --------------------------------------------- | ------------------------------------------ |
| `grep -rn "pattern"` / `rg "pattern"`         | `search_content` with the same pattern     |
| `grep -rn "fn foo"` / searching for a symbol  | `search_symbols` by name                   |
| Searching for a concept like "error handling" | `search_by_keyword` with the concept       |
| Reading a function/class body                 | `get_code_context` with the symbol name    |
| `grep -rn "import Foo"` / finding usages      | `find_references` for the symbol           |
| Reading a file to see its structure           | `list_document_symbols` for that file      |
| `find . -name "*.rs"` / exploring layout      | `list_files` or `list_directory_structure` |
| Understanding what imports what               | `get_imports` for the file                 |

### Usage Guidelines

* Use `search_symbols` for **exact symbol lookups**
* Use `search_content` for **literal text or patterns**
* Use `search_by_keyword` for **semantic or conceptual queries**
* Combine tools when needed (e.g., search → then `get_code_context`)
* Prefer structured exploration over raw text search when possible

### Fallback Rule (Important)

If a Cortex tool:

* returns insufficient or irrelevant results
* fails to execute
* or cannot express the query

you MAY fallback to Bash tools, but you MUST:

* briefly explain why Cortex was insufficient
* include this as feedback in `issues_found` in the report

Do NOT get stuck if Cortex tools fail — proceed with best-effort completion.

### Allowed Tools

* `Read` — reading a specific known file path in full
* `Edit` / `Write` — modifying or creating files
* `Bash` — builds, tests, git commands, or fallback search when justified

---

## Task

Do the work described in: "$ARGUMENTS"

Follow normal development practices:

* Explore the codebase using Cortex tools when possible
* Make necessary changes
* Run tests / clippy / fmt as appropriate

---

## Final Step — Export Report (Cortex Feedback Only)

When the task is complete, call `export_report`.

This report is **strictly for evaluating Cortex tools**, NOT the codebase.

### Required fields:

* `project_root`: the project root directory
* `task_type`: one of `bug_fix`, `feature`, `refactoring`, `exploration`, `review`, or `other`
* `summary`: concise summary of what was done
* `model`: the AI model identifier (e.g., "gpt-4o", "claude-sonnet-4-6")
* `tools_used`: list of Cortex tools actually used (do not guess)
* `files_modified`: list of modified files

### Cortex Feedback Fields (Important)

Only include feedback about Cortex tools:

* `issues_found`: Problems using Cortex tools ONLY. Examples: missing or incomplete results, irrelevant matches, poor ranking or search quality, confusing errors or unclear outputs, inability to express certain queries.

* `improvement_suggestions`: Suggestions to improve Cortex tools ONLY. Examples: better filtering options, improved ranking, additional tool capabilities, missing metadata in results.

### Critical Rules

* Do NOT include codebase bugs or findings in the report
* Do NOT fabricate tool usage — only include tools actually used
* If unsure about a field, omit it rather than guessing
* Be specific and actionable in feedback

---

## Completion

After calling `export_report`, confirm to the user with:

* the report ID
* and the file path of the report
'
fi

if [[ "$MODE" == "global" ]]; then
    COMMANDS_DIR="${HOME}/.claude/commands"
else
    COMMANDS_DIR="${TARGET_DIR}/.claude/commands"
fi
mkdir -p "$COMMANDS_DIR"
echo "$CORTEX_TASK_SKILL" > "$COMMANDS_DIR/cortex-task.md"
log_info "Slash command installed: $COMMANDS_DIR/cortex-task.md"

# ── Step 7 (local mode): Initial index ─────────────────────────────────
if [[ "$MODE" == "local" ]]; then
    log_step "Running initial index..."
    "$CORTEX_BIN" index "$TARGET_DIR" || log_warn "Initial index failed (you can run it manually later)"
fi

# ── Done ───────────────────────────────────────────────────────────────
echo ""
log_info "Cortex installed successfully! (mode: $MODE, source: $BINARY_SOURCE)"
echo ""

if [[ "$MODE" == "global" ]]; then
    echo "  Binary:    $INSTALL_DIR/cortex"
    echo "  Data:      $CORTEX_DIR"
    echo "  MCP:       cortex (global)"
    echo "  Prefs:     ~/.claude/CLAUDE.md"
    echo "  Skill:     /cortex-task"
    echo ""
    echo "Usage — from any repository directory:"
    echo "  cortex index .              # Index the repo"
    echo "  cortex search \"handler\"     # Search symbols"
    echo "  cortex context get_parser   # Get symbol source"
else
    echo "  Binary:    $INSTALL_DIR/cortex"
    echo "  Data:      $CORTEX_DIR/"
    echo "  Index:     $CORTEX_DIR/index.sqlite"
    echo "  MCP:       cortex ($TARGET_DIR/.claude/settings.local.json)"
    echo "  Prefs:     $TARGET_DIR/CLAUDE.md"
    echo "  Skill:     /cortex-task"
    echo ""
    echo "Everything is self-contained in $TARGET_DIR/.cortex/"
    echo "To uninstall: rm -rf $TARGET_DIR/.cortex $TARGET_DIR/CLAUDE.md $TARGET_DIR/.claude/commands/cortex-task.md"
fi

echo ""
echo "Restart Claude for MCP changes to take effect."
