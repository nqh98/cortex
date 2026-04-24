#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
INSTALL_DIR="${HOME}/.local/bin"
CORTEX_DIR="${HOME}/.cortex"

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

usage() {
    cat <<EOF
Usage: $0 [OPTIONS]

Builds and installs Cortex locally.

OPTIONS:
  --install-dir <path>   Custom directory for binary installation (default: ~/.local/bin)
  -h, --help             Show this help message

After installation:
  cortex index /path/to/project   # Index a repo
  cortex search "handler"         # Search symbols
  cortex context get_parser       # Get symbol source
  cortex serve                    # Start MCP server
EOF
    exit 0
}

while [[ $# -gt 0 ]]; do
    case $1 in
        --install-dir) INSTALL_DIR="$2"; shift 2 ;;
        -h|--help) usage ;;
        *) log_error "Unknown option: $1"; usage ;;
    esac
done

# ── Checks ─────────────────────────────────────────────────────────────
if ! command -v cargo &>/dev/null; then
    log_error "Rust/Cargo is not installed. Install it first: https://rustup.rs"
    exit 1
fi

if ! command -v jq &>/dev/null; then
    log_error "jq is not installed. Install it first:"
    echo "  Ubuntu/Debian: sudo apt install jq"
    echo "  macOS: brew install jq"
    exit 1
fi

# ── Step 1: Build the binary ───────────────────────────────────────────
log_step "Building Cortex binary..."
cd "$SCRIPT_DIR"
cargo build --release 2>&1 | tail -1

# ── Step 2: Install binary ─────────────────────────────────────────────
mkdir -p "$INSTALL_DIR"
cp "$SCRIPT_DIR/target/release/cortex" "$INSTALL_DIR/cortex"
chmod +x "$INSTALL_DIR/cortex"
log_info "Binary installed to: $INSTALL_DIR/cortex"

if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    log_warn "$INSTALL_DIR is not in your PATH."
    echo ""
    echo "Add this to your shell profile (~/.bashrc, ~/.zshrc, etc.):"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
    echo ""
    echo "Then run: source ~/.bashrc (or ~/.zshrc)"
fi

# ── Step 3: Create ~/.cortex directory ─────────────────────────────────
mkdir -p "$CORTEX_DIR"
log_info "Config directory: $CORTEX_DIR"

# ── Step 4: Detect and configure MCP ───────────────────────────────────
log_step "Configuring MCP server..."

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
    --arg bin_path "$INSTALL_DIR/cortex" \
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

# ── Done ───────────────────────────────────────────────────────────────
echo ""
log_info "Cortex installed successfully!"
echo ""
echo "  Binary:    $INSTALL_DIR/cortex"
echo "  Data:      $CORTEX_DIR"
echo "  MCP:       cortex"
echo ""
echo "Usage — from any repository directory:"
echo "  cortex index .              # Index the repo"
echo "  cortex search \"handler\"     # Search symbols"
echo "  cortex context get_parser   # Get symbol source"
echo ""
echo "Restart Claude for MCP changes to take effect."
