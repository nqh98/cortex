.PHONY: build test check fmt clippy clean run-index run-search run-serve

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

# Development helpers
run-index:
	cargo run -- index .

run-search:
	cargo run -- search "$(Q)"

run-serve:
	cargo run -- serve

run-watch:
	cargo run -- watch .
