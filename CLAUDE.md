# Cortex Project

## Cortex MCP Tools

This project ships the Cortex MCP server. Prefer Cortex tools over raw grep/find for code exploration:

- `search_symbols` — find symbols by name
- `search_by_semantic` — find symbols by concept/keyword
- `get_code_context` — read full source for a symbol
- `list_document_symbols` — list all symbols in a file
- `find_references` — find all usages of a symbol
- `get_imports` — analyze import dependencies
- `search_content` — grep file contents by pattern
- `list_directory_structure` / `list_files` — explore project layout

### When to use Cortex vs built-in tools

- **Use Cortex** for: symbol lookups, finding references, understanding code structure, semantic search, dependency analysis
- **Use built-in tools** (Read, Edit, Write) for: reading/editing specific known files, writing new code

Cortex auto-reindexes when source files change (30s cooldown). Call `index_project` if the index seems stale.
