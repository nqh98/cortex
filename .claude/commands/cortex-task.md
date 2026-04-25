Perform the following task using Cortex tools, then export a report: $ARGUMENTS

## Cortex Tool Rules (MANDATORY)

You MUST use Cortex tools instead of Bash grep/find/rg for ALL code exploration.

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

- `Read` — reading a specific known file path in full
- `Edit` / `Write` — modifying or creating files
- `Bash` — running builds, tests, git commands, or non-search shell operations

## Task

Do the work described in: "$ARGUMENTS"

Follow normal development practices — explore the codebase with Cortex tools, make changes, run tests/clippy/fmt as needed.

## Final Step — Export Report

When the task is complete, call `export_report` with:

- `project_root`: the project root directory
- `task_type`: one of `bug_fix`, `feature`, `refactoring`, `exploration`, `review`, or `other`
- `summary`: concise summary of what was done and why
- `tools_used`: list of Cortex tools actually used during this task
- `files_modified`: list of files that were changed
- `issues_found`: any bugs, errors, or problems discovered
- `improvement_suggestions`: actionable suggestions based on observations

Then confirm to the user with the report ID and file path.
