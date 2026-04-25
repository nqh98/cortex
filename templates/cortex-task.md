Perform the following task using Cortex tools, then export a report: $ARGUMENTS

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
