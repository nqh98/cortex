#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
IMAGE_NAME="cortex"
VOLUME_NAME="cortex-data"
PROJECT_PATH=""
INSTALL_WRAPPER=false
WRAPPER_INSTALL_DIR=""
PROJECT_NAME=""

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

# ── Arguments ──────────────────────────────────────────────────────────
usage() {
    cat <<EOF
Usage: $0 [OPTIONS]

Installs Cortex as a Docker container and connects it to Claude via MCP.

OPTIONS:
  --project-path <path>   Path to the project directory to index (default: current directory)
  --project-name <name>   Name for this project (used for volume naming, for multi-project setups)
  --install-wrapper       Install the cortex-docker.sh wrapper script to ~/.local/bin
  --wrapper-dir <path>    Custom directory for wrapper installation (default: ~/.local/bin)
  -h, --help              Show this help message

EXAMPLES:
  # Basic installation
  $0 --project-path /path/to/project

  # Multi-project setup with named project
  $0 --project-path /path/to/project1 --project-name project1

  # Install with wrapper script (recommended)
  $0 --project-path /path/to/project --install-wrapper

  # Full setup: multi-project + wrapper
  $0 --project-path /path/to/my-api --project-name my-api --install-wrapper
EOF
    exit 0
}

while [[ $# -gt 0 ]]; do
    case $1 in
        --project-path) PROJECT_PATH="$2"; shift 2 ;;
        --project-name) PROJECT_NAME="$2"; shift 2 ;;
        --install-wrapper) INSTALL_WRAPPER=true; shift ;;
        --wrapper-dir) WRAPPER_INSTALL_DIR="$2"; shift 2 ;;
        -h|--help) usage ;;
        *) log_error "Unknown option: $1"; usage ;;
    esac
done

PROJECT_PATH="$(cd "${PROJECT_PATH:-.}" && pwd)"
PROJECT_NAME="${PROJECT_NAME:-$(basename "$PROJECT_PATH")}"

# Use project name for volume if provided (for multi-project support)
if [[ "$PROJECT_NAME" != "." && "$PROJECT_NAME" != "$(basename "$PROJECT_PATH")" ]]; then
    VOLUME_NAME="cortex-data-${PROJECT_NAME}"
fi

# ── Checks ─────────────────────────────────────────────────────────────
if ! command -v docker &>/dev/null; then
    log_error "Docker is not installed. Install it first: https://docs.docker.com/get-docker/"
    exit 1
fi

if ! docker info &>/dev/null; then
    log_error "Docker daemon is not running. Start it first."
    exit 1
fi

if ! command -v jq &>/dev/null; then
    log_error "jq is not installed. Install it first:"
    echo "  Ubuntu/Debian: sudo apt install jq"
    echo "  macOS: brew install jq"
    exit 1
fi

# ── Install Wrapper Script (if requested) ─────────────────────────────
install_wrapper() {
    local wrapper_script="$SCRIPT_DIR/cortex-docker.sh"

    if [[ ! -f "$wrapper_script" ]]; then
        log_warn "Wrapper script not found at $wrapper_script. Skipping wrapper installation."
        return
    fi

    local install_dir="${WRAPPER_INSTALL_DIR:-$HOME/.local/bin}"
    local target_file="$install_dir/cortex-docker"

    # Create install directory if it doesn't exist
    mkdir -p "$install_dir"

    # Copy the wrapper script
    cp "$wrapper_script" "$target_file"
    chmod +x "$target_file"

    log_info "Wrapper script installed to: $target_file"

    # Check if install_dir is in PATH
    if [[ ":$PATH:" != *":$install_dir:"* ]]; then
        log_warn "$install_dir is not in your PATH."
        echo ""
        echo "Add this to your shell profile (~/.bashrc, ~/.zshrc, etc.):"
        echo "  export PATH=\"$install_dir:\$PATH\""
        echo ""
        echo "Then run: source ~/.bashrc (or ~/.zshrc)"
    else
        log_info "You can now run: cortex-docker <command>"
    fi
}

if [[ "$INSTALL_WRAPPER" == true ]]; then
    log_step "Installing wrapper script..."
    install_wrapper
    echo ""
fi

# ── Step 1: Build Docker image ─────────────────────────────────────────
log_step "Building Cortex Docker image..."
docker build -t "$IMAGE_NAME" "$SCRIPT_DIR" --quiet 2>/dev/null || docker build -t "$IMAGE_NAME" "$SCRIPT_DIR"

