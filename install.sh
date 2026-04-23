#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
IMAGE_NAME="cortex"
VOLUME_NAME="cortex-data"
PROJECT_PATH=""

# ── Arguments ──────────────────────────────────────────────────────────
usage() {
    echo "Usage: $0 [--project-path <path>]"
    echo ""
    echo "Installs Cortex as a Docker container and connects it to Claude via MCP."
    echo ""
    echo "Options:"
    echo "  --project-path  Path to the project directory to index (default: current directory)"
    exit 0
}

while [[ $# -gt 0 ]]; do
    case $1 in
        --project-path) PROJECT_PATH="$2"; shift 2 ;;
        -h|--help) usage ;;
        *) echo "Unknown option: $1"; usage ;;
    esac
done

PROJECT_PATH="$(cd "${PROJECT_PATH:-.}" && pwd)"

# ── Checks ─────────────────────────────────────────────────────────────
if ! command -v docker &>/dev/null; then
    echo "Error: docker is not installed. Install it first: https://docs.docker.com/get-docker/"
    exit 1
fi

if ! docker info &>/dev/null; then
    echo "Error: Docker daemon is not running. Start it first."
    exit 1
fi

if ! command -v jq &>/dev/null; then
    echo "Error: jq is not installed. Install it first:"
    echo "  Ubuntu/Debian: sudo apt install jq"
    echo "  macOS: brew install jq"
    exit 1
fi

# ── Step 1: Build Docker image ─────────────────────────────────────────
echo "==> Building Cortex Docker image..."
docker build -t "$IMAGE_NAME" "$SCRIPT_DIR"

# ── Step 2: Create persistence volume ──────────────────────────────────
if docker volume inspect "$VOLUME_NAME" &>/dev/null; then
    echo "==> Volume '$VOLUME_NAME' already exists, skipping."
else
    echo "==> Creating Docker volume '$VOLUME_NAME'..."
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
MCP_ENTRY=$(jq -n \
    --arg project_path "$PROJECT_PATH" \
    '{
        "command": "docker",
        "args": [
            "run", "--rm", "-i",
            "-v", "\($project_path):/project",
            "-v", "cortex-data:/home/cortex/.cortex",
            "cortex",
            "serve"
        ]
    }')

if [[ -f "$CONFIG_FILE" ]]; then
    # Merge into existing config, preserving other MCP servers and settings
    UPDATED=$(jq --argjson entry "$MCP_ENTRY" \
        '.mcpServers = (.mcpServers // {}) + {"cortex": $entry}' \
        "$CONFIG_FILE")
    echo "$UPDATED" > "$CONFIG_FILE"
else
    # Create new config
    jq -n --argjson entry "$MCP_ENTRY" \
        '{"mcpServers": {"cortex": $entry}}' \
        > "$CONFIG_FILE"
fi

# ── Done ───────────────────────────────────────────────────────────────
echo ""
echo "Cortex installed successfully."
echo ""
echo "  Image:    $IMAGE_NAME"
echo "  Volume:   $VOLUME_NAME"
echo "  Project:  $PROJECT_PATH"
echo "  Config:   $CONFIG_FILE"
echo ""
echo "Claude will now have access to Cortex MCP tools:"
echo "  - search_symbols"
echo "  - get_code_context"
echo "  - list_directory_structure"
echo ""
echo "Restart Claude for changes to take effect."
