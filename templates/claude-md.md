# Cortex MCP Tools (MANDATORY)

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
