#!/usr/bin/env bash
# Sync templates/ files into the embedded heredocs in install.sh.
# templates/ is the source of truth; install.sh embedded fallbacks are updated to match.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

sync_template() {
    local var_name="$1"       # e.g. CORTEX_PREFS
    local template_file="$2"  # e.g. templates/claude-md.md
    local label="$3"          # e.g. CLAUDE.md

    local template_path="$ROOT/$template_file"
    local install_sh="$ROOT/install.sh"

    if [[ ! -f "$template_path" ]]; then
        echo "Error: $template_path not found"
        exit 1
    fi

    # Read the template content and escape single quotes for bash heredoc
    # Template ' becomes '"'"' in the heredoc
    local content
    content=$(cat "$template_path")
    local escaped
    escaped=$(echo "$content" | sed "s/'/'\"'\"'/g")

    # Build the new fallback block
    local new_block
    new_block="if ! $var_name=\"\$(load_template $(basename "$template_file"))\"; then
    # Fallback: embedded copy for curl-piped installs (no templates/ on disk).
    # Keep in sync with $template_file — run: make sync-templates
    ${var_name}='${escaped}
'
fi"

    # Use Python to do the replacement (more reliable than sed for multiline)
    python3 -c "
import sys

with open('$install_sh') as f:
    content = f.read()

# Find the fallback block: starts with 'if ! VAR=' and ends with matching 'fi'
marker = 'if ! $var_name='
start = content.find(marker)
if start == -1:
    print('Error: could not find $var_name fallback block in install.sh', file=sys.stderr)
    sys.exit(1)

# Find the matching 'fi' — scan forward counting if/fi pairs
pos = start
depth = 0
end = -1
while pos < len(content):
    if content[pos:pos+3] == 'if ' or content[pos:pos+3] == \"if!\":
        depth += 1
        pos += 2
    elif content[pos:pos+2] == 'fi':
        depth -= 1
        if depth == 0:
            end = pos + 2
            break
        pos += 2
    else:
        pos += 1

if end == -1:
    print('Error: could not find end of $var_name fallback block', file=sys.stderr)
    sys.exit(1)

# Replace the block
new_content = content[:start] + '''$new_block''' + content[end:]

with open('$install_sh', 'w') as f:
    f.write(new_content)

print('Synced: $label -> install.sh ($var_name)')
"
}

sync_template "CORTEX_PREFS" "templates/claude-md.md" "CLAUDE.md template"
sync_template "CORTEX_TASK_SKILL" "templates/cortex-task.md" "cortex-task template"