# ── Step 2: Create persistence volume ──────────────────────────────────
if docker volume inspect "$VOLUME_NAME" &>/dev/null; then
    log_info "Volume '$VOLUME_NAME' already exists, skipping."
else
    log_info "Creating Docker volume '$VOLUME_NAME'..."
    docker volume create "$VOLUME_NAME" >/dev/null
fi

# ── Step 3: Detect Claude config location ──────────────────────────────
detect_claude_config() {
    local candidates=()

    # Claude Code (CLI) — global settings
    candidates+=("$HOME/.claude/settings.json")

    # Claude Desktop — macOS
    candidates+=("$HOME/Library/Application Support/Claude/claude_desktop_config.json")

    # Claude Desktop — Linux
    candidates+=("$HOME/.config/claude/claude_desktop_config.json")

    for f in "${candidates[@]}"; do
        if [[ -f "$f" ]]; then
            echo "$f"
            return
        fi
    done

    # None found — create Claude Code config as default
    echo "$HOME/.claude/settings.json"
}

CONFIG_FILE="$(detect_claude_config)"
CONFIG_DIR="$(dirname "$CONFIG_FILE")"

echo "==> Using Claude config: $CONFIG_FILE"

# Create directory if needed
mkdir -p "$CONFIG_DIR"

# ── Step 4: Inject MCP server config ───────────────────────────────────
# Check if wrapper is installed and use it
WRAPPER_PATH="${WRAPPER_INSTALL_DIR:-$HOME/.local/bin}/cortex-docker"

if [[ -f "$WRAPPER_PATH" && -x "$WRAPPER_PATH" ]]; then
    # Use wrapper script
    MCP_ENTRY=$(jq -n \
        --arg wrapper_path "$WRAPPER_PATH" \
        --arg project_path "$PROJECT_PATH" \
        '{
            "command": "\($wrapper_path)",
            "args": ["serve"],
            "env": {
                "CORTEX_PROJECT": "\($project_path)",
                "CORTEX_VOLUME": "'"$VOLUME_NAME"'"
            }
        }')
else
    # Use docker command directly
    MCP_ENTRY=$(jq -n \
        --arg project_path "$PROJECT_PATH" \
        '{
            "command": "docker",
            "args": [
                "run", "--rm", "-i",
                "-v", "\($project_path):/project",
                "-v", "'"$VOLUME_NAME"':/home/cortex/.cortex",
                "'"$IMAGE_NAME"'",
                "serve"
            ]
        }')
fi

# Determine server name (use project name for multi-project)
MCP_SERVER_NAME="cortex"
if [[ "$VOLUME_NAME" != "cortex-data" ]]; then
    # Extract project name from volume name
    MCP_SERVER_NAME="cortex-${VOLUME_NAME#cortex-data-}"
fi

if [[ -f "$CONFIG_FILE" ]]; then
    # Merge into existing config, preserving other MCP servers and settings
    UPDATED=$(jq --argjson entry "$MCP_ENTRY" --arg name "$MCP_SERVER_NAME" \
        '.mcpServers = (.mcpServers // {}) + {($name): $entry}' \
        "$CONFIG_FILE")
    echo "$UPDATED" > "$CONFIG_FILE"
else
    # Create new config
    jq -n --argjson entry "$MCP_ENTRY" --arg name "$MCP_SERVER_NAME" \
        '{"mcpServers": {($name): $entry}}' \
        > "$CONFIG_FILE"
fi

# ── Done ───────────────────────────────────────────────────────────────
echo ""
log_info "Cortex installed successfully."
echo ""
echo "  Image:        $IMAGE_NAME"
echo "  Volume:       $VOLUME_NAME"
echo "  Project:      $PROJECT_PATH"
echo "  Project Name: $PROJECT_NAME"
echo "  MCP Server:   $MCP_SERVER_NAME"
echo "  Config:       $CONFIG_FILE"
echo ""

if [[ -f "$WRAPPER_PATH" && -x "$WRAPPER_PATH" ]]; then
    echo "Using wrapper script for MCP server."
    echo ""
fi

echo "Claude will now have access to Cortex MCP tools:"
echo "  - search_symbols"
echo "  - get_code_context"
echo "  - list_directory_structure"
echo ""

if [[ "$INSTALL_WRAPPER" == true ]]; then
    echo "You can also use the wrapper script directly:"
    echo "  cortex-docker index $PROJECT_PATH"
    echo "  cortex-docker search \"handler\""
    echo "  cortex-docker context src/main.rs main"
    echo ""
fi

echo "Restart Claude for changes to take effect."
