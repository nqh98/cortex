.PHONY: build test check fmt clippy clean install sync-templates run-index run-search run-serve run-watch help

build:
	cargo build --release

test:
	cargo test --all-features

check:
	cargo check --all-features

fmt:
	cargo fmt --all -- --check

clippy:
	cargo clippy --all-features -- -D warnings

lint: fmt clippy

clean:
	cargo clean
	rm -rf .cortex

ci: check test lint

install:
	./install.sh

sync-templates:
	@echo "Syncing embedded templates in install.sh from templates/..."
	./scripts/sync-templates.sh
	@echo "Done. Commit the changes to install.sh."

# Development helpers
run-index:
	cargo run -- index .

run-search:
	cargo run -- search "$(Q)"

run-serve:
	cargo run -- serve

run-watch:
	cargo run -- watch .

# Help target
help:
	@echo "Cortex Makefile"
	@echo ""
	@echo "Build & Test:"
	@echo "  make build          Build the Rust binary"
	@echo "  make test           Run all tests"
	@echo "  make check          Run cargo check"
	@echo "  make lint           Run fmt and clippy"
	@echo "  make clean          Clean build artifacts"
	@echo ""
	@echo "Installation:"
	@echo "  make install        Build and install to ~/.local/bin"
	@echo "  make sync-templates Update install.sh embedded templates from templates/"
	@echo ""
	@echo "Development:"
	@echo "  make run-index      Index current directory"
	@echo "  make run-search Q='query'  Search for symbols"
	@echo "  make run-serve      Start MCP server"
	@echo "  make run-watch      Watch current directory"
