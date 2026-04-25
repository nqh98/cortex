#!/usr/bin/env bash
# Verify that the embedded templates in install.sh match the files in templates/.
# Exits 1 with a diff if they have drifted.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

INSTALL_SH="$ROOT/install.sh"
STATUS=0

extract_embedded() {
    # Extract the embedded heredoc content for a variable from install.sh.
    # $1 = variable name (e.g. CORTEX_PREFS)
    # Outputs the extracted content to stdout.
    local var_name="$1"
    python3 - "$INSTALL_SH" "$var_name" <<'PYEOF'
import sys

install_sh_path = sys.argv[1]
var_name = sys.argv[2]

with open(install_sh_path) as f:
    content = f.read()

# Find the fallback block: "if ! VAR="$(load_template ...)"
marker = 'if ! ' + var_name + '="$(load_template'
idx = content.find(marker)
if idx == -1:
    print(f"Error: could not find {var_name} fallback block", file=sys.stderr)
    sys.exit(1)

# Find the opening single quote of the heredoc after the marker
start = content.find("'", idx)
if start == -1:
    sys.exit(1)
start += 1  # skip the opening quote

# Find the closing single quote that ends the heredoc.
# It's on its own line (or at end of line before \n) followed by newline and then 'fi'
# We scan forward from start, looking for a ' followed by \n    fi or \nfi
pos = start
while pos < len(content):
    q = content.find("'", pos)
    if q == -1:
        break
    # Check if this quote ends the heredoc: next non-whitespace should be on a new line leading to 'fi'
    after = content[q+1:]
    # The closing pattern is: '\n followed by optional spaces then fi
    if after.startswith("\n") and after.lstrip().startswith("fi"):
        # Extract the content between the quotes
        block = content[start:q]
        # Normalize bash single-quote escaping: '"'"' -> '
        block = block.replace("'\"'\"'", "'")
        # Strip the 4-space indent used inside the if block
        lines = []
        for line in block.split("\n"):
            if line.startswith("    "):
                lines.append(line[4:])
            else:
                lines.append(line)
        print("\n".join(lines))
        sys.exit(0)
    pos = q + 1

print(f"Error: could not find end of {var_name} heredoc", file=sys.stderr)
sys.exit(1)
PYEOF
}

check_template() {
    local var_name="$1"
    local template_file="$2"
    local label="$3"

    local template_path="$ROOT/$template_file"

    if [[ ! -f "$template_path" ]]; then
        echo "FAIL: $template_path not found"
        STATUS=1
        return
    fi

    local embedded
    if ! embedded=$(extract_embedded "$var_name"); then
        echo "FAIL: Could not extract $var_name from install.sh"
        STATUS=1
        return
    fi

    local template_content
    template_content=$(cat "$template_path")

    if [[ "$embedded" == "$template_content" ]]; then
        echo "OK:   $label is in sync"
    else
        echo "FAIL: $label has drifted between install.sh and $template_file"
        echo "  Run: make sync-templates"
        diff <(echo "$embedded") "$template_path" || true
        STATUS=1
    fi
}

check_template "CORTEX_PREFS" "templates/claude-md.md" "CLAUDE.md template"
check_template "CORTEX_TASK_SKILL" "templates/cortex-task.md" "cortex-task template"

exit $STATUS
