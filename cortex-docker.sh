#!/usr/bin/env bash
# ==========================================================================
# Cortex Docker Wrapper — Multi-repository support with data isolation
# Usage: cortex-docker <command> [args...]
# Commands: index, search, context, serve, watch, list, clean, help
# ==========================================================================

set -euo pipefail

IMAGE_NAME="${CORTEX_IMAGE:-cortex}"
DATA_VOLUME="${CORTEX_VOLUME:-cortex-data}"
PROJECT_PATH="${CORTEX_PROJECT:-$(pwd)}"
REPO_NAME=""
CORTEX_DIR="/home/cortex/.cortex"
REPOS_DIR="$CORTEX_DIR/repos"
CURRENT_REPO_FILE="$CORTEX_DIR/current"
TEMP_CONFIG_DIR="/tmp/cortex-config-$$"

# Cleanup temp config directory on exit
trap 'rm -rf "$TEMP_CONFIG_DIR"' EXIT

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${GREEN}==>${NC} $*"; }
log_warn() { echo -e "${YELLOW}Warning:${NC} $*"; }
log_error() { echo -e "${RED}Error:${NC} $*"; }
log_repo() { echo -e "${BLUE}[${REPO_NAME:-default}]${NC} $*"; }

# Check if Docker is available
check_docker() {
    if ! command -v docker &>/dev/null; then
        log_error "Docker is not installed. Install it first: https://docs.docker.com/get-docker/"
        exit 1
    fi

    if ! docker info &>/dev/null; then
        log_error "Docker daemon is not running. Start it first."
        exit 1
    fi
}

# Ensure data volume exists
ensure_volume() {
    if ! docker volume inspect "$DATA_VOLUME" &>/dev/null; then
        log_info "Creating Docker volume '$DATA_VOLUME'..."
        docker volume create "$DATA_VOLUME" >/dev/null
    fi
}

# Get repo name from path
get_repo_name() {
    local path="$1"
    basename "$(cd "$path" && pwd)"
}

# Set current repo
set_current_repo() {
    local repo_name="$1"
    check_docker
    ensure_volume
    docker run --rm -v "$DATA_VOLUME:$CORTEX_DIR" alpine sh -c "echo '$repo_name' > '$CURRENT_REPO_FILE'"
}

# Get current repo
get_current_repo() {
    check_docker
    ensure_volume
    docker run --rm -v "$DATA_VOLUME:$CORTEX_DIR" alpine sh -c "cat '$CURRENT_REPO_FILE' 2>/dev/null || echo ''"
}

# Get the path associated with a repo
get_repo_path() {
    local repo_name="$1"
    docker run --rm -v "$DATA_VOLUME:$CORTEX_DIR" alpine sh -c "cat '$REPOS_DIR/$repo_name/.path' 2>/dev/null || echo ''"
}

# Set the path for a repo
set_repo_path() {
    local repo_name="$1"
    local repo_path="$2"
    docker run --rm -v "$DATA_VOLUME:$CORTEX_DIR" alpine sh -c "echo '$repo_path' > '$REPOS_DIR/$repo_name/.path'"
}

# Check if repo exists
repo_exists() {
    local repo_name="$1"
    docker run --rm -v "$DATA_VOLUME:$CORTEX_DIR" alpine sh -c "[ -d '$REPOS_DIR/$repo_name' ]" 2>/dev/null
}

# Get repo data directory
get_repo_dir() {
    local repo_name="${1:-$REPO_NAME}"
    if [[ -z "$repo_name" ]]; then
        repo_name="$(get_current_repo)"
        if [[ -z "$repo_name" ]]; then
            log_error "No repository selected. Use --repo <name> or index a project first."
            exit 1
        fi
    fi
    echo "$REPOS_DIR/$repo_name"
}

