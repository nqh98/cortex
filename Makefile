.PHONY: build test check fmt clippy clean install run-index run-search run-serve run-watch docker-build docker-index docker-search docker-serve docker-watch docker-shell docker-clean help

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
	./install.sh --project-path "$(PWD)"

# Development helpers
run-index:
	cargo run -- index .

run-search:
	cargo run -- search "$(Q)"

run-serve:
	cargo run -- serve

run-watch:
	cargo run -- watch .

# Docker targets
docker-build:
	docker build -t cortex .

docker-index: docker-build
	docker run --rm -v $(PWD):/project -v cortex-data:/home/cortex/.cortex cortex index /project

docker-search: docker-build
	docker run --rm -v $(PWD):/project -v cortex-data:/home/cortex/.cortex cortex search "$(Q)"

docker-serve: docker-build
	docker run --rm -i -v $(PWD):/project -v cortex-data:/home/cortex/.cortex cortex serve

docker-watch: docker-build
	docker run --rm -v $(PWD):/project -v cortex-data:/home/cortex/.cortex cortex watch /project

docker-shell: docker-build
	docker run --rm -it -v $(PWD):/project -v cortex-data:/home/cortex/.cortex --entrypoint /bin/bash cortex

docker-clean:
	docker volume rm cortex-data || true

# Help target
help:
	@echo "Cortex Makefile"
	@echo ""
	@echo "Build & Test:"
	@echo "  make build          Build the Rust binary"
	@echo "  make test           Run all tests"
	@echo "  make check          Run cargo check"
	@echo "  make lint           Run fmt and clippy"
	@echo "  make clean          Clean build artifacts and cortex data"
	@echo ""
	@echo "Installation:"
	@echo "  make install        Install using install.sh"
	@echo ""
	@echo "Local Development:"
	@echo "  make run-index      Index current directory (cargo run)"
	@echo "  make run-search Q='query'  Search for symbols (cargo run)"
	@echo "  make run-serve      Start MCP server (cargo run)"
	@echo "  make run-watch      Watch current directory (cargo run)"
	@echo ""
	@echo "Docker (Easy Mode):"
	@echo "  make docker-build   Build Docker image"
	@echo "  make docker-index   Index current directory via Docker"
	@echo "  make docker-search Q='query'  Search via Docker"
	@echo "  make docker-serve   Start MCP server via Docker"
	@echo "  make docker-watch   Watch current directory via Docker"
	@echo "  make docker-shell   Open shell in Docker container"
	@echo "  make docker-clean   Remove Docker volume"
	@echo ""
	@echo "Wrapper Script (Recommended for Docker):"
	@echo "  ./cortex-docker.sh index ."
	@echo "  ./cortex-docker.sh search 'handler'"
	@echo "  ./cortex-docker.sh serve"
	@echo ""
	@echo "Docker Compose:"
	@echo "  docker compose run --rm cortex index /project"
	@echo "  docker compose run --rm cortex search 'handler'"
