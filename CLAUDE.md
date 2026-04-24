# Cortex Project

## Cortex MCP Tools (MANDATORY)

This project ships the Cortex MCP server. You MUST use Cortex tools instead of Bash grep/find/rg for ALL code exploration. Do NOT fall back to Bash for searches when a Cortex tool can do the job.

### Mandatory tool mapping — ALWAYS use Cortex instead of:

| Instead of this (Bash) | ALWAYS use this (Cortex) |
|---|---|
| `grep -rn "pattern"` / `rg "pattern"` | `search_content` with the same pattern |
| `grep -rn "fn foo"` / searching for a symbol | `search_symbols` by name |
| Searching for a concept like "error handling" | `search_by_semantic` with the concept |
| Reading a function/class body | `get_code_context` with the symbol name |
| `grep -rn "import Foo"` / finding usages | `find_references` for the symbol |
| Reading a file to see its structure | `list_document_symbols` for that file |
| `find . -name "*.rs"` / exploring layout | `list_files` or `list_directory_structure` |
| Understanding what imports what | `get_imports` for the file |

### When built-in tools ARE appropriate

- `Read` — reading a specific known file path in full
- `Edit` / `Write` — modifying or creating files
- `Bash` — running builds, tests, git commands, or non-search shell operations

### Rule of thumb

If the goal is "find", "search", "look up", "where is", "who uses", "what imports", or "show me the code for" — use Cortex. Only use Bash/Read when you already know the exact file path and just want to read or edit it.

Cortex auto-reindexes when source files change (30s cooldown). Call `index_project` if the index seems stale.