# Generate repo-specific config file
generate_repo_config() {
    local repo_dir="$1"
    mkdir -p "$TEMP_CONFIG_DIR"
    cat > "$TEMP_CONFIG_DIR/config.toml" <<EOF
[database]
path = "$repo_dir/db.sqlite"

[indexing]
max_file_size_kb = 1024
supported_extensions = ["rs", "py", "js", "ts"]

[embeddings]
enabled = false
model = "AllMiniLML6V2"
batch_size = 32

[watcher]
debounce_ms = 500
EOF
    echo "$TEMP_CONFIG_DIR/config.toml"
}

# Run command in repo context
run_in_repo() {
    local repo_dir
    repo_dir="$(get_repo_dir "$REPO_NAME")"
    docker run --rm -v "$DATA_VOLUME:$CORTEX_DIR" alpine sh -c "mkdir -p '$repo_dir' && cd '$repo_dir' && $*"
}

# List all indexed repositories
list_repos() {
    check_docker
    ensure_volume

    local repos
    repos=$(docker run --rm -v "$DATA_VOLUME:$CORTEX_DIR" alpine sh -c "if [ -d '$REPOS_DIR' ]; then ls -1 '$REPOS_DIR' 2>/dev/null; fi" || true)

    if [[ -z "$repos" ]]; then
        log_info "No indexed repositories found."
        return
    fi

    local current
    current=$(get_current_repo)

    echo "Indexed repositories:"
    echo ""

    while IFS= read -r repo; do
        if [[ "$repo" == "$current" ]]; then
            echo "  * $repo (current)"
        else
            echo "    $repo"
        fi

        # Show path
        local repo_path
        repo_path=$(get_repo_path "$repo")
        if [[ -n "$repo_path" ]]; then
            echo "      Path: $repo_path"
        fi

        # Check if database exists
        local db_exists
        db_exists=$(docker run --rm -v "$DATA_VOLUME:$CORTEX_DIR" alpine sh -c \
            "if [ -f '$REPOS_DIR/$repo/db.sqlite' ]; then echo 'yes'; else echo 'no'; fi")

        if [[ "$db_exists" == "yes" ]]; then
            echo "      Status: Indexed"
        else
            echo "      Status: Incomplete"
        fi
        echo ""
    done <<< "$repos"
}

# Run Cortex in Docker
run_cortex() {
    local cmd="$1"
    shift

    check_docker
    ensure_volume

    local repo_dir
    repo_dir="$(get_repo_dir)"

    # Ensure repo directory exists with correct permissions
    # Note: $CORTEX_DIR is the mount point, so we create relative to it
    docker run --rm -v "$DATA_VOLUME:$CORTEX_DIR" alpine sh -c "mkdir -p '$repo_dir' && chown -R 1000:1000 '$repo_dir'"

    # Generate repo-specific config
    local config_file
    config_file="$(generate_repo_config "$repo_dir")"

    docker run --rm \
        -v "$PROJECT_PATH:/project" \
        -v "$DATA_VOLUME:$CORTEX_DIR" \
        -v "$config_file:/project/config.toml:ro" \
        "$IMAGE_NAME" "$cmd" "$@"
}

# Show usage
usage() {
    cat <<'EOF'
Cortex Docker Wrapper — Multi-repository support with data isolation

USAGE:
    cortex-docker <command> [options] [args...]

GLOBAL OPTIONS:
    -r, --repo <name>    Specify which repository to use
    --all                Operate on all repositories (for clean command)

COMMANDS:
    index <path>         Index a project directory
                          Options: --name <name>  Custom repository name
    search <query>       Search for symbols in current repo
                          Options: --kind <type>  Filter by symbol type
    context <symbol>     Get source code for a symbol
    serve                Start MCP server for current repo
    watch <path>         Watch a directory for changes
    list, repos          List all indexed repositories
    clean                Remove repository data
                          Options: --repo <name>  Clean specific repo
                                   --all           Clean all repos
    shell                Open shell in container (debugging)
    help                 Show this help message

ENVIRONMENT VARIABLES:
    CORTEX_IMAGE         Docker image name (default: cortex)
    CORTEX_VOLUME        Docker volume name (default: cortex-data)
    CORTEX_PROJECT       Project path to mount (default: current directory)

EXAMPLES:
    # Index a repository (uses directory name as repo name)
    cortex-docker index /path/to/project

    # Index with custom name
    cortex-docker index /path/to/project --name my-api

    # List all indexed repositories
    cortex-docker list

    # Search in current repo
    cortex-docker search "handler"

    # Search in specific repo
    cortex-docker search "handler" --repo my-api

    # Get context from current repo
    cortex-docker context get_parser

    # Clean specific repository
    cortex-docker clean --repo my-api

    # Clean all repositories
    cortex-docker clean --all

NOTES:
    - Each repository is stored with strict data isolation
    - The most recently indexed repository becomes the default
    - Use --repo to work with a specific repository
    - The clean command without options prompts for confirmation

For more information, see: https://github.com/your-org/cortex
EOF
}

