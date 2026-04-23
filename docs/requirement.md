Technical Specification: "Locus" - Local-First Code Context Engine
1. Project Overview
Locus is a high-performance, local-first background service written in Rust. It indexes local source code, extracts semantic structures using Tree-sitter, and provides a queryable interface to LLMs (specifically Claude) via the Model Context Protocol (MCP).

Core Objectives:
Low Latency: Sub-millisecond retrieval of code symbols.

Privacy: All indexing and data storage remain on the local machine.

Intelligence: Understands code structure (classes, functions) rather than just raw text.

2. Technical Stack
Language: Rust (Latest Stable)

Async Runtime: tokio

Code Parsing: tree-sitter & tree-sitter-languages

Storage: * Metadata: SQLite (via sqlx) for symbol relationships.

Vector Search: lancedb (Local-first vector database).

File Watching: notify crate.

Protocol: mcp-rust-sdk (Model Context Protocol).

3. System Architecture
A. The Ingestion Pipeline
Scanner: Walks the directory tree, respecting .gitignore (using the ignore crate).

Parser: Uses Tree-sitter to generate an AST (Abstract Syntax Tree) for each file.

Extractor: Identifies "Symbols" (Function definitions, Structs, Impls, Constants).

Indexer: * Saves symbol location (file, line, col) to SQLite.

Generates embeddings for code snippets and saves them to LanceDB.

B. The MCP Server Layer
Acts as a JSON-RPC bridge between Claude and the Rust backend. It exposes "Tools" that the AI can call dynamically.

4. Database Schema (SQLite)
SQL
CREATE TABLE symbols (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL, -- e.g., 'function', 'struct', 'module'
    file_path TEXT NOT NULL,
    start_line INTEGER,
    end_line INTEGER,
    signature TEXT,
    last_indexed TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_symbol_name ON symbols(name);
5. MCP Tool Definitions (API for AI)
The server must implement the following MCP tools:

search_symbols(query: string):

Performs a fuzzy search in SQLite or a vector search in LanceDB.

Returns: List of matching symbols with file paths and line numbers.

get_code_context(file_path: string, symbol_name: string):

Reads the specific file and extracts the implementation of the symbol.

Returns: The source code block.

list_directory_structure(path: string):

Returns a tree view of the project to help the AI understand the layout.

6. Implementation Roadmap for AI Agent
Phase 1: Project Setup & CLI
Initialize Rust project.

Implement CLI arguments using clap (e.g., locus serve --path ./my-project).

Phase 2: The Parser Module
Setup tree-sitter for Rust, Python, and JavaScript.

Create a logic to traverse the AST and extract symbol metadata.

Phase 3: Storage & Indexing
Implement SQLite integration for metadata.

Implement file watching to trigger incremental re-indexing on save.

Phase 4: MCP Integration
Implement the mcp-rust-sdk server.

Link MCP tool calls to the Storage queries.

7. Master Prompt for AI Implementation (Copy this)
"Act as a Senior Rust Engineer. Implement a local-first code context engine based on the provided technical spec.

Start by creating the Cargo.toml with tokio, sqlx, tree-sitter, and mcp-rust-sdk.

Implement a Parser module that can extract function signatures from Rust files using Tree-sitter.

Implement an MCP server that exposes a search_symbol tool.

Ensure the code is modular, uses Error handling with thiserror, and is highly performant.
Let's start with the Project Structure and Cargo.toml."