# Parse global options
REPO_NAME=""
CLEAN_ALL=false

# Parse arguments
PARSED_ARGS=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        -r|--repo)
            REPO_NAME="$2"
            shift 2
            ;;
        --all)
            CLEAN_ALL=true
            shift
            ;;
        *)
            PARSED_ARGS+=("$1")
            shift
            ;;
    esac
done

set -- "${PARSED_ARGS[@]}"

# Parse commands
case "${1:-help}" in
    index)
        shift
        index_path="${1:-.}"
        custom_name=""
        should_overwrite=false

        # Parse index options
        while [[ $# -gt 0 ]]; do
            case "$1" in
                --name)
                    custom_name="$2"
                    shift 2
                    ;;
                --force|-f)
                    # Allow overwriting without confirmation
                    FORCE_INDEX=true
                    shift
                    ;;
                --help|-h)
                    run_cortex index --help
                    exit 0
                    ;;
                *)
                    shift
                    ;;
            esac
        done

        if [[ ! -d "$index_path" ]]; then
            log_error "Directory not found: $index_path"
            exit 1
        fi

        PROJECT_PATH="$(cd "$index_path" && pwd)"
        REPO_NAME="${custom_name:-$(get_repo_name "$index_path")}"

        # Check if repo already exists with different path
        if repo_exists "$REPO_NAME"; then
            existing_path="$(get_repo_path "$REPO_NAME")"
            if [[ -n "$existing_path" && "$existing_path" != "$PROJECT_PATH" ]]; then
                log_warn "Repository '$REPO_NAME' already exists at: $existing_path"
                log_warn "You're trying to index: $PROJECT_PATH"
                echo ""
                echo "Options:"
                echo "  1. Overwrite the existing repository (will lose data)"
                echo "  2. Use a different name with --name <name>"
                echo "  3. Cancel"
                echo ""

                if [[ "${FORCE_INDEX:-false}" == true ]]; then
                    log_info "Force mode enabled, overwriting repository..."
                    should_overwrite=true
                else
                    read -p "Choose an option [1/2/3]: " -n 1 -r
                    echo
                    case $REPLY in
                        1)
                            log_info "Overwriting repository '$REPO_NAME'..."
                            should_overwrite=true
                            ;;
                        2)
                            read -p "Enter new repository name: " new_name
                            if [[ -z "$new_name" ]]; then
                                log_error "Name cannot be empty"
                                exit 1
                            fi
                            REPO_NAME="$new_name"
                            log_info "Using new name: $REPO_NAME"
                            ;;
                        3|*)
                            log_warn "Cancelled."
                            exit 0
                            ;;
                    esac
                fi
            fi
        fi

        # If overwriting, clean the database first
        if [[ "$should_overwrite" == true ]]; then
            check_docker
            ensure_volume
            overwrite_repo_dir="$(get_repo_dir "$REPO_NAME")"
            docker run --rm -v "$DATA_VOLUME:$CORTEX_DIR" alpine sh -c "rm -f '$overwrite_repo_dir/db.sqlite'"
            log_info "Cleaned existing database."
        fi

        log_info "Indexing: $PROJECT_PATH"
        log_info "Repository: $REPO_NAME"

        run_cortex index /project
        set_current_repo "$REPO_NAME"
        set_repo_path "$REPO_NAME" "$PROJECT_PATH"
        log_info "Set as current repository."
        ;;
    search)
        if [[ -z "${2:-}" ]]; then
            log_error "search requires a query argument"
            exit 1
        fi
        if [[ "${2:-}" == "--help" ]] || [[ "${2:-}" == "-h" ]]; then
            run_cortex search --help
            exit 0
        fi
        run_cortex search "${@:2}"
        ;;
    context)
        if [[ "${2:-}" == "--help" ]] || [[ "${2:-}" == "-h" ]]; then
            run_cortex context --help
            exit 0
        fi
        if [[ -z "${2:-}" ]]; then
            log_error "context requires a symbol name"
            exit 1
        fi
        run_cortex context "${@:2}"
        ;;
    serve)
        run_cortex serve
        ;;
    watch)
        watch_path="${2:-.}"

        if [[ "$watch_path" == "--help" ]] || [[ "$watch_path" == "-h" ]]; then
            run_cortex watch --help
            exit 0
        fi

        if [[ ! -d "$watch_path" ]]; then
            log_error "Directory not found: $watch_path"
            exit 1
        fi

        PROJECT_PATH="$(cd "$watch_path" && pwd)"
        REPO_NAME="${REPO_NAME:-$(get_repo_name "$watch_path")}"

        log_info "Watching: $PROJECT_PATH"
        log_info "Repository: $REPO_NAME"

        run_cortex watch /project
        set_current_repo "$REPO_NAME"
        ;;
    list|repos)
        list_repos
        ;;
    shell)
        check_docker
        ensure_volume
        shell_repo_dir="$(get_repo_dir "$REPO_NAME")"
        shell_config_file="$(generate_repo_config "$shell_repo_dir")"
        log_info "Opening shell in Cortex container..."
        log_info "Repository: ${REPO_NAME:-$(get_current_repo)}"
        docker run --rm -it \
            -v "$PROJECT_PATH:/project" \
            -v "$DATA_VOLUME:$CORTEX_DIR" \
            -v "$shell_config_file:/project/config.toml:ro" \
            --entrypoint /bin/bash \
            "$IMAGE_NAME"
        ;;
    clean)
        check_docker

        if [[ "$CLEAN_ALL" == true ]]; then
            read -p "Remove ALL repositories? This will delete all indexed data. [y/N] " -n 1 -r
            echo
            if [[ $REPLY =~ ^[Yy]$ ]]; then
                log_info "Removing all repositories..."
                docker run --rm -v "$DATA_VOLUME:$CORTEX_DIR" alpine sh -c "rm -rf '$REPOS_DIR'/* '$CURRENT_REPO_FILE'"
                log_info "All repositories removed."
            else
                log_warn "Cancelled."
            fi
        elif [[ -n "$REPO_NAME" ]]; then
            read -p "Remove repository '$REPO_NAME'? This will delete all indexed data for this repo. [y/N] " -n 1 -r
            echo
            if [[ $REPLY =~ ^[Yy]$ ]]; then
                log_info "Removing repository '$REPO_NAME'..."
                docker run --rm -v "$DATA_VOLUME:$CORTEX_DIR" alpine sh -c "rm -rf '$REPOS_DIR/$REPO_NAME'"
                log_info "Repository removed."
            else
                log_warn "Cancelled."
            fi
        else
            read -p "Remove ALL repositories? This will delete all indexed data. [y/N] " -n 1 -r
            echo
            if [[ $REPLY =~ ^[Yy]$ ]]; then
                log_info "Removing all repositories..."
                docker run --rm -v "$DATA_VOLUME:$CORTEX_DIR" alpine sh -c "rm -rf '$REPOS_DIR'/* '$CURRENT_REPO_FILE'"
                log_info "All repositories removed."
            else
                log_warn "Cancelled."
            fi
        fi
        ;;
    help|--help|-h)
        usage
        ;;
    *)
        log_error "Unknown command: $1"
        echo
        usage
        exit 1
        ;;
esac